fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // GPUI initialization and Butter Paper's startup recovery/storage graph
    // exceed the Windows PE default main-thread stack before the first window
    // is created. Keep this on the GUI executable only: the worker has a much
    // smaller entry path, and the other platforms retain their native defaults.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg-bin=gpui-migration=/STACK:16777216");
    }
}
