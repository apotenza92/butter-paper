import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";
import test from "node:test";

test("active workspace has zoom and page modes without CAD activation", async () => {
  const source = await readFile(new URL("../src/document_workspace.rs", import.meta.url), "utf8");
  assert.match(source, /ViewerToolbarStrip::new_with_zoom\(/);
  assert.doesNotMatch(source, /ViewerToolbarStrip::new_with_cad_view\(|CadViewControl::|handle_cad_view_control_event/);
});

test("both workspace tab paths use stock Outline without visual overrides", async () => {
  const source = await readFile(new URL("../src/document_workspace.rs", import.meta.url), "utf8");
  const bars = [...source.matchAll(/TabBar::new\("document-workspace-session-tabs-component"\)([\s\S]*?)\.children\(/g)];
  assert.equal(bars.length, 2, "cover loaded and empty/loading workspace paths");
  for (const [, style] of bars) {
    assert.match(style, /\.outline\(\)/);
    assert.match(style, /\.menu\(false\)/, "keep the measured external overflow control");
    assert.match(style, /\.max_width\(px\(190\.\)\)/, "retain document label truncation");
    assert.doesNotMatch(style, /\.(pill|segmented|bg|p_0)\(/);
  }
});

test("close controls overlay naturally sized labels without a reserved suffix", async () => {
  const source = await readFile(new URL("../src/document_workspace.rs", import.meta.url), "utf8");
  assert.equal([...source.matchAll(/\.child\(\s*session_tab_close_lane\(\)/g)].length, 2);
  assert.equal([...source.matchAll(/\.child\(session_tab_overlay_label\(/g)].length, 2);
  const label = source.slice(source.indexOf("fn session_tab_overlay_label"), source.indexOf("impl Render for DocumentWorkspace"));
  assert.match(label, /\.opacity\(0\.\)\.child\(label\.clone\(\)\)/);
  assert.match(label, /\.when\(revealed, \|this\| this\.pr_6\(\)\)/);
  assert.doesNotMatch(label, /\.group_hover\(/);
});

test("close affordances reveal on tab hover and keyboard focus without removing layout", async () => {
  const source = await readFile(new URL("../src/document_workspace.rs", import.meta.url), "utf8");
  const reveals = [...source.matchAll(/\.opacity\(if reveal_close \{ 1\. \} else \{ 0\. \}\)\s*\.focus\(\|style\| style\.opacity\(1\.\)\)/g)];
  assert.equal(reveals.length, 2);
});

test("close fills blend with their tab until the close target itself is hovered", async () => {
  const source = await readFile(new URL("../src/document_workspace.rs", import.meta.url), "utf8");
  const closes = [...source.matchAll(/Button::new\(close_id\)([\s\S]*?)\.small\(\)/g)];
  assert.equal(closes.length, 2);
  for (const [, style] of closes) {
    assert.match(style, /\.color\(cx.theme\(\).transparent\).hover\(cx.theme\(\).background\)/);
    assert.match(style, /\.bg\(cx.theme\(\).transparent\)/);
    assert.match(style, /\.border_color\(cx.theme\(\).transparent\)/);
  }
});
