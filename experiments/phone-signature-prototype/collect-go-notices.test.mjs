import assert from "node:assert/strict";
import test from "node:test";
import { renderGoNotices } from "./collect-go-notices.mjs";

test("renders a deterministic sorted Go dependency notice inventory", () => {
  const rendered = renderGoNotices({
    goVersion: "go1.24.1",
    goLicense: "Go licence text",
    modules: [
      { path: "example.com/z", version: "v1.0.0", licenses: [{ name: "LICENSE", text: "Z licence" }] },
      { path: "example.com/a", version: "", licenses: [{ name: "NOTICE.txt", text: "A notice" }, { name: "COPYING", text: "A licence" }] },
    ],
  });
  assert.ok(rendered.indexOf("example.com/a") < rendered.indexOf("example.com/z"));
  assert.ok(rendered.indexOf("COPYING") < rendered.indexOf("NOTICE.txt"));
  assert.match(rendered, /Go standard library \(go1\.24\.1\)/);
  assert.match(rendered, /main-source-pin/);
});

test("fails closed when a used module has no licence", () => {
  assert.throws(() => renderGoNotices({
    goVersion: "go1.24.1",
    goLicense: "Go licence text",
    modules: [{ path: "example.com/missing", version: "v1.0.0", licenses: [] }],
  }), /has no licence file/);
});
