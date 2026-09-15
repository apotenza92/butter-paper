import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  deterministicTreeDigest,
  fileSha256,
  validateCargoMetadata,
  validatePreparedManifest,
  validateSharedSourceReceipts,
} from "../scripts/source-preparation.mjs";

const revision = "8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc";

test("tab button-state exception is separate, opt-in, and delegates to stock Button colours", async () => {
  const policy = JSON.parse(await readFile(new URL("../source-preparation-policy.json", import.meta.url), "utf8"));
  const patch = await readFile(new URL(`../${policy.tabStatePatch.path}`, import.meta.url), "utf8");
  assert.equal(await fileSha256(new URL(`../${policy.tabStatePatch.path}`, import.meta.url)), policy.tabStatePatch.sha256);
  assert.deepEqual(patch.split("diff --git ").slice(1).map(diff => diff.split("\n")[0]), [
    "a/crates/ui/src/button/button.rs b/crates/ui/src/button/button.rs",
    "a/crates/ui/src/tab/tab.rs b/crates/ui/src/tab/tab.rs",
  ]);
  assert.match(patch, /button_states: false/);
  assert.match(patch, /self.button_states && self.variant == TabVariant::Outline/);
  for (const state of ["normal", "selected", "disabled"]) {
    assert.ok(patch.includes(`ButtonVariant::Default.${state}(true, cx)`));
  }
  for (const state of ["hovered", "selected", "active"]) {
    assert.ok(patch.includes(`ButtonVariant::Ghost.${state}(false, cx)`));
  }
  assert.match(patch, /if self.selected \|\| self.disabled/);
  assert.match(patch, /selected_style.border_color = cx.theme\(\).transparent/);
  assert.doesNotMatch(patch, /normal_style.border_color = cx.theme\(\).transparent/);
  assert.doesNotMatch(patch.split("\n").filter(line => line.startsWith("+")).join("\n"), /rgb\(|hsla\(|\.h\(|\.rounded\(|\.on_click\(/);
});

test("menu accessibility exception is limited to AccessKit state projection", async () => {
  const policy = JSON.parse(await readFile(new URL("../source-preparation-policy.json", import.meta.url), "utf8"));
  const patchUrl = new URL(`../${policy.menuAccessibilityPatch.path}`, import.meta.url);
  const patch = await readFile(patchUrl, "utf8");
  assert.equal(await fileSha256(patchUrl), policy.menuAccessibilityPatch.sha256);
  assert.deepEqual(patch.split("diff --git ").slice(1).map(diff => diff.split("\n")[0]), [
    "a/crates/ui/src/menu/menu_item.rs b/crates/ui/src/menu/menu_item.rs",
    "a/crates/ui/src/menu/popup_menu.rs b/crates/ui/src/menu/popup_menu.rs",
  ]);
  assert.match(patch, /node\.set_disabled\(\)/);
  assert.match(patch, /node\.set_toggled\(gpui::accesskit::Toggled::True\)/);
  assert.match(patch, /\.checked\(item\.is_checked\(\)\)/);
  assert.match(patch, /fn accessibility_exposes_disabled_and_checked_state/);
  assert.match(patch, /fn accessibility_omits_inapplicable_menu_item_state/);
  assert.doesNotMatch(patch.split("\n").filter(line => line.startsWith("+")).join("\n"), /rgb\(|hsla\(|\.bg\(|\.text_color\(|\.on_click\(/);
});

test("descender backport contains exactly the upstream Button and Tab corrections", async () => {
  // longbridge/gpui-kit#2921, commit 20f8a4502b001fca85a9d7e6718b37cb78053238.
  // Compare all changed source lines, not merely the presence of an ellipsis:
  // font metrics, sizes and interaction handlers must remain untouched.
  const patchUrl = new URL("../patches/gpui-component-exact-zed.patch", import.meta.url);
  const patch = await readFile(patchUrl, "utf8");
  const sourceDiffs = patch.split("diff --git ").filter((diff) => /^a\/crates\/ui\/src\/(button\/button|tab\/tab)\.rs /.test(diff));
  assert.equal(sourceDiffs.length, 2);
  const changes = sourceDiffs.map((diff) => diff.split("\n")
    .filter((line) => /^[+-]/.test(line) && !/^(---|\+\+\+)/.test(line))
    .map((line) => line[0] + line.slice(1).trim()));
  assert.deepEqual(changes, [
    ["-.overflow_hidden()", "-.truncate()", "+.text_ellipsis()"],
    [
      "-(Some(label), Some(_)) => this.child(div().truncate().child(label)),",
      "+(Some(label), Some(_)) => this.child(div()",
      "+.min_w_0()", "+.whitespace_nowrap()", "+.text_ellipsis()", "+.child(label)),",
    ],
  ]);
  const sourcePolicy = JSON.parse(await readFile(new URL("../source-preparation-policy.json", import.meta.url), "utf8"));
  assert.equal(await fileSha256(patchUrl), sourcePolicy.patch.sha256);
});
test("submenu correction only forwards disabled state and includes both-state regression coverage", async () => {
  const patch = await readFile(new URL("../patches/gpui-component-exact-zed.patch", import.meta.url), "utf8");
  const menuDiff = patch.split("diff --git ").find((diff) => diff.startsWith("a/crates/ui/src/menu/popup_menu.rs "));
  assert.ok(menuDiff);
  const hunks = menuDiff.split(/^@@.*@@.*$/m).slice(1);
  assert.equal(hunks.length, 4);
  const productionChanges = hunks[0].split("\n")
    .filter((line) => /^[+-]/.test(line))
    .map((line) => line[0] + line.slice(1).trim());
  assert.deepEqual(productionChanges, [
    "+let submenu_disabled = submenu.disabled;",
    "-})", "+});",
    "+if let Some(PopupMenuItem::Submenu { disabled, .. }) =",
    "+self.menu_items.last_mut()", "+{",
    "+*disabled = submenu_disabled;", "+}",
  ]);
  assert.match(hunks[1], /disabled: false/);
  assert.match(hunks[2], /selected && !disabled/);
  assert.match(hunks[3], /fn owned_submenus_preserve_disabled_state/);
  assert.match(hunks[3], /\[true, false\]/);
  assert.match(hunks[3], /assert_eq!\(item.is_clickable\(\), !expected\)/);
  assert.match(hunks[3], /assert_eq!\(menu.active_submenu\(\).is_some\(\), !expected\)/);
});

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
    { path: "../gpui-migration/src/pdf_worker.rs", sha256: "a".repeat(64) },
    { path: "../gpui-migration/Cargo.toml", sha256: "b".repeat(64) },
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
