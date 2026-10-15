//! `cargo xtask pdf-corpus DIR...`: checks Edit PDF Content against a local folder
//! of PDFs (alignment with PDFium, outlines on ink, edit round-trips, speed)
//! and writes a ranked report. PDFs are only read; online-only cloud files
//! and duplicates are skipped. Nothing here runs in CI.

use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{BufRead as _, BufReader, Read as _},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

use serde_json::Value;

use crate::{
    Args, Result, command, create_dir, pdfium, root, sha256, target::Target, target_dir, write,
};

pub fn run(args: &mut Args) -> Result {
    let pages = args.value("--pages")?.unwrap_or_else(|| "30".into());
    let edits = args.value("--edits")?.unwrap_or_else(|| "3".into());
    let limit = args
        .value("--limit")?
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| "--limit needs a number".to_string())
        })
        .transpose()?;
    let cores = std::thread::available_parallelism().map_or(4, usize::from);
    let jobs = args
        .value("--jobs")?
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| "--jobs needs a number".to_string())
        })
        .transpose()?
        .unwrap_or_else(|| (cores / 2).max(1))
        .max(1);
    let out = args
        .value("--out")?
        .map(PathBuf::from)
        .unwrap_or_else(|| target_dir().join("pdf-corpus"));
    let mut folders = Vec::new();
    while let Some(folder) = args.positional() {
        folders.push(PathBuf::from(folder));
    }
    args.finish()?;
    if folders.is_empty() {
        return Err("pdf-corpus needs at least one folder".into());
    }
    let started = Instant::now();

    let found = find_pdfs(&folders);
    let (mut documents, skipped) = local_unique(found.files);
    if let Some(limit) = limit {
        documents.truncate(limit);
    }
    eprintln!(
        "{} PDFs: {} to check, {} online-only skipped, {} duplicates skipped",
        found.count,
        documents.len(),
        skipped.online_only,
        skipped.duplicates
    );
    if documents.is_empty() {
        return Err("no local PDFs to check".into());
    }

    // Largest first, each to the least loaded shard.
    documents.sort_by_key(|document| std::cmp::Reverse(document.1));
    let jobs = jobs.min(documents.len());
    let mut shards = vec![(0u64, Vec::new()); jobs];
    for (path, size) in documents {
        let shard = shards
            .iter_mut()
            .min_by_key(|(load, _)| *load)
            .expect("at least one shard");
        shard.0 += size.max(1);
        shard.1.push(path);
    }
    if out.exists() {
        fs::remove_dir_all(&out).map_err(|error| format!("{}: {error}", out.display()))?;
    }
    create_dir(&out)?;

    let library = pdfium::fetch(Target::host()?)?.library;
    let test = build_test_binary()?;
    let mut children = Vec::new();
    for (index, (_, paths)) in shards.iter().enumerate() {
        let list = out.join(format!("shard-{index}.txt"));
        let mut text = String::new();
        for path in paths {
            text.push_str(&path.display().to_string());
            text.push('\n');
        }
        write(&list, text)?;
        let child = command(&test)
            .args([
                "corpus::pdf_corpus",
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads",
                "1",
            ])
            .env("BP_PDFIUM_LIBRARY", &library)
            .env("BP_CORPUS_LIST", &list)
            .env("BP_CORPUS_OUT", &out)
            .env("BP_CORPUS_PAGES", &pages)
            .env("BP_CORPUS_EDITS", &edits)
            .env("BP_CORPUS_THREADS", (cores / jobs).max(1).to_string())
            .stdout(Stdio::null())
            .spawn()
            .map_err(|error| format!("{}: {error}", test.display()))?;
        children.push(child);
    }
    let mut failed = 0;
    for mut child in children {
        if !child.wait().map_err(|error| error.to_string())?.success() {
            failed += 1;
        }
    }
    let report = report(
        &out,
        &found.folders,
        &skipped,
        started.elapsed().as_secs_f64(),
    )?;
    write(&out.join("report.md"), &report)?;
    eprintln!("\nReport: {}", out.join("report.md").display());
    if failed > 0 {
        eprintln!("{failed} shard(s) stopped early; their documents so far are in the report.");
    }
    Ok(())
}

