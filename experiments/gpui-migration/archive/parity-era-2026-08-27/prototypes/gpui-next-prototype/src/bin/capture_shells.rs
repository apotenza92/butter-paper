#[cfg(target_os = "macos")]
#[path = "../main.rs"]
mod prototype_app;

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("direct GPUI texture capture is currently supported on macOS only");
    std::process::exit(1);
}

#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{path::PathBuf, rc::Rc, sync::Arc};

    use gpui::{AnyWindowHandle, AppContext as _, Styled as _, VisualTestAppContext, px, size};
    use gpui_component::{ActiveTheme as _, Root};
    use gpui_component_assets::Assets;

    let output_dir = std::env::var_os("BP_CAPTURE_OUTPUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/captures"));
    std::fs::create_dir_all(&output_dir)?;

    let platform: Rc<dyn gpui::Platform> = gpui_platform::current_platform(false);
    let mut cx = VisualTestAppContext::with_asset_source(platform, Arc::new(Assets));
    cx.update(gpui_component::init);

    for (index, name) in ["workbench", "focus", "review"].into_iter().enumerate() {
        let window = cx.open_offscreen_window(size(px(1320.), px(860.)), |window, cx| {
            let view = cx.new(|_| prototype_app::Prototype::for_capture(index));
            cx.new(|cx| Root::new(view, window, cx).bg(cx.theme().background))
        })?;
        let window: AnyWindowHandle = window.into();

        cx.run_until_parked();
        cx.update_window(window, |_, window, _| window.refresh())?;
        cx.run_until_parked();

        let screenshot = cx.capture_screenshot(window)?;
        let output_path = output_dir.join(format!("gpui-next-{name}.png"));
        screenshot.save(&output_path)?;
        println!("{}", output_path.display());

        cx.update_window(window, |_, window, _| window.remove_window())?;
        cx.run_until_parked();
    }

    Ok(())
}
