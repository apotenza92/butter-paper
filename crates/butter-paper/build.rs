// Used only by Windows builds.
#[allow(dead_code)]
mod windows_resource {
    include!("build/windows_resource.rs");
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build/windows_resource.rs");

    // GPUI initialization and Butter Paper's startup recovery/storage graph
    // exceed the Windows PE default main-thread stack before the first window
    // is created. Keep this on the GUI executable only: the worker has a much
    // smaller entry path, and the other platforms retain their native defaults.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg-bin=butter-paper=/STACK:16777216");
        embed_application_resources();
    }
}

/// The app's icon and version information (see `windows_resource_script`).
/// The worker has no windows.
fn embed_application_resources() {
    let manifest_dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let icon = manifest_dir.join("../../assets/app/icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let rc = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("app.rc");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    std::fs::write(
        &rc,
        windows_resource::windows_resource_script(&icon.to_string_lossy(), &version),
    )
    .unwrap();
    embed_resource::compile_for(&rc, ["butter-paper"], embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
