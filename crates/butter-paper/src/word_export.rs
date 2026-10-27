//! Convert and Open in Word: Microsoft Word converts a PDF with its own
//! PDF import, saves the result beside the PDF and shows it. Butter Paper
//! has no converter of its own: Word's keeps paragraphs and tables, which
//! the free ones do not.

use std::path::{Path, PathBuf};

/// Where a PDF's Word document goes: beside it, with its name and a
/// `.docx` extension.
pub fn docx_path_for(pdf: &Path) -> PathBuf {
    pdf.with_file_name(format!("{}.docx", stem(pdf)))
}

/// Beside the PDF without replacing a file: [`docx_path_for`] while that is
/// free, then the first free `Name 2.docx`, `Name 3.docx`, ….
pub fn unused_docx_path_for(pdf: &Path) -> PathBuf {
    let stem = stem(pdf);
    std::iter::once(docx_path_for(pdf))
        .chain((2..).map(|n| pdf.with_file_name(format!("{stem} {n}.docx"))))
        .find(|path| !path.exists())
        .expect("an unused name exists")
}

fn stem(pdf: &Path) -> String {
    pdf.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document".to_owned())
}

/// Has Word convert `pdf`, save it as `docx` (replacing a file there) and
/// show it. When Word has `docx` open, that copy is closed first, unless it
/// has changes not yet saved. Blocks until Word has saved it (Word may take
/// a while to start).
pub fn convert_and_open(pdf: &Path, docx: &Path) -> Result<(), String> {
    platform::convert_and_open(pdf, docx)
}

#[cfg(target_os = "macos")]
mod platform {
    use std::path::Path;
    use std::process::Command;

    /// Word's AppleScript: open converts the PDF into a new document. Word
    /// cannot save over a document it has open, so an open copy of the
    /// Word document is closed first (its path is compared as Word gives
    /// it, an HFS path, worked out outside Word's `tell`).
    const SCRIPT: [&str; 17] = [
        "on run argv",
        "set pdfPath to item 1 of argv",
        "set docxPath to item 2 of argv",
        "set docxName to (POSIX file docxPath) as text",
        "tell application id \"com.microsoft.Word\"",
        "repeat with i from (count of documents) to 1 by -1",
        "if (full name of document i) is docxName then",
        "if not (saved of document i) then error \"BP_WORD_UNSAVED\" number 1001",
        "close document i saving no",
        "end if",
        "end repeat",
        "open POSIX file pdfPath",
        "set converted to active document",
        "save as converted file name docxPath file format format document",
        "activate",
        "end tell",
        "end run",
    ];

    pub fn convert_and_open(pdf: &Path, docx: &Path) -> Result<(), String> {
        let mut command = Command::new("/usr/bin/osascript");
        for line in SCRIPT {
            command.arg("-e").arg(line);
        }
        let output = command
            .arg(pdf)
            .arg(docx)
            .output()
            .map_err(|error| format!("Word could not be asked to convert the PDF: {error}"))?;
        if output.status.success() {
            return Ok(());
        }
        Err(super::failure(&String::from_utf8_lossy(&output.stderr)))
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::os::windows::process::CommandExt as _;
    use std::path::Path;
    use std::process::Command;

    /// Word's COM automation, in the Word already running when there is
    /// one (so its open copy of the Word document is seen and closed
    /// first): opening a PDF converts it, without asking first
    /// (`ConfirmConversions` off, alerts off).
    const SCRIPT: &str = "$ErrorActionPreference = 'Stop'; \
        try { $word = [Runtime.InteropServices.Marshal]::GetActiveObject('Word.Application') } \
        catch { $word = New-Object -ComObject Word.Application }; \
        $word.DisplayAlerts = 0; \
        $target = [IO.Path]::GetFullPath($env:BP_WORD_DOCX); \
        foreach ($open in @($word.Documents)) { \
            if ($open.FullName -ieq $target) { \
                if (-not $open.Saved) { throw 'BP_WORD_UNSAVED' }; \
                $open.Close(0) } }; \
        $document = $word.Documents.Open($env:BP_WORD_PDF, $false, $false, $false); \
        $document.SaveAs2($env:BP_WORD_DOCX, 16); \
        $word.DisplayAlerts = -1; \
        $word.Visible = $true; \
        $document.Activate(); \
        $word.Activate()";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn convert_and_open(pdf: &Path, docx: &Path) -> Result<(), String> {
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
            .env("BP_WORD_PDF", pdf)
            .env("BP_WORD_DOCX", docx)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|error| format!("Word could not be asked to convert the PDF: {error}"))?;
        if output.status.success() {
            return Ok(());
        }
        Err(super::failure(&String::from_utf8_lossy(&output.stderr)))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use std::path::Path;

    pub fn convert_and_open(_pdf: &Path, _docx: &Path) -> Result<(), String> {
        Err("Converting to Word needs Microsoft Word, which runs on macOS and Windows.".into())
    }
}

/// What went wrong, from Word's or the system's error output.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
fn failure(detail: &str) -> String {
    let detail = detail.trim();
    // AppleScript: no such application (-1728, -10814); not allowed to
    // control it (-1743). COM: class not registered (80040154).
    if ["-1728", "-10814", "80040154", "Can’t get application"]
        .iter()
        .any(|code| detail.contains(code))
    {
        return "Microsoft Word is needed to convert to Word, and it is not installed.".into();
    }
    if detail.contains("BP_WORD_UNSAVED") {
        return "The Word document is open in Word with changes not yet saved. Save or \
                close it there, then try again."
            .into();
    }
    if detail.contains("-1743") {
        return "Butter Paper is not allowed to control Microsoft Word. Allow it in System \
                Settings > Privacy & Security > Automation, then try again."
            .into();
    }
    if detail.is_empty() {
        "Word could not convert the PDF.".into()
    } else {
        format!("Word could not convert the PDF: {detail}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_word_document_goes_beside_the_pdf() {
        let directory = std::env::temp_dir().join(format!(
            "bp-word-export-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let pdf = directory.join("Initial report 15.10.26.pdf");
        assert_eq!(
            unused_docx_path_for(&pdf),
            directory.join("Initial report 15.10.26.docx")
        );
        std::fs::write(directory.join("Initial report 15.10.26.docx"), b"").unwrap();
        assert_eq!(
            docx_path_for(&pdf),
            directory.join("Initial report 15.10.26.docx")
        );
        assert_eq!(
            unused_docx_path_for(&pdf),
            directory.join("Initial report 15.10.26 2.docx")
        );
        std::fs::write(directory.join("Initial report 15.10.26 2.docx"), b"").unwrap();
        assert_eq!(
            unused_docx_path_for(&pdf),
            directory.join("Initial report 15.10.26 3.docx")
        );
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn failures_say_what_to_do() {
        assert!(
            failure("execution error: Can’t get application id \"com.microsoft.Word\". (-1728)")
                .contains("not installed")
        );
        assert!(
            failure("Not authorized to send Apple events to Microsoft Word. (-1743)")
                .contains("Automation")
        );
        assert!(failure("execution error: BP_WORD_UNSAVED (1001)").contains("Save or close it"));
        assert_eq!(failure(""), "Word could not convert the PDF.");
    }
}
