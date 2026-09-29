import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const applicationEntry = readFileSync(
  new URL(
    "../experiments/gpui-migration/gpui-migration/src/bin/gpui-migration.rs",
    import.meta.url,
  ),
  "utf8",
);
const cargoManifest = readFileSync(
  new URL(
    "../experiments/gpui-migration/gpui-migration/Cargo.toml",
    import.meta.url,
  ),
  "utf8",
);

describe("GPUI Windows fatal launch dialog contract", () => {
  it("shows startup errors with a modal native Windows error dialog", () => {
    expect(applicationEntry).toContain(
      '#[cfg(target_os = "windows")]\n    display_windows_fatal_launch_error(&message);',
    );
    expect(applicationEntry).toContain("fn display_windows_fatal_launch_error(detail: &str)");
    expect(applicationEntry).toContain("MessageBoxW(");
    expect(applicationEntry).toContain('wide_null_terminated("Butter Paper could not start")');
    expect(applicationEntry).toContain("MB_OK | MB_ICONERROR | MB_TASKMODAL");
    expect(applicationEntry).toContain("fn wide_null_terminated(value: &str) -> Vec<u16>");
    expect(applicationEntry).toContain(".replace('\\0', \"�\")");
  });

  it("enables only the Windows messaging API feature needed by the dialog", () => {
    expect(cargoManifest).toContain('"Win32_UI_WindowsAndMessaging"');
  });
});
