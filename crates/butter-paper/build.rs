fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // GPUI initialization and Butter Paper's startup recovery/storage graph
    // exceed the Windows PE default main-thread stack before the first window
    // is created. Keep this on the GUI executable only: the worker has a much
    // smaller entry path, and the other platforms retain their native defaults.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg-bin=butter-paper=/STACK:16777216");
        embed_application_icon();
    }
}

/// GPUI loads icon resource 1 for its windows and the taskbar; without it,
/// Windows shows a generic application icon. The worker has no windows.
fn embed_application_icon() {
    let manifest_dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let icon = manifest_dir.join("../../assets/app/icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let rc = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("icon.rc");
    let icon = icon.to_string_lossy().replace('\\', "\\\\");
    std::fs::write(&rc, format!("1 ICON \"{icon}\"\n")).unwrap();
    embed_resource::compile_for(&rc, ["butter-paper"], embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
