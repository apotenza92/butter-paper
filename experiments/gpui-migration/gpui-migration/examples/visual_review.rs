//! Development tool: renders the document workspace off-screen with real
//! Metal, replays a small input script and saves screenshots for visual
//! review. macOS only; uses placeholder white pages rather than a real PDF.
//!
//! cargo run --example visual_review -- <script> <output-dir>
//!
//! Script lines (window pixels):
//!   size W H          window size (first line only)
//!   open              open a three-page placeholder document
//!   click X Y         left click
//!   drag X1 Y1 X2 Y2  left drag
//!   move X Y          pointer move
//!   wheel X Y DY      scroll wheel (add `ctrl` to zoom)
//!   keys KEYSTROKES   e.g. `cmd-o`
//!   wait MS           advance the clock
//!   shot NAME         save NAME.png

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use butter_paper_gpui_migration::application_assets::ApplicationAssets;
use butter_paper_gpui_migration::document_workspace::{
    DocumentWorkspace, DocumentWorkspaceTemplateCommand, NativeDocumentResource, OpenedNativeDocument, RasterSurface,
    ThumbnailSurface, init_document_workspace_actions,
};
use butter_paper_gpui_migration::page_view_control::PageViewMode;
use butter_paper_gpui_migration::template_manager::{TemplateManagerView, route_workspace_template_command};
use butter_paper_gpui_migration::viewer::TileRequest;
use gpui::{
    AnyWindowHandle, AppContext as _, Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent,
    VisualTestAppContext, point, px, size,
};
use gpui_component::Root;

struct WhitePages(AtomicBool);

fn white(width: u32, height: u32) -> RasterSurface {
    RasterSurface::new(width, height, vec![0xff; width as usize * height as usize * 4]).unwrap()
}

impl NativeDocumentResource for WhitePages {
    fn worker_pid(&self) -> Option<u32> {
        None
    }
    fn render_page(&self, _: u32, width: u32) -> Result<RasterSurface, String> {
        Ok(white(width.max(1), (width as f32 * 792. / 612.) as u32 + 1))
    }
    fn render_tile(&self, request: TileRequest) -> Result<RasterSurface, String> {
        Ok(white(request.crop.width.max(1) as u32, request.crop.height.max(1) as u32))
    }
    fn close(&self) -> Result<(), String> {
        self.0.store(true, Ordering::Release);
        Ok(())
    }
    fn is_released(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Hosts the workspace with the dialog layer, as the application shell does.
struct Shell(gpui::Entity<DocumentWorkspace>);

impl gpui::Render for Shell {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        use gpui::ParentElement as _;
        use gpui::Styled as _;
        gpui::div()
            .size_full()
            .child(self.0.clone())
            .children(Root::render_dialog_layer(window, cx))
    }
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let script = std::fs::read_to_string(arguments.next().expect("script path")).unwrap();
    let output = PathBuf::from(arguments.next().expect("output directory"));
    std::fs::create_dir_all(&output).unwrap();

    let mut lines = script.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')).peekable();
    let (width, height) = match lines.peek().map(|line| line.split_whitespace().collect::<Vec<_>>()) {
        Some(words) if words.first() == Some(&"size") => {
            lines.next();
            (words[1].parse::<f32>().unwrap(), words[2].parse::<f32>().unwrap())
        }
        _ => (1320., 860.),
    };

    let platform = gpui_platform::current_platform(false);
    let mut cx = VisualTestAppContext::with_asset_source(platform, Arc::new(ApplicationAssets));
    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let slot = Rc::new(RefCell::new(None));
    let window = cx
        .open_offscreen_window(size(px(width), px(height)), {
            let slot = slot.clone();
            move |window, cx| {
                let workspace = cx.new(DocumentWorkspace::new);
                slot.replace(Some(workspace.clone()));
                let root = std::env::temp_dir().join(format!("bp-visual-review-{}", std::process::id()));
                let manager = cx.new(|cx| TemplateManagerView::open_persistent(root, window, cx).unwrap());
                window
                    .subscribe(&workspace, cx, move |_, event: &DocumentWorkspaceTemplateCommand, window, cx| {
                        route_workspace_template_command(&manager, event, window, cx);
                    })
                    .detach();
                let shell = cx.new(|_| Shell(workspace));
                cx.new(|cx| Root::new(shell, window, cx))
            }
        })
        .unwrap();
    let handle: AnyWindowHandle = window.into();
    let workspace = slot.borrow_mut().take().unwrap();
    let settle = |cx: &mut VisualTestAppContext| {
        for _ in 0..3 {
            cx.advance_clock(Duration::from_millis(50));
            cx.run_until_parked();
            let _ = cx.update_window(handle, |_, window, _| window.refresh());
        }
    };
    settle(&mut cx);

    for line in lines {
        let words = line.split_whitespace().collect::<Vec<_>>();
        let number = |index: usize| words[index].parse::<f32>().unwrap();
        match words[0] {
            "open" => {
                cx.update(|cx| {
                    workspace.update(cx, |workspace, cx| {
                        let request = workspace.begin_open(PathBuf::from("Sample.pdf"), cx);
                        let document = OpenedNativeDocument::new(
                            "Sample.pdf",
                            vec![(612., 792.); 3],
                            white(612, 792),
                            (0..3).map(|page| ThumbnailSurface::new(page, white(61, 79))).collect(),
                            Arc::new(WhitePages(AtomicBool::new(false))),
                        )
                        .unwrap();
                        workspace.apply_open_result(&request, Ok(document), cx);
                        workspace.set_view_configuration(
                            request.document_id,
                            PageViewMode::Continuous,
                            100.,
                            cx,
                        );
                    });
                });
            }
            "click" => cx.simulate_click(handle, point(px(number(1)), px(number(2))), Modifiers::default()),
            "move" => cx.simulate_mouse_move(handle, point(px(number(1)), px(number(2))), None, Modifiers::default()),
            "drag" => {
                let start = point(px(number(1)), px(number(2)));
                let end = point(px(number(3)), px(number(4)));
                cx.simulate_mouse_down(handle, start, MouseButton::Left, Modifiers::default());
                for step in 1..=8 {
                    let t = step as f32 / 8.;
                    let at = point(start.x + (end.x - start.x) * t, start.y + (end.y - start.y) * t);
                    cx.simulate_mouse_move(handle, at, Some(MouseButton::Left), Modifiers::default());
                }
                cx.simulate_mouse_up(handle, end, MouseButton::Left, Modifiers::default());
            }
            "wheel" => cx.simulate_event(
                handle,
                ScrollWheelEvent {
                    position: point(px(number(1)), px(number(2))),
                    delta: ScrollDelta::Pixels(point(px(0.), px(number(3)))),
                    modifiers: Modifiers { control: words.get(4) == Some(&"ctrl"), ..Modifiers::default() },
                    ..Default::default()
                },
            ),
            "keys" => cx.simulate_keystrokes(handle, &words[1..].join(" ")),
            "wait" => cx.advance_clock(Duration::from_millis(number(1) as u64)),
            "shot" => {
                settle(&mut cx);
                let image = cx.capture_screenshot(handle).unwrap();
                let path = output.join(format!("{}.png", words[1]));
                image.save(&path).unwrap();
                println!("saved {}", path.display());
            }
            other => panic!("unknown script command: {other}"),
        }
        settle(&mut cx);
    }
    // The review is complete; skip GPUI's leaked-handle check for the
    // workspace and window this tool deliberately keeps alive.
    drop(workspace);
    std::process::exit(0);
}
