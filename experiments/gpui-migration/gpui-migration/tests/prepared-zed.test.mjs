import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { join } from "node:path";
import test from "node:test";
import {
  validateCargoMetadata,
  validateZedPatch,
  validateZedLockTransition,
  loadPolicy,
} from "../scripts/source-preparation.mjs";
const hash = (value) => createHash("sha256").update(value).digest("hex");
const policy = await loadPolicy();
const patch = await readFile(
  new URL(`../${policy.zedPrepared.patch.path}`, import.meta.url),
);
const preparationScript = await readFile(
  new URL("../scripts/prepare-zed.mjs", import.meta.url),
  "utf8",
);
test("prepared Zed patch is exact, checksum-bound, and includes the reviewed test-window contract", () => {
  validateZedPatch(patch, policy);
  assert.throws(
    () =>
      validateZedPatch(Buffer.concat([patch, Buffer.from("drift")]), policy),
    /checksum/,
  );
  const injected = Buffer.concat([
    patch,
    Buffer.from("\ndiff --git a/other.rs b/other.rs\n"),
  ]);
  const changed = structuredClone(policy);
  changed.zedPrepared.patch.sha256 = hash(injected);
  assert.throws(() => validateZedPatch(injected, changed), /scope/);
  assert.ok(
    policy.zedPrepared.changedFiles.includes(
      "crates/gpui/src/platform/test/window.rs",
    ),
  );
  const source = patch.toString("utf8");
  assert.equal(
    source.match(/Err\(raw_window_handle::HandleError::NotSupported\)/g)
      ?.length,
    2,
  );
  assert.ok(
    !/^\+\s*unimplemented!\("Test Windows are not backed by a real platform window"\)/m.test(
      source,
    ),
  );
});
test("prepared Zed digest binds every reviewed worktree change through the index", () => {
  assert.ok(preparationScript.includes("git(['add', 'Cargo.toml'], temporary)"));
  assert.ok(preparationScript.includes("git(['diff', '--name-only'], root)"));
});
test("prepared renderer patch carries transformed text and image sprites across every backend", () => {
  const source = patch.toString("utf8");
  for (const api of [
    "paint_glyph_transformed",
    "paint_emoji_transformed",
    "paint_image_transformed",
    "pub fn paint_transformed",
  ])
    assert.ok(source.includes(api), `missing transformed renderer API ${api}`);
  for (const backend of [
    "crates/gpui_apple/src/shaders.metal",
    "crates/gpui_wgpu/src/shaders.wgsl",
    "crates/gpui_wgpu/src/shaders_webgl.wgsl",
    "crates/gpui_windows/src/shaders.hlsl",
  ])
    assert.ok(
      policy.zedPrepared.changedFiles.includes(backend),
      `missing backend patch scope ${backend}`,
    );
  assert.ok(source.includes("transformation: TransformationMatrix"));
  assert.ok(source.includes("to_device_position_transformed"));
  assert.ok(source.includes("distance_from_clip_rect_transformed"));
  assert.ok(source.includes("TransformationMatrix::unit()"));
  assert.ok(
    source.includes("transformed_image_retains_crop_tile_and_records_matrix"),
  );
});
test("prepared Linux backend rejects a missing Wayland seat and falls back to X11", () => {
  const source = patch.toString("utf8");
  for (const path of [
    "crates/gpui_linux/src/linux.rs",
    "crates/gpui_linux/src/linux/wayland/client.rs",
  ])
    assert.ok(
      policy.zedPrepared.changedFiles.includes(path),
      `missing Linux patch scope ${path}`,
    );
  assert.ok(
    source.includes(
      "Wayland compositor did not advertise the required wl_seat global",
    ),
  );
  assert.ok(source.includes("let seat = require_wl_seat(seat)?;"));
  assert.ok(
    source.includes(
      "Wayland initialisation failed ({error:#}); falling back to X11",
    ),
  );
  assert.ok(
    source.includes("requires_a_wayland_seat_before_client_initialisation"),
  );
  assert.ok(
    source.includes("rejects_unsupported_wayland_seats_without_panicking"),
  );
  assert.ok(!source.includes("+        let seat = seat.unwrap();"));
  assert.ok(!source.includes('+            "wl_seat below required version:'));
});
test("all prepared Zed identities share the reviewed workspace without Git duplicates", () => {
  const root = "/owned/prepared-zed";
  const metadata = {
    packages: Object.entries(policy.zedPrepared.packages).map(
      ([name, path]) => ({
        id: name,
        name,
        source: null,
        license: "Apache-2.0",
        manifest_path: join(root, path, "Cargo.toml"),
      }),
    ),
    resolve: { nodes: [] },
  };
  validateCargoMetadata(metadata, policy, root);
  const mixed = structuredClone(metadata);
  mixed.packages.find((pkg) => pkg.name === "collections").source =
    `git+${policy.zed.url}?rev=${policy.zed.revision}#${policy.zed.revision}`;
  assert.throws(
    () => validateCargoMetadata(mixed, policy, root),
    /identity drifted/,
  );
  const escaped = structuredClone(metadata);
  escaped.packages[0].manifest_path = "/other/Cargo.toml";
  assert.throws(
    () => validateCargoMetadata(escaped, policy, root),
    /identity drifted/,
  );
  const missing = structuredClone(metadata);
  missing.packages.pop();
  assert.throws(
    () => validateCargoMetadata(missing, policy, root),
    /identity drifted/,
  );
  const feature = structuredClone(metadata);
  feature.resolve.nodes.push({ id: "gpui", features: ["runtime_shaders"] });
  assert.throws(
    () => validateCargoMetadata(feature, policy, root),
    /forbidden feature/,
  );
});
test("lock transition permits only reviewed source identity substitutions", () => {
  const source = `source = "git+${policy.zed.url}?rev=${policy.zed.revision}#${policy.zed.revision}"\n`;
  const before = Object.keys(policy.zedPrepared.packages)
    .map(
      (name) =>
        `[[package]]\nname = "${name}"\nversion = "1.0.0"\n${source}dependencies = ["existing"]\n`,
    )
    .join("\n");
  const after = before.split(source).join("");
  validateZedLockTransition(before, after, policy);
  assert.throws(
    () =>
      validateZedLockTransition(
        before,
        after.replace("1.0.0", "2.0.0"),
        policy,
      ),
    /versions/,
  );
  assert.throws(
    () =>
      validateZedLockTransition(
        before,
        after.replace("existing", "new"),
        policy,
      ),
    /dependency edges/,
  );
});
