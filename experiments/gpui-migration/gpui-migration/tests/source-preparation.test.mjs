import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  deterministicTreeDigest,
  fileSha256,
  validateAnnotationFontReceipts,
  validateCargoMetadata,
  validatePreparedManifest,
  validateSharedSourceReceipts,
  verifyAnnotationFontInputs,
} from "../scripts/source-preparation.mjs";

const revision = "8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc";

test("Unicode PDF fallback fonts stay checksum-pinned and their OFL notices ship in macOS bundles", async () => {
  const fontRoot = new URL("../.prepared/gpui-component-c27f5d5c/crates/story-web/fonts/", import.meta.url);
  assert.equal(
    await fileSha256(new URL("NotoSansSC-Regular.source.ttf", fontRoot)),
    "8e51e3cf738e98c31557d9cf1453ba7d1aee545632d12dd66149dbd8c73931c6",
  );
  assert.equal(
    await fileSha256(new URL("NotoEmoji-Regular.source.ttf", fontRoot)),
    "3c4aea565060fa91575a851e2718a5b14b9fe8856ead696b374c5a7e672179cb",
  );
  assert.equal(
    await fileSha256(new URL("OFL.txt", fontRoot)),
    "3907ac2a3f017acb830950733bf3bb7ecd5c66060b65f9f6d66255267cfad95d",
  );
  const buildScript = await readFile(new URL("../scripts/build-macos-app.sh", import.meta.url), "utf8");
  assert.match(buildScript, /Resources\/Licenses\/allura-font\.txt/);
  assert.match(buildScript, /Resources\/Licenses\/noto-fonts\.txt/);
});

test("native annotation fonts stay byte-identical to the locked Electron font packages", async () => {
  const policy = JSON.parse(await readFile(new URL("../source-preparation-policy.json", import.meta.url), "utf8"));
  const receipts = await verifyAnnotationFontInputs(policy);
  assert.equal(receipts.length, 12);
  const buildScript = await readFile(new URL("../scripts/build-macos-app.sh", import.meta.url), "utf8");
  for (const notice of ["arimo-font.txt", "roboto-mono-font.txt", "tinos-font.txt", "expo-google-fonts.txt"]) {
    assert.ok(buildScript.includes(`Resources/Licenses/${notice}`));
  }
});

test("macOS camera helper inherits the bundle minimum system version", async () => {
  const buildScript = await readFile(new URL("../scripts/build-macos-app.sh", import.meta.url), "utf8");
  assert.match(buildScript, /Print :LSMinimumSystemVersion/);
  assert.match(buildScript, /-target "\$\{native_arch\}-apple-macos\$\{minimum_macos_version\}"/);
  assert.match(buildScript, /arm64\|x86_64/);
});

test("macOS native build binds Cargo C compilation to the selected Xcode SDK", async () => {
  const buildScript = await readFile(new URL("../scripts/build-macos-app.sh", import.meta.url), "utf8");
  assert.match(buildScript, /export SDKROOT="\$\{SDKROOT:-\$\(xcrun --sdk macosx --show-sdk-path\)\}"/);
  assert.match(buildScript, /export CC="\$\{CC:-\$\(xcrun --sdk macosx --find clang\)\}"/);
  assert.match(buildScript, /export CXX="\$\{CXX:-\$\(xcrun --sdk macosx --find clang\+\+\)\}"/);
});

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

