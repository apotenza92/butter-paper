import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  buildBundleManifest,
  parseEntrypointArguments,
  validateArchivePaths,
  validateBundleDirectory,
} from "./ubuntu24-gpu-qualification-bundle.mjs";

test("archive paths are relative, normalized, unique, and injection-safe", () => {
  assert.deepEqual(
    validateArchivePaths(["bundle/README.md", "bundle/runtime/node"]),
    ["bundle/README.md", "bundle/runtime/node"],
  );
  for (const unsafe of [
    "/etc/passwd",
    "../escape",
    "bundle/../escape",
    "bundle\\escape",
    "bundle//file",
    "-checkpoint-action=exec=sh payload",
    "bundle/file\nnext",
  ]) {
    assert.throws(() => validateArchivePaths([unsafe]), /unsafe archive path/);
  }
  assert.throws(
    () => validateArchivePaths(["bundle/file", "bundle/file"]),
    /duplicate archive path/,
  );
});

test("fixed entrypoint accepts one absolute fresh output path only", () => {
  assert.deepEqual(parseEntrypointArguments(["--output", "/tmp/bp-result"]), {
    output: "/tmp/bp-result",
  });
  for (const argv of [
    [],
    ["--output", "relative"],
    ["--output", "/tmp/out", "--extra"],
    ["--output", "/tmp/out; touch /tmp/pwned"],
    ["--output", "/tmp/out\nnext"],
  ]) {
    assert.throws(() => parseEntrypointArguments(argv), /fixed entrypoint/);
  }
});

test("directory validation rejects missing, changed, extra, and symlinked files", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "bp-u24-bundle-test-"));
  try {
    await mkdir(path.join(root, "runtime"));
    await writeFile(path.join(root, "RUN-QUALIFICATION.sh"), "exact\n", {
      mode: 0o755,
    });
    await writeFile(path.join(root, "README.md"), "sealed\n");
    const manifest = await buildBundleManifest(root, {
      entrypoint: "RUN-QUALIFICATION.sh",
      classification: "blocked-contract-mismatch",
      excludedPaths: ["BUNDLE-MANIFEST.json"],
    });
    assert.equal((await validateBundleDirectory(root, manifest)).file_count, 2);

    await writeFile(path.join(root, "README.md"), "tampered\n");
    await assert.rejects(
      validateBundleDirectory(root, manifest),
      /hash or byte count changed/,
    );
    await writeFile(path.join(root, "README.md"), "sealed\n");

    await writeFile(path.join(root, "extra"), "not sealed\n");
    await assert.rejects(
      validateBundleDirectory(root, manifest),
      /unsealed file/,
    );
    await rm(path.join(root, "extra"));

    await rm(path.join(root, "RUN-QUALIFICATION.sh"));
    await writeFile(path.join(root, "target"), "exact\n", { mode: 0o755 });
    await import("node:fs/promises").then(({ symlink }) =>
      symlink("target", path.join(root, "RUN-QUALIFICATION.sh")),
    );
    await assert.rejects(
      validateBundleDirectory(root, manifest),
      /symbolic link/,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("manifest bytes are deterministic for unchanged files", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "bp-u24-manifest-test-"));
  try {
    await writeFile(path.join(root, "RUN-QUALIFICATION.sh"), "#!/bin/sh\n", {
      mode: 0o755,
    });
    const options = {
      entrypoint: "RUN-QUALIFICATION.sh",
      classification: "blocked-contract-mismatch",
      excludedPaths: ["BUNDLE-MANIFEST.json"],
    };
    const left = await buildBundleManifest(root, options);
    const right = await buildBundleManifest(root, options);
    assert.equal(JSON.stringify(left), JSON.stringify(right));
    assert.equal(left.files[0].path, "RUN-QUALIFICATION.sh");
    assert.match(left.files[0].sha256, /^[a-f0-9]{64}$/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
