import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { link, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { stageNonMacProductionPdfium } from "../scripts/stage-nonmac-pdfium-production.mjs";

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const targets = [
  ["aarch64-pc-windows-msvc", "pdfium.dll", 0xaa64],
  ["x86_64-pc-windows-msvc", "pdfium.dll", 0x8664],
  ["aarch64-unknown-linux-gnu", "libpdfium.so", 183],
  ["x86_64-unknown-linux-gnu", "libpdfium.so", 62],
];

function pe(machine) {
  const bytes = Buffer.alloc(0x80 + 24 + 0xf0);
  bytes.write("MZ", 0, "ascii");
  bytes.writeUInt32LE(0x80, 0x3c);
  bytes.write("PE\0\0", 0x80, "ascii");
  bytes.writeUInt16LE(machine, 0x84);
  bytes.writeUInt16LE(0xf0, 0x94);
  bytes.writeUInt16LE(0x2022, 0x96);
  bytes.writeUInt16LE(0x20b, 0x98);
  return bytes;
}

function elf(machine) {
  const bytes = Buffer.alloc(64);
  bytes.set([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1], 0);
  bytes.writeUInt16LE(3, 16);
  bytes.writeUInt16LE(machine, 18);
  bytes.writeUInt32LE(1, 20);
  bytes.writeUInt16LE(64, 52);
  return bytes;
}

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "bp-nonmac-pdfium-"));
  const artifacts = join(root, "artifacts");
  const files = new Map([
    ["reviews/redistribution.txt", Buffer.from("reviewed redistribution grant\n")],
    ["reviews/supplier.txt", Buffer.from("reviewed supplier terms\n")],
  ]);
  for (let index = 0; index < targets.length; index += 1) {
    const [target, library, machine] = targets[index];
    const directory = `target-${index}`;
    files.set(`${directory}/${library}`, target.includes("windows") ? pe(machine) : elf(machine));
    files.set(`${directory}/sbom.json`, Buffer.from('{"bomFormat":"CycloneDX"}\n'));
    files.set(`${directory}/provenance.json`, Buffer.from('{"builder":"approved-offline-builder"}\n'));
    files.set(`${directory}/args.gn`, Buffer.from("pdf_enable_v8 = false\npdf_enable_xfa = false\nis_debug = false\n"));
    files.set(`notices/${directory}/LICENSE`, Buffer.from(`PDFium notice for ${target}\n`));
    files.set(`notices/${directory}/third_party/NOTICE`, Buffer.from("Third-party notice\n"));
  }
  for (const [path, bytes] of files) {
    await mkdir(join(artifacts, path, ".."), { recursive: true });
    await writeFile(join(artifacts, path), bytes);
  }
  const record = (path) => ({ path, bytes: files.get(path).length, sha256: sha256(files.get(path)) });
  const manifest = {
    schemaVersion: 1,
    purpose: "production-distribution",
    productionApproved: true,
    wrapper: {
      package: "pdfium-render",
      version: "0.9.4",
      revision: "6cee8b9a3951832ac0ff62ce4c32800278001cb8",
      feature: "pdfium_7881",
    },
    source: { repository: "https://pdfium.googlesource.com/pdfium", revision: "91b9d569b34be4f38eed7b3c49b227356c3aadad" },
    build: { apiBuild: 7881, v8: false, xfa: false, debug: false, sharedLibraryPatchSha256: "1e521b48561c51a63425baeec7c74c1edaf65956b6e6297d98aff57f2cc2ee40", dependencyPolicyPatchSha256: "54591df969f7a323c24f78e435340e8177081afc56dceaf1ec8102362a40601b", toolchain: { compiler: "reviewed cross toolchain 1.0" } },
    redistributionReview: record("reviews/redistribution.txt"),
    supplierReview: record("reviews/supplier.txt"),
    artifacts: targets.map(([target, library], index) => {
      const directory = `target-${index}`;
      const noticeDirectory = `notices/${directory}`;
      return {
        target,
        library: record(`${directory}/${library}`),
        sbom: record(`${directory}/sbom.json`),
        provenance: record(`${directory}/provenance.json`),
        gnArgs: record(`${directory}/args.gn`),
        noticeRoot: noticeDirectory,
        notices: [
          { ...record(`${noticeDirectory}/LICENSE`), path: "LICENSE" },
          { ...record(`${noticeDirectory}/third_party/NOTICE`), path: "third_party/NOTICE" },
        ],
      };
    }),
  };
  const manifestPath = join(root, "manifest.json");
  await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  return { root, artifacts, files, manifest, manifestPath };
}

