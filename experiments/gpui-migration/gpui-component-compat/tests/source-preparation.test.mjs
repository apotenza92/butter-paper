import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  deterministicTreeDigest,
  validateCargoMetadata,
  validatePreparedManifest,
  validateSharedSourceReceipts,
} from "../scripts/source-preparation.mjs";

const revision = "8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc";
const policy = {
  zed: { url: "https://github.com/zed-industries/zed", revision },
  forbiddenFeatures: ["profiler", "runtime_shaders"],
  forbiddenPackages: ["ztracing_macro", "zlog"],
  replacementPackages: { ztracing: { license: "Apache-2.0", source: null } },
  allowedGitSources: { "https://github.com/zed-industries/zed": revision },
  licenseClarifications: {},
  rejectedLicenseExpressions: ["GPL-3.0-or-later"],
};

const preparedManifest = `
[workspace.dependencies]
gpui = { git = "https://github.com/zed-industries/zed", rev = "${revision}", default-features = false, features = ["wayland", "x11", "windows-manifest"] }
gpui_platform = { git = "https://github.com/zed-industries/zed", rev = "${revision}", default-features = false, features = ["font-kit", "wayland", "x11"] }
gpui_web = { git = "https://github.com/zed-industries/zed", rev = "${revision}" }
gpui_macros = { git = "https://github.com/zed-industries/zed", rev = "${revision}" }
reqwest_client = { git = "https://github.com/zed-industries/zed", rev = "${revision}" }

[patch.crates-io]
psm = { git = "https://github.com/rust-lang/stacker", rev = "100e77fa10a193f3c949af8e7f334c160e1424de" }
`;

test("prepared manifest pins every Zed input and removes forbidden features", () => {
  assert.doesNotThrow(() => validatePreparedManifest(preparedManifest, policy));

  assert.throws(
    () => validatePreparedManifest(preparedManifest.replace(`rev = "${revision}"`, ""), policy),
    /moving Git input|must pin every Zed dependency/,
  );
  assert.throws(
    () => validatePreparedManifest(preparedManifest.replace('features = ["font-kit", "wayland", "x11"]', 'features = ["font-kit", "runtime_shaders"]'), policy),
    /forbidden feature runtime_shaders/,
  );
  assert.throws(
    () => validatePreparedManifest(preparedManifest.replace('rev = "100e77fa10a193f3c949af8e7f334c160e1424de"', 'branch = "master"'), policy),
    /moving Git input/,
  );
});

test("resolved graph has one GPUI identity and no forbidden package or feature", () => {
  const source = `git+https://github.com/zed-industries/zed?rev=${revision}#${revision}`;
  const metadata = {
    packages: [
      { id: `gpui 0.2.2 (${source})`, name: "gpui", source, license: "Apache-2.0" },
      { id: "ztracing 0.1.0 (path+file:///shim)", name: "ztracing", source: null, license: "Apache-2.0" },
    ],
    resolve: {
      nodes: [
        { id: `gpui 0.2.2 (${source})`, features: ["wayland", "x11"] },
      ],
    },
  };

  assert.doesNotThrow(() => validateCargoMetadata(metadata, policy));
  assert.throws(
    () => validateCargoMetadata({ ...metadata, packages: [...metadata.packages, { id: "gpui duplicate", name: "gpui", source: "git+https://example.invalid/gpui" }] }, policy),
    /exactly one gpui package identity/,
  );
  assert.throws(
    () => validateCargoMetadata({ ...metadata, packages: [...metadata.packages, { id: "zlog", name: "zlog", source }] }, policy),
    /forbidden package zlog/,
  );
  assert.throws(
    () => validateCargoMetadata({ ...metadata, resolve: { nodes: [{ id: `gpui 0.2.2 (${source})`, features: ["profiler"] }] } }, policy),
    /resolved forbidden feature profiler/,
  );
  assert.throws(
    () => validateCargoMetadata({ ...metadata, packages: [...metadata.packages, { id: "moving", name: "moving", source: "git+https://example.invalid/moving#1111111111111111111111111111111111111111", license: "Apache-2.0" }] }, policy),
    /unapproved Git source/,
  );
  assert.throws(
    () => validateCargoMetadata({ ...metadata, packages: [...metadata.packages, { id: "unknown", name: "unknown", source: null, license: null, license_file: null }] }, policy),
    /missing license metadata/,
  );
  assert.throws(
    () => validateCargoMetadata({ ...metadata, packages: [...metadata.packages, { id: "copyleft", name: "copyleft", source: null, license: "GPL-3.0-or-later" }] }, policy),
    /rejected license/,
  );
  assert.doesNotThrow(
    () => validateCargoMetadata({ ...metadata, packages: [...metadata.packages, { id: "weak-copyleft", name: "weak-copyleft", source: null, license: "MIT OR Apache-2.0 OR LGPL-2.1-or-later" }] }, policy),
  );
});

test("prepared tree digest is independent of file creation order", async () => {
  const first = await mkdtemp(join(tmpdir(), "bp-prep-first-"));
  const second = await mkdtemp(join(tmpdir(), "bp-prep-second-"));
  await mkdir(join(first, "nested"));
  await writeFile(join(first, "nested", "b.txt"), "bravo\n");
  await writeFile(join(first, "a.txt"), "alpha\n");
  await writeFile(join(second, "a.txt"), "alpha\n");
  await mkdir(join(second, "nested"));
  await writeFile(join(second, "nested", "b.txt"), "bravo\n");

  assert.equal(await deterministicTreeDigest(first), await deterministicTreeDigest(second));
});

test("shared experiment source receipts reject path, checksum, and coverage drift", () => {
  const expected = [
    { path: "../gpui-gallery/src/pdf_worker.rs", sha256: "a".repeat(64) },
    { path: "../gpui-gallery/Cargo.toml", sha256: "b".repeat(64) },
  ];
  assert.doesNotThrow(() => validateSharedSourceReceipts(expected, expected));
  assert.throws(
    () => validateSharedSourceReceipts(expected, [expected[0]]),
    /shared experiment source receipt coverage drifted/,
  );
  assert.throws(
    () => validateSharedSourceReceipts(expected, [expected[0], { ...expected[1], sha256: "c".repeat(64) }]),
    /shared experiment source checksum drifted/,
  );
  assert.throws(
    () => validateSharedSourceReceipts(expected, [{ ...expected[0], path: "../other.rs" }, expected[1]]),
    /shared experiment source path drifted/,
  );
});