struct Found {
    files: Vec<PathBuf>,
    count: usize,
    folders: Vec<String>,
}

fn find_pdfs(folders: &[PathBuf]) -> Found {
    let mut files = Vec::new();
    let mut stack = folders.to_vec();
    while let Some(folder) = stack.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
            {
                files.push(path);
            }
        }
    }
    files.sort();
    Found {
        count: files.len(),
        files,
        folders: folders
            .iter()
            .map(|folder| folder.display().to_string())
            .collect(),
    }
}

#[derive(Default)]
struct Skipped {
    online_only: usize,
    duplicates: usize,
}

/// Local files, once each: content stored only in the cloud is never read
/// (that would download it), and copies are recognised by size and their
/// first megabyte.
fn local_unique(files: Vec<PathBuf>) -> (Vec<(PathBuf, u64)>, Skipped) {
    let mut skipped = Skipped::default();
    let mut seen = HashSet::new();
    let mut kept = Vec::new();
    for path in files {
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if metadata.len() == 0 {
            continue;
        }
        if online_only(&metadata) {
            skipped.online_only += 1;
            continue;
        }
        let mut head = vec![0; 1 << 20];
        let Ok(read) = fs::File::open(&path).and_then(|mut file| file.read(&mut head)) else {
            continue;
        };
        head.truncate(read);
        if !seen.insert((metadata.len(), sha256(&head))) {
            skipped.duplicates += 1;
            continue;
        }
        kept.push((path, metadata.len()));
    }
    (kept, skipped)
}

#[cfg(target_os = "macos")]
fn online_only(metadata: &fs::Metadata) -> bool {
    use std::os::macos::fs::MetadataExt;
    // SF_DATALESS: a File Provider placeholder whose contents are in the cloud.
    metadata.st_flags() & 0x4000_0000 != 0
}

#[cfg(not(target_os = "macos"))]
fn online_only(_metadata: &fs::Metadata) -> bool {
    false
}

/// Builds the worker's tests in release once and returns the binary.
fn build_test_binary() -> Result<PathBuf> {
    let mut build = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    build
        .current_dir(root())
        .args([
            "test",
            "--release",
            "--package",
            "butter-paper",
            "--bin",
            "butter-paper-pdf-worker",
            "--no-run",
            "--message-format=json-render-diagnostics",
        ])
        .stdout(Stdio::piped());
    let mut child = build
        .spawn()
        .map_err(|error| format!("cargo test: {error}"))?;
    let mut executable = None;
    for line in BufReader::new(child.stdout.take().expect("piped"))
        .lines()
        .map_while(std::result::Result::ok)
    {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if message["reason"] == "compiler-artifact"
            && message["target"]["name"] == "butter-paper-pdf-worker"
            && message["profile"]["test"] == true
            && let Some(path) = message["executable"].as_str()
        {
            executable = Some(PathBuf::from(path));
        }
    }
    if !child.wait().map_err(|error| error.to_string())?.success() {
        return Err("building the worker tests failed".into());
    }
    executable.ok_or_else(|| "cargo did not report the worker test binary".into())
}

