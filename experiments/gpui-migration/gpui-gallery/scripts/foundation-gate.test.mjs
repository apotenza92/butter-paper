import assert from "node:assert/strict";
import test from "node:test";

import {
  collectReachable,
  licenseIsForbiddenOnly,
  missingReleasePlatformFeatures,
  parseCargoLock,
  parseGitSource,
  sha256,
} from "./foundation-gate.mjs";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

test("parseGitSource requires and preserves exact revisions", () => {
  assert.deepEqual(
    parseGitSource("git+https://example.com/repo?rev=abc123#abc123"),
    { url: "https://example.com/repo", revision: "abc123", resolved: "abc123" },
  );
  assert.equal(parseGitSource("registry+https://example.com/index"), null);
});

test("parseCargoLock captures registry checksums and exact Git sources", () => {
  const packages = parseCargoLock(`
[[package]]
name = "registry-package"
version = "1.2.3"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "abc123"

[[package]]
name = "git-package"
version = "0.1.0"
source = "git+https://example.com/repo?rev=def456#def456"
`);
  assert.deepEqual(
    packages.get(
      "registry-package\u00001.2.3\u0000registry+https://github.com/rust-lang/crates.io-index",
    ),
    {
      name: "registry-package",
      version: "1.2.3",
      source: "registry+https://github.com/rust-lang/crates.io-index",
      checksum: "abc123",
    },
  );
  assert.equal(
    packages.get("git-package\u00000.1.0\u0000git+https://example.com/repo?rev=def456#def456")
      ?.checksum,
    null,
  );
});

test("sha256 binds license clarification evidence to exact contents", () => {
  assert.equal(
    sha256(Buffer.from("Apache evidence\n")),
    "0718b65f36dff092552ac803b8dede8c0a0f3cb92f942dd944389e67d5f1f9bb",
  );
});

test("licenseIsForbiddenOnly accepts a permissive OR branch", () => {
  assert.equal(licenseIsForbiddenOnly("GPL-3.0-or-later"), true);
  assert.equal(licenseIsForbiddenOnly("AGPL-3.0-only OR GPL-3.0-only"), true);
  assert.equal(licenseIsForbiddenOnly("Apache-2.0 OR GPL-2.0-only"), false);
  assert.equal(licenseIsForbiddenOnly("MIT OR Apache-2.0"), false);
});

test("collectReachable excludes development-only edges", () => {
  const metadata = {
    packages: [
      { id: "root", name: "root" },
      { id: "runtime", name: "runtime" },
      { id: "dev", name: "dev" },
    ],
    resolve: {
      root: "root",
      nodes: [
        {
          id: "root",
          deps: [
            { pkg: "runtime", dep_kinds: [{ kind: null }] },
            { pkg: "dev", dep_kinds: [{ kind: "dev" }] },
          ],
        },
        { id: "runtime", deps: [] },
        { id: "dev", deps: [] },
      ],
    },
  };
  assert.deepEqual(
    collectReachable(metadata).map((pkg) => pkg.name).sort(),
    ["root", "runtime"],
  );
});

test("release GPUI dependencies keep every selected Linux backend discoverable", () => {
  const manifest = readFileSync(resolve(import.meta.dirname, "..", "Cargo.toml"), "utf8");
  assert.deepEqual(
    missingReleasePlatformFeatures(manifest, {
      gpui: ["wayland", "x11"],
      gpui_platform: ["font-kit", "wayland", "x11"],
    }),
    [],
  );
});

test("foundation pins the accepted GPUI-CE source without a compatibility patch", () => {
  const galleryDirectory = resolve(import.meta.dirname, "..");
  const manifest = readFileSync(resolve(galleryDirectory, "Cargo.toml"), "utf8");
  const policy = JSON.parse(
    readFileSync(resolve(galleryDirectory, "foundation-policy.json"), "utf8"),
  );
  const expectedSource = "https://github.com/gpui-ce/gpui-ce";
  const expectedRevision = "c738623ffbcec2aeddc44a645cc6b74646d5cf97";
  const directPins = [
    ...manifest.matchAll(
      /(?:gpui|gpui_platform)\s*=\s*\{[^\n]*git\s*=\s*"([^"]+)"[^\n]*rev\s*=\s*"([0-9a-f]{40})"/g,
    ),
  ].map((match) => ({ source: match[1], revision: match[2] }));

  assert.equal(policy.gpuiSource, expectedSource);
  assert.equal(policy.gpuiRevision, expectedRevision);
  assert.equal("compatibilityPatch" in policy, false);
  assert.equal(directPins.length, 4);
  assert.deepEqual(
    directPins,
    Array.from({ length: 4 }, () => ({
      source: expectedSource,
      revision: expectedRevision,
    })),
  );
});

test("foundation explicitly permits only the reviewed pdfium-render 0.9.4 revision", () => {
  const galleryDirectory = resolve(import.meta.dirname, "..");
  const manifest = readFileSync(resolve(galleryDirectory, "Cargo.toml"), "utf8");
  const policy = JSON.parse(
    readFileSync(resolve(galleryDirectory, "foundation-policy.json"), "utf8"),
  );
  const source = "https://github.com/ajrcarey/pdfium-render";
  const revision = "6cee8b9a3951832ac0ff62ce4c32800278001cb8";
  assert.equal(policy.allowedGitSources[source], revision);
  assert.match(
    manifest,
    new RegExp(
      `pdfium-render = \\{ git = "${source}", rev = "${revision}", version = "=0\\.9\\.4", default-features = false, features = \\["pdfium_7881"\\]`,
    ),
  );
});
