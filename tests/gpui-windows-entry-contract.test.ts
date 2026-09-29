import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const buildScript = readFileSync(
  new URL(
    "../experiments/gpui-migration/gpui-migration/build.rs",
    import.meta.url,
  ),
  "utf8",
);
const applicationEntry = readFileSync(
  new URL(
    "../experiments/gpui-migration/gpui-migration/src/bin/gpui-migration.rs",
    import.meta.url,
  ),
  "utf8",
);

describe("GPUI Windows executable entry contract", () => {
  it("links the Windows GUI executable with the reviewed main-thread stack reserve", () => {
    expect(buildScript).toContain(
      'std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")',
    );
    expect(buildScript).toContain(
      "cargo:rustc-link-arg-bin=gpui-migration=/STACK:16777216",
    );
    expect(buildScript).not.toContain("rustc-link-arg-bin=butter-paper-pdf-worker");
  });

  it("uses the Windows GUI subsystem instead of opening a console window", () => {
    expect(applicationEntry).toMatch(
      /^#!\[cfg_attr\(target_os = "windows", windows_subsystem = "windows"\)\]/,
    );
  });
});