fn report(out: &Path, folders: &[String], skipped: &Skipped, seconds: f64) -> Result<String> {
    let mut documents = Vec::new();
    for entry in fs::read_dir(out)
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
        {
            let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
            documents.extend(
                text.lines()
                    .filter_map(|line| serde_json::from_str::<Value>(line).ok()),
            );
        }
    }
    let number = |document: &Value, key: &str| document[key].as_f64().unwrap_or(0.);
    let count = |document: &Value, key: &str| document[key].as_array().map_or(0, Vec::len);
    let total = |key: &str| {
        documents
            .iter()
            .map(|document| number(document, key))
            .sum::<f64>()
    };
    let name = |document: &Value| {
        let path = document["path"].as_str().unwrap_or_default();
        folders
            .iter()
            .find_map(|folder| path.strip_prefix(folder.as_str()))
            .unwrap_or(path)
            .trim_start_matches('/')
            .to_owned()
    };
    let percent = |part: f64, whole: f64| {
        if whole > 0. {
            format!("{:.1}%", 100. * part / whole)
        } else {
            "n/a".into()
        }
    };

    let mut md = String::from("# PDF corpus report\n\n");
    md.push_str(&format!(
        "- {} documents ({} pages, {} sampled) in {:.0} s; {} online-only and {} duplicate files skipped.\n",
        documents.len(),
        total("pages"),
        total("sampled"),
        seconds,
        skipped.online_only,
        skipped.duplicates
    ));
    md.push_str(&format!(
        "- Characters aligned with PDFium: {} of {} ({}).\n",
        total("aligned_chars"),
        total("checked_chars"),
        percent(total("aligned_chars"), total("checked_chars"))
    ));
    md.push_str(&format!(
        "- Outlined lines with no ink under them: {} of {} ({}).\n",
        total("inkless_lines"),
        total("ink_lines"),
        percent(total("inkless_lines"), total("ink_lines"))
    ));
    let failures = documents
        .iter()
        .map(|document| count(document, "edit_failures"))
        .sum::<usize>();
    md.push_str(&format!(
        "- Edit round-trips: {} tried, {} failed; {} no-op edits changed content bytes.\n",
        total("edits_tried"),
        failures,
        total("noop_changed")
    ));
    md.push_str(&format!(
        "- Editable lines: {} of {} ({}); one- or two-character fragments: {}.\n",
        total("editable_lines"),
        total("lines"),
        percent(total("editable_lines"), total("lines")),
        total("fragment_lines")
    ));
    let errors = documents
        .iter()
        .filter(|document| !document["error"].is_null())
        .count();
    let panics = documents
        .iter()
        .map(|document| count(document, "panics"))
        .sum::<usize>();
    md.push_str(&format!(
        "- Documents that failed to open: {errors}; panics or page errors: {panics}.\n"
    ));

    let section = |md: &mut String, title: &str, rows: Vec<String>| {
        md.push_str(&format!("\n## {title}\n\n"));
        if rows.is_empty() {
            md.push_str("None.\n");
        }
        for row in rows {
            md.push_str(&format!("- {row}\n"));
        }
    };
    let crops = |document: &Value| {
        document["crops"]
            .as_array()
            .map(|crops| {
                crops
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|crop| format!(" [crop](crops/{crop})"))
                    .collect::<String>()
            })
            .unwrap_or_default()
    };
    let pages = |document: &Value, key: &str| {
        document[key]
            .as_array()
            .into_iter()
            .flatten()
            .take(3)
            .map(|issue| {
                format!(
                    "p{} {}",
                    issue["page"],
                    issue["detail"].as_str().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    };

    let mut panicked = documents
        .iter()
        .flat_map(|document| {
            document["panics"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|panic| format!("{}: {panic}", name(document)))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    panicked.truncate(40);
    section(&mut md, "Panics and page errors", panicked);

    let mut misaligned = documents
        .iter()
        .filter(|document| count(document, "misaligned_pages") > 0)
        .collect::<Vec<_>>();
    misaligned.sort_by_key(|document| std::cmp::Reverse(count(document, "misaligned_pages")));
    section(
        &mut md,
        "Misaligned pages (our text vs PDFium's characters)",
        misaligned
            .iter()
            .take(30)
            .map(|document| {
                format!(
                    "{} — {}{}",
                    name(document),
                    pages(document, "misaligned_pages"),
                    crops(document)
                )
            })
            .collect(),
    );

    let mut inkless = documents
        .iter()
        .filter(|document| count(document, "inkless_pages") > 0)
        .collect::<Vec<_>>();
    inkless.sort_by_key(|document| std::cmp::Reverse(count(document, "inkless_pages")));
    section(
        &mut md,
        "Outlines over blank space",
        inkless
            .iter()
            .take(30)
            .map(|document| {
                format!(
                    "{} — {}{}",
                    name(document),
                    pages(document, "inkless_pages"),
                    crops(document)
                )
            })
            .collect(),
    );

    let mut reasons: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for document in &documents {
        for failure in document["edit_failures"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let reason = failure
                .split_once(": ")
                .map_or(failure, |(_, reason)| reason);
            let reason = reason.split(':').next().unwrap_or(reason).to_owned();
            let entry = reasons.entry(reason).or_insert((0, name(document)));
            entry.0 += 1;
        }
    }
    let mut reasons = reasons.into_iter().collect::<Vec<_>>();
    reasons.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
    section(
        &mut md,
        "Edit round-trip failures",
        reasons
            .into_iter()
            .map(|(reason, (count, example))| format!("{count} × {reason} (e.g. {example})"))
            .collect(),
    );

    let mut opened: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for document in &documents {
        if let Some(error) = document["error"].as_str() {
            let key = error.chars().take(60).collect::<String>();
            opened.entry(key).or_insert((0, name(document))).0 += 1;
        }
    }
    section(
        &mut md,
        "Documents that did not open",
        opened
            .into_iter()
            .map(|(error, (count, example))| format!("{count} × {error} (e.g. {example})"))
            .collect(),
    );

    let mut read_only: BTreeMap<String, f64> = BTreeMap::new();
    for document in &documents {
        if let Some(reasons) = document["read_only"].as_object() {
            for (reason, lines) in reasons {
                *read_only.entry(reason.clone()).or_default() += lines.as_f64().unwrap_or(0.);
            }
        }
    }
    let mut read_only = read_only.into_iter().collect::<Vec<_>>();
    read_only.sort_by(|a, b| b.1.total_cmp(&a.1));
    section(
        &mut md,
        "Why lines cannot be edited",
        read_only
            .into_iter()
            .map(|(reason, lines)| format!("{lines} lines: {reason}"))
            .collect(),
    );

    let mut fragmented = documents
        .iter()
        .filter(|document| number(document, "editable_lines") >= 50.)
        .map(|document| {
            (
                number(document, "fragment_lines") / number(document, "editable_lines"),
                document,
            )
        })
        .filter(|(share, _)| *share > 0.2)
        .collect::<Vec<_>>();
    fragmented.sort_by(|a, b| b.0.total_cmp(&a.0));
    section(
        &mut md,
        "Most fragmented text (lines of one or two characters)",
        fragmented
            .iter()
            .take(15)
            .map(|(share, document)| {
                format!(
                    "{} — {:.0}% of editable lines",
                    name(document),
                    share * 100.
                )
            })
            .collect(),
    );

    let mut slow = documents.iter().collect::<Vec<_>>();
    slow.sort_by(|a, b| number(b, "extract_max_ms").total_cmp(&number(a, "extract_max_ms")));
    section(
        &mut md,
        "Slowest page reads",
        slow.iter()
            .take(10)
            .map(|document| {
                format!(
                    "{} — page {} took {:.0} ms ({} pages, load {:.0} ms)",
                    name(document),
                    document["extract_max_page"],
                    number(document, "extract_max_ms"),
                    document["pages"],
                    number(document, "load_ms")
                )
            })
            .collect(),
    );
    let mut slow_edits = documents
        .iter()
        .filter(|document| document["worker_edit_ms"].is_number())
        .collect::<Vec<_>>();
    slow_edits.sort_by(|a, b| number(b, "worker_edit_ms").total_cmp(&number(a, "worker_edit_ms")));
    section(
        &mut md,
        "Slowest edits shown (the worker applies, saves and reloads)",
        slow_edits
            .iter()
            .take(10)
            .map(|document| {
                format!(
                    "{} — {:.0} ms ({:.1} MB, {} pages)",
                    name(document),
                    number(document, "worker_edit_ms"),
                    number(document, "bytes") / 1e6,
                    document["pages"]
                )
            })
            .collect(),
    );
    Ok(md)
}