test("stages each approved Windows and Linux production target with package-ready receipt and notices", async (t) => {
  const setup = await fixture();
  t.after(() => rm(setup.root, { recursive: true, force: true }));
  for (const [target, library] of targets) {
    const index = targets.findIndex(([candidate]) => candidate === target);
    const outputDirectory = join(setup.root, `stage-${index}`);
    const receipt = await stageNonMacProductionPdfium({ manifestPath: setup.manifestPath, artifactRoot: setup.artifacts, target, outputDirectory });
    const targetPrefix = target.includes("windows") ? "production-pdfium-windows" : "production-pdfium-linux";
    const architecture = target.startsWith("aarch64") ? "arm64" : "x86_64";
    assert.equal(receipt.target, target);
    const names = (await (await import("node:fs/promises")).readdir(outputDirectory)).sort();
    assert.deepEqual(names, [library, "THIRD_PARTY_NOTICES.md", `${targetPrefix}-${architecture}.json`].sort());
    assert.equal(receipt.library.path, library);
    assert.equal(receipt.library.sha256, setup.manifest.artifacts[index].library.sha256);
    assert.equal(receipt.build.provenance, `sha256:${setup.manifest.artifacts[index].provenance.sha256}`);
    assert.equal(receipt.redistributionReview.reference, `sha256:${setup.manifest.redistributionReview.sha256}`);
    assert.equal((await readFile(join(outputDirectory, "THIRD_PARTY_NOTICES.md"), "utf8")).includes("third_party/NOTICE"), true);
    assert.deepEqual(JSON.parse(await readFile(join(outputDirectory, `${targetPrefix}-${architecture}.json`), "utf8")), receipt);
  }
});

test("accepts the combined six-target approval manifest for non-macOS staging", async (t) => {
  const setup = await fixture();
  t.after(() => rm(setup.root, { recursive: true, force: true }));
  const mac = structuredClone(setup.manifest.artifacts[0]);
  mac.target = "aarch64-apple-darwin";
  mac.library.path = "target-0/libpdfium.dylib";
  setup.manifest.artifacts.push(mac);
  await writeFile(setup.manifestPath, `${JSON.stringify(setup.manifest, null, 2)}\n`);
  await assert.doesNotReject(stageNonMacProductionPdfium({
    manifestPath: setup.manifestPath,
    artifactRoot: setup.artifacts,
    target: targets[0][0],
    outputDirectory: join(setup.root, "combined-manifest"),
  }));
});

test("rejects unapproved manifests, altered bytes and binary architecture mismatches", async (t) => {
  const setup = await fixture();
  t.after(() => rm(setup.root, { recursive: true, force: true }));
  setup.manifest.productionApproved = false;
  await writeFile(setup.manifestPath, JSON.stringify(setup.manifest));
  await assert.rejects(stageNonMacProductionPdfium({ manifestPath: setup.manifestPath, artifactRoot: setup.artifacts, target: targets[0][0], outputDirectory: join(setup.root, "unapproved") }), /not explicitly approved/);

  const fresh = await fixture();
  t.after(() => rm(fresh.root, { recursive: true, force: true }));
  await writeFile(join(fresh.artifacts, "target-0/pdfium.dll"), Buffer.from("altered"));
  await assert.rejects(stageNonMacProductionPdfium({ manifestPath: fresh.manifestPath, artifactRoot: fresh.artifacts, target: targets[0][0], outputDirectory: join(fresh.root, "hash-mismatch") }), /does not match its reviewed byte receipt/);

  const wrongMachine = await fixture();
  t.after(() => rm(wrongMachine.root, { recursive: true, force: true }));
  const wrongBytes = pe(0x8664);
  await writeFile(join(wrongMachine.artifacts, "target-0/pdfium.dll"), wrongBytes);
  wrongMachine.manifest.artifacts[0].library = { path: "target-0/pdfium.dll", bytes: wrongBytes.length, sha256: sha256(wrongBytes) };
  await writeFile(wrongMachine.manifestPath, JSON.stringify(wrongMachine.manifest));
  await assert.rejects(stageNonMacProductionPdfium({ manifestPath: wrongMachine.manifestPath, artifactRoot: wrongMachine.artifacts, target: targets[0][0], outputDirectory: join(wrongMachine.root, "wrong-machine") }), /PE machine does not match/);
});

