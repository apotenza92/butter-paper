import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("Quit transaction replaces the stock default footer rather than adding another OK action", async () => {
  const source = await readFile(new URL("../src/application_close_workspace.rs", import.meta.url), "utf8");
  const dialog = source.slice(source.indexOf("pub fn open_dialog("), source.indexOf("fn reconcile_dialog_window("));
  assert.match(dialog, /\.footer\(gpui::Empty\)/);
  assert.match(dialog, /for action in dialog\.actions/);
  assert.match(dialog, /\.child\(footer\)/);
  // Enter still follows the safe Save All transaction; hiding the default
  // button must not replace or remove the existing keyboard command.
  assert.match(dialog, /\.on_ok\([\s\S]*?ApplicationCloseAction::SaveAll/);
});