test("textarea padding exception is an opt-in layout override", async () => {
  const policy = JSON.parse(await readFile(new URL("../source-preparation-policy.json", import.meta.url), "utf8"));
  const patchUrl = new URL(`../${policy.textareaPaddingPatch.path}`, import.meta.url);
  const patch = await readFile(patchUrl, "utf8");
  assert.equal(await fileSha256(patchUrl), policy.textareaPaddingPatch.sha256);
  assert.deepEqual(patch.split("diff --git ").slice(1).map(diff => diff.split("\n")[0]), [
    "a/crates/ui/src/input/input.rs b/crates/ui/src/input/input.rs",
    "a/crates/ui/src/input/textarea.rs b/crates/ui/src/input/textarea.rs",
  ]);
  assert.match(patch, /pub fn editor_paddings\(mut self, paddings: Edges<Pixels>\)/);
  assert.match(patch, /self\.editor_paddings\.unwrap_or_else\(\|\|/);
  assert.match(patch, /if state\.presentation\(cx\)\.is_multi_line\(\)/);
  assert.doesNotMatch(
    patch.split("\n").filter(line => line.startsWith("+")).join("\n"),
    /rgb\(|hsla\(|\.bg\(|\.text_color\(|\.on_click\(/,
  );
});

test("textarea rotation exception is rotation-only and carries reversible geometry through Textarea", async () => {
  const policy = JSON.parse(await readFile(new URL("../source-preparation-policy.json", import.meta.url), "utf8"));
  const patchUrl = new URL(`../${policy.textareaRotationPatch.path}`, import.meta.url);
  const patch = await readFile(patchUrl, "utf8");
  assert.equal(await fileSha256(patchUrl), policy.textareaRotationPatch.sha256);
  assert.deepEqual(patch.split("diff --git ").slice(1).map(diff => diff.split("\n")[0]), [
    "a/crates/base/src/input/base/element.rs b/crates/base/src/input/base/element.rs",
    "a/crates/base/src/input/base/layout.rs b/crates/base/src/input/base/layout.rs",
    "a/crates/base/src/input/base/rotation.rs b/crates/base/src/input/base/rotation.rs",
    "a/crates/base/src/input/base/state.rs b/crates/base/src/input/base/state.rs",
    "a/crates/base/src/input/editor/display_map/text_wrapper.rs b/crates/base/src/input/editor/display_map/text_wrapper.rs",
    "a/crates/base/src/input/mod.rs b/crates/base/src/input/mod.rs",
    "a/crates/ui/src/input/input.rs b/crates/ui/src/input/input.rs",
    "a/crates/ui/src/input/mod.rs b/crates/ui/src/input/mod.rs",
    "a/crates/ui/src/input/state.rs b/crates/ui/src/input/state.rs",
    "a/crates/ui/src/input/textarea.rs b/crates/ui/src/input/textarea.rs",
  ]);
  assert.match(patch, /pub struct TextareaRotation/);
  assert.match(patch, /pub fn frame_size\(&self\) -> Size<Pixels>/);
  assert.match(patch, /pub\(crate\) fn to_world/);
  assert.match(patch, /pub\(crate\) fn to_local/);
  assert.match(patch, /pub\(crate\) fn bounds_to_world_aabb/);
  assert.match(patch, /state\.set_textarea_rotation\(self\.textarea_rotation, cx\)/);
  assert.match(patch, /line\.paint_transformed\(/);
  assert.match(patch, /rotation\.device_transformation\(window\.scale_factor\(\)\)/);
  assert.match(patch, /rotation\.to_local\(position\)/);
  assert.match(patch, /rotation\.to_world\(corners\[0\]\)/);
  assert.match(patch, /rotated_background_paths/);
  assert.match(patch, /rotated_underline_paths/);
  assert.match(patch, /rotated_strikethrough_paths/);
  assert.match(patch, /run\.strikethrough\.is_some\(\)/);
  assert.match(patch, /fn layout_strikethrough_range/);
  assert.match(patch, /strikethrough_band_top\(/);
  assert.match(patch, /fn layout_underline_range/);
  assert.match(patch, /underline_origin_top\(/);
  assert.match(patch, /wavy_underline_metrics\(/);
  assert.match(patch, /coalesce_underline_spans\(/);
  assert.match(patch, /snap_to_device_pixel\(/);
  assert.match(patch, /PathBuilder::stroke/);
  assert.match(patch, /builder\.cubic_bezier_to\(/);
  assert.match(patch, /builder\.close\(\)/);
  assert.match(patch, /run\.background_color = None/);
  assert.match(patch, /rotation\.bounds_to_world_aabb\(bounds\)/);
  assert.match(patch, /debug_assert!\(rotation\.is_none\(\), "only Textarea supports editor rotation"\)/);
  assert.match(patch, /fn identity_rotation_preserves_size_and_points/);
  assert.match(patch, /fn non_square_rotation_resolves_aabb_and_inverse_geometry/);
  assert.match(patch, /fn thirty_degree_click_maps_back_to_local_text_position/);
  assert.match(patch, /fn thirty_degree_selection_and_caret_aabbs_contain_every_rotated_corner/);
  assert.match(patch, /fn empty_bounds_aabb_is_the_single_rotated_corner/);
  assert.match(patch, /fn empty_frame_aabb_corners_map_outside_rotated_content/);
  assert.match(patch, /fn strikethrough_band_matches_gpui_shaped_line_placement/);
  assert.match(patch, /fn underline_origin_matches_gpui_shaped_line_placement/);
  assert.match(patch, /fn wavy_underline_metrics_match_gpui_device_snapping_at_one_and_two_x/);
  assert.match(patch, /fn adjacent_equal_effective_underlines_form_one_continuous_span/);
  assert.match(patch, /pub\(crate\) struct AccessibilityTextLayout/);
  assert.match(patch, /fn accessibility_positions_preserve_chunk_and_soft_wrap_boundaries/);
  assert.match(patch, /fn accessibility_word_starts_are_transitions_not_every_word_character/);
  assert.match(patch, /fn accessibility_rows_share_the_scrolled_paint_origin/);
  assert.match(patch, /let previous = if run_index > 0/);
  assert.doesNotMatch(patch, /then_some\(run_index - 1\)/);
  assert.doesNotMatch(patch, /pub fn .*TransformationMatrix|matrix:|\.scale\(|skew\(/);
});

test("prepared Textarea rotation cases execute through the app-owned Cargo graph", async () => {
  const harness = await readFile(new URL("prepared_textarea_rotation.rs", import.meta.url), "utf8");
  assert.equal(
    harness.match(/^#\[path = "([^\"]+)"\]$/m)?.[1],
    "../.prepared/gpui-component-c27f5d5c/crates/base/src/input/base/rotation.rs",
  );
  assert.match(harness, /^mod prepared_textarea_rotation;$/m);
  assert.doesNotMatch(harness, /^\s*(?:#\[test\]|fn\s+\w+\s*\()/m);
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

test("annotation font receipts reject missing, reordered, substituted and checksum-drifted faces", () => {
  const expected = [
    { asset: "assets/fonts/Arimo-Regular.ttf", source: "node_modules/arimo.ttf", bytes: 10, sha256: "a".repeat(64) },
    { asset: "assets/fonts/Tinos-Regular.ttf", source: "node_modules/tinos.ttf", bytes: 20, sha256: "b".repeat(64) },
  ];
  const actual = expected.map(face => ({
    ...face,
    sourceBytes: face.bytes,
    sourceSha256: face.sha256,
  }));
  assert.doesNotThrow(() => validateAnnotationFontReceipts(expected, actual));
  assert.throws(
    () => validateAnnotationFontReceipts(expected, [actual[0]]),
    /annotation font receipt coverage drifted/,
  );
  assert.throws(
    () => validateAnnotationFontReceipts(expected, [actual[1], actual[0]]),
    /annotation font path drifted/,
  );
  assert.throws(
    () => validateAnnotationFontReceipts(expected, [actual[0], { ...actual[1], source: "node_modules/substitute.ttf" }]),
    /annotation font path drifted/,
  );
  assert.throws(
    () => validateAnnotationFontReceipts(expected, [actual[0], { ...actual[1], sourceSha256: "c".repeat(64) }]),
    /annotation font checksum drifted/,
  );
});