test("rejects notice inventory changes and symlink or hard-linked inputs", async (t) => {
  const extra = await fixture();
  t.after(() => rm(extra.root, { recursive: true, force: true }));
  await writeFile(join(extra.artifacts, "notices/target-0/EXTRA"), "unreviewed\n");
  await assert.rejects(stageNonMacProductionPdfium({ manifestPath: extra.manifestPath, artifactRoot: extra.artifacts, target: targets[0][0], outputDirectory: join(extra.root, "extra-notice") }), /notice inventory has missing or extra files/);

  const linked = await fixture();
  t.after(() => rm(linked.root, { recursive: true, force: true }));
  const library = join(linked.artifacts, "target-0/pdfium.dll");
  const hardlinkPath = join(linked.root, "external-hardlink.dll");
  await link(library, hardlinkPath);
  await assert.rejects(stageNonMacProductionPdfium({ manifestPath: linked.manifestPath, artifactRoot: linked.artifacts, target: targets[0][0], outputDirectory: join(linked.root, "hardlink") }), /regular single-link file/);

  const symlinked = await fixture();
  t.after(() => rm(symlinked.root, { recursive: true, force: true }));
  const symlinkTarget = join(symlinked.root, "real.dll");
  await writeFile(symlinkTarget, symlinked.files.get("target-0/pdfium.dll"));
  await rm(join(symlinked.artifacts, "target-0/pdfium.dll"));
  await symlink(symlinkTarget, join(symlinked.artifacts, "target-0/pdfium.dll"));
  await assert.rejects(stageNonMacProductionPdfium({ manifestPath: symlinked.manifestPath, artifactRoot: symlinked.artifacts, target: targets[0][0], outputDirectory: join(symlinked.root, "symlink") }), /must not traverse a symlink/);
});

test("rejects malformed binary headers and GN arguments that enable V8, XFA or debug", async (t) => {
  const malformed = await fixture();
  t.after(() => rm(malformed.root, { recursive: true, force: true }));
  const invalid = Buffer.from("not a binary");
  await writeFile(join(malformed.artifacts, "target-0/pdfium.dll"), invalid);
  malformed.manifest.artifacts[0].library = { path: "target-0/pdfium.dll", bytes: invalid.length, sha256: sha256(invalid) };
  await writeFile(malformed.manifestPath, JSON.stringify(malformed.manifest));
  await assert.rejects(stageNonMacProductionPdfium({ manifestPath: malformed.manifestPath, artifactRoot: malformed.artifacts, target: targets[0][0], outputDirectory: join(malformed.root, "bad-header") }), /not a valid PE DLL/);

  const badArgs = await fixture();
  t.after(() => rm(badArgs.root, { recursive: true, force: true }));
  const bytes = Buffer.from("pdf_enable_v8 = true\npdf_enable_xfa = false\nis_debug = false\n");
  await writeFile(join(badArgs.artifacts, "target-0/args.gn"), bytes);
  badArgs.manifest.artifacts[0].gnArgs = { path: "target-0/args.gn", bytes: bytes.length, sha256: sha256(bytes) };
  await writeFile(badArgs.manifestPath, JSON.stringify(badArgs.manifest));
  await assert.rejects(stageNonMacProductionPdfium({ manifestPath: badArgs.manifestPath, artifactRoot: badArgs.artifacts, target: targets[0][0], outputDirectory: join(badArgs.root, "bad-policy") }), /GN args do not enforce/);
});
