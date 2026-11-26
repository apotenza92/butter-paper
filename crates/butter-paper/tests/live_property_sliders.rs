//! A property slider shows its value on the page while the thumb is dragged
//! and the number beside it follows. The value is committed once, when the
//! pointer is released: one undo step, one change of the unsaved state, and
//! no history, revision or recovery write before that.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use butter_paper::annotation_adapter::AnnotationTool;
use butter_paper::annotation_model::{
    Annotation, AnnotationScene, AnnotationSnapshot, ArcAnnotation, CalloutAnnotation,
    CalloutAppearance, CloudAnnotation, CloudPlusAnnotation, CloudPlusAppearance, DecodedRgbaAsset,
    DimensionAnnotation, DimensionAppearance, EllipseAnnotation, ImageAnnotation, LineKind,
    MarkupId, PdfPoint, PdfRect, PenAnnotation, PenAppearance, RectangleAnnotation,
    RectangleAppearance, SnapshotAnnotation, StraightLineAnnotation, StraightLineAppearance,
    StrokeStyle, TextBoxAnnotation, TextBoxStyle, VertexPathAnnotation, VertexPathKind,
};
use butter_paper::document_resource::{
    NativeDocumentResource, OpenedNativeDocument, RasterSurface, ThumbnailSurface,
};
use butter_paper::document_workspace::{
    DocumentId, DocumentWorkspace, init_document_workspace_actions,
};
use butter_paper::pdf_text_edit::PageTextLayer;
use butter_paper::pdf_worker::DocumentEdits;
use gpui::{AppContext as _, Bounds, Modifiers, MouseButton, Pixels, Point, TestAppContext, point};
use gpui_component::Root;

fn raster(width: u32, height: u32) -> RasterSurface {
    RasterSurface::new(
        width,
        height,
        vec![0xff; width as usize * height as usize * 4],
    )
    .unwrap()
}

struct BlankResource {
    released: AtomicBool,
}

impl NativeDocumentResource for BlankResource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4303)
    }

    fn render_page(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
        Ok(raster(width, page_index + 2))
    }

    fn render_tile(
        &self,
        request: butter_paper::viewer::TileRequest,
    ) -> Result<RasterSurface, String> {
        Ok(raster(
            request.crop.width as u32,
            request.crop.height as u32,
        ))
    }

    fn page_text(&self, page_index: u32) -> Result<PageTextLayer, String> {
        Ok(PageTextLayer {
            page_index,
            ..Default::default()
        })
    }

    fn set_document_edits(&self, _: &DocumentEdits) -> Result<(), String> {
        Ok(())
    }

    fn close(&self) -> Result<(), String> {
        self.released.store(true, Ordering::Release);
        Ok(())
    }

    fn is_released(&self) -> bool {
        self.released.load(Ordering::Acquire)
    }
}

type Workspace = gpui::Entity<DocumentWorkspace>;

/// A workspace that shows one page with `annotations`, with `selected`
/// selected and its properties open.
fn open_with_selection<'a>(
    cx: &'a mut TestAppContext,
    annotations: Vec<Annotation>,
    selected: &MarkupId,
) -> (Workspace, DocumentId, &'a mut gpui::VisualTestContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        // Behaviour tests check end states; motion has its own tests.
        cx.set_reduce_motion(true);
        init_document_workspace_actions(cx);
    });
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let slot = slot.clone();
        move |window, cx| {
            let workspace = cx.new(DocumentWorkspace::new);
            slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = slot.borrow_mut().take().unwrap();
    // Leaving a field sends a blur only to an active window.
    cx.update(|window, _| window.activate_window());
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("live-slider.pdf"), cx)
    });
    let document_id = request.document_id;
    workspace.update(cx, |workspace, cx| {
        let opened = OpenedNativeDocument::new(
            "live-slider.pdf",
            vec![(612., 792.)],
            raster(32, 40),
            vec![ThumbnailSurface::new(0, raster(8, 10))],
            Arc::new(BlankResource {
                released: AtomicBool::new(false),
            }),
        )
        .unwrap()
        .with_annotations(annotations);
        workspace.apply_open_result(&request, Ok(opened), cx);
    });
    assert!(workspace.update(cx, |workspace, cx| {
        workspace
            .set_annotation_tool(document_id, AnnotationTool::Select, cx)
            .unwrap();
        workspace.select_annotation(document_id, selected, cx)
    }));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let actions = cx
        .debug_bounds("document-workspace-rail-actions")
        .expect("the right-rail properties action must render");
    cx.simulate_click(actions.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (workspace, document_id, cx)
}

fn snapshot(
    workspace: &Workspace,
    document_id: DocumentId,
    cx: &gpui::VisualTestContext,
) -> AnnotationSnapshot {
    workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap()
}

fn scene(
    workspace: &Workspace,
    document_id: DocumentId,
    cx: &mut gpui::VisualTestContext,
) -> AnnotationScene {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(document_id, 0, cx)
    })
}

/// What a test changes about the history of the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct History {
    revision: u64,
    undo_depth: usize,
    dirty: bool,
}

fn history(
    workspace: &Workspace,
    document_id: DocumentId,
    cx: &gpui::VisualTestContext,
) -> History {
    let snapshot = snapshot(workspace, document_id, cx);
    History {
        revision: snapshot.revision,
        undo_depth: snapshot.undo_depth,
        dirty: snapshot.dirty,
    }
}

/// One slider of a property panel, and where its value is drawn, stored and
/// shown as a number.
struct SliderCase {
    /// The slider's debug selector, or the selector of its row.
    track: &'static str,
    /// The slider's selector is the row's: the slider leaves 96 px (the number
    /// and its gap) of its right end.
    row: bool,
    /// The value, as a fraction of the slider, that the markup has.
    original: f64,
    /// The value as drawn on the page.
    drawn: Box<dyn Fn(&AnnotationScene) -> f64>,
    /// The value as stored in the document.
    stored: Box<dyn Fn(&AnnotationSnapshot) -> f64>,
    /// The text of the number beside the slider.
    number: Box<dyn Fn(&Workspace, &gpui::VisualTestContext) -> String>,
    /// The value at the slider's right end (it reads 0 at the left).
    scale: f64,
    /// The number's text for a value of 1: 100 for a percentage.
    number_scale: f64,
    /// The slider shares its horizontal extent with `track` but sits level
    /// with this selector (its number's field).
    y_from: Option<&'static str>,
}

impl SliderCase {
    fn bounds(&self, cx: &mut gpui::VisualTestContext) -> Bounds<Pixels> {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = cx
            .debug_bounds(self.track)
            .unwrap_or_else(|| panic!("{} must render", self.track));
        let bounds = match self.y_from {
            Some(level) => {
                let level = cx
                    .debug_bounds(level)
                    .unwrap_or_else(|| panic!("{level} must render"));
                Bounds::new(
                    point(bounds.origin.x, level.origin.y),
                    gpui::size(bounds.size.width, level.size.height),
                )
            }
            None => bounds,
        };
        if self.row {
            Bounds::new(
                bounds.origin,
                gpui::size(bounds.size.width - gpui::px(96.), bounds.size.height),
            )
        } else {
            bounds
        }
    }

    fn at(&self, cx: &mut gpui::VisualTestContext, fraction: f64) -> Point<Pixels> {
        let track = self.bounds(cx);
        point(
            track.origin.x + track.size.width * fraction as f32,
            track.center().y,
        )
    }

    fn drawn_value(
        &self,
        workspace: &Workspace,
        document_id: DocumentId,
        cx: &mut gpui::VisualTestContext,
    ) -> f64 {
        (self.drawn)(&scene(workspace, document_id, cx))
    }

    fn stored_value(
        &self,
        workspace: &Workspace,
        document_id: DocumentId,
        cx: &gpui::VisualTestContext,
    ) -> f64 {
        (self.stored)(&snapshot(workspace, document_id, cx))
    }

    fn number_value(&self, workspace: &Workspace, cx: &gpui::VisualTestContext) -> f64 {
        let text = (self.number)(workspace, cx);
        text.trim_end_matches('%')
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("the number beside the slider must be numeric: {text:?}"))
    }
}

/// A drag starts on the first move after the press and follows from the
/// second, as the system's moves arrive.
fn start_drag(cx: &mut gpui::VisualTestContext, from: Point<Pixels>) {
    cx.simulate_mouse_move(
        point(from.x + gpui::px(4.), from.y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
}

fn near(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual} is not within {tolerance} of {expected}"
    );
}

/// Drags the slider of `case`: the page shows each value before release, the
/// number follows, nothing is committed until release, and the release
/// commits once. Undo restores the original value.
fn drag_slider(
    workspace: &Workspace,
    document_id: DocumentId,
    cx: &mut gpui::VisualTestContext,
    case: &SliderCase,
) {
    let name = case.track;
    let before = history(workspace, document_id, cx);
    assert!(!before.dirty, "{name}: the document starts unchanged");
    let original = case.original * case.scale;
    near(
        case.drawn_value(workspace, document_id, cx),
        original,
        0.0001,
        name,
    );

    // Press at 30%, then drag to 60%, then 80%: each frame shows its value.
    let start = case.at(cx, 0.3);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    near(
        case.drawn_value(workspace, document_id, cx),
        0.3 * case.scale,
        0.04 * case.scale,
        &format!("{name}: drawn after the press"),
    );
    start_drag(cx, start);
    for fraction in [0.6, 0.8] {
        let to = case.at(cx, fraction);
        cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
        near(
            case.drawn_value(workspace, document_id, cx),
            fraction * case.scale,
            0.04 * case.scale,
            &format!("{name}: drawn while dragging to {fraction}"),
        );
        near(
            case.number_value(workspace, cx),
            fraction * case.scale * case.number_scale,
            0.04 * case.scale * case.number_scale + 0.5,
            &format!("{name}: number while dragging to {fraction}"),
        );
        assert_eq!(
            history(workspace, document_id, cx),
            before,
            "{name}: nothing is committed before release"
        );
        near(
            case.stored_value(workspace, document_id, cx),
            original,
            0.0001,
            &format!("{name}: the stored value before release"),
        );
    }

    let end = case.at(cx, 0.8);
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    let drawn = case.drawn_value(workspace, document_id, cx);
    near(drawn, 0.8 * case.scale, 0.04 * case.scale, name);
    near(
        case.stored_value(workspace, document_id, cx),
        drawn,
        0.0001,
        &format!("{name}: release commits the value that was drawn"),
    );
    let after = history(workspace, document_id, cx);
    assert_eq!(
        (after.undo_depth, after.dirty),
        (before.undo_depth + 1, true),
        "{name}: release is one undo step and marks the document unsaved"
    );
    assert_ne!(
        after.revision, before.revision,
        "{name}: release changes the revision"
    );
    assert!(
        !workspace.read_with(cx, |workspace, cx| workspace
            .property_preview_active(document_id, cx)),
        "{name}: no preview remains after release"
    );

    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    assert_eq!(
        history(workspace, document_id, cx).undo_depth,
        before.undo_depth,
        "{name}: one undo reverts the whole drag"
    );
    near(
        case.drawn_value(workspace, document_id, cx),
        original,
        0.0001,
        &format!("{name}: undo restores the drawn value"),
    );
    near(
        case.stored_value(workspace, document_id, cx),
        original,
        0.0001,
        &format!("{name}: undo restores the stored value"),
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.redo_annotations(document_id, cx)
        })
        .unwrap();
    near(
        case.stored_value(workspace, document_id, cx),
        drawn,
        0.0001,
        &format!("{name}: redo commits the dragged value again"),
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
}

/// A drag that returns to the original value before release changes nothing:
/// the page shows the original value again and the history is untouched.
fn drag_and_return(
    workspace: &Workspace,
    document_id: DocumentId,
    cx: &mut gpui::VisualTestContext,
    case: &SliderCase,
) {
    let name = case.track;
    let before = history(workspace, document_id, cx);
    let original = case.original * case.scale;
    let start = case.at(cx, 0.4);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    start_drag(cx, start);
    assert!(
        (case.drawn_value(workspace, document_id, cx) - original).abs() > 0.01,
        "{name}: the drag shows a different value"
    );
    let home = case.at(cx, case.original);
    cx.simulate_mouse_move(home, Some(MouseButton::Left), Modifiers::default());
    near(
        case.drawn_value(workspace, document_id, cx),
        original,
        0.0001,
        &format!("{name}: back at the original value"),
    );
    cx.simulate_mouse_up(home, MouseButton::Left, Modifiers::default());
    assert_eq!(
        history(workspace, document_id, cx),
        before,
        "{name}: a drag that ends where it began changes nothing"
    );
}

/// Escape during a drag shows the committed markup again, returns the slider
/// and its number to the committed value, and the release that follows
/// changes nothing.
fn escape_cancels_the_drag(
    workspace: &Workspace,
    document_id: DocumentId,
    cx: &mut gpui::VisualTestContext,
    case: &SliderCase,
) {
    let name = case.track;
    let before = history(workspace, document_id, cx);
    let original = case.original * case.scale;
    let start = case.at(cx, 0.5);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    start_drag(cx, start);
    let moved = case.at(cx, 0.6);
    cx.simulate_mouse_move(moved, Some(MouseButton::Left), Modifiers::default());
    assert!(
        (case.drawn_value(workspace, document_id, cx) - original).abs() > 0.01,
        "{name}: the drag shows a different value"
    );
    let focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| focus.focus(window, cx));
    cx.simulate_keystrokes("escape");
    near(
        case.drawn_value(workspace, document_id, cx),
        original,
        0.0001,
        &format!("{name}: Escape shows the original value"),
    );
    near(
        case.number_value(workspace, cx),
        original * case.number_scale,
        0.5,
        &format!("{name}: Escape returns the number"),
    );
    let further = case.at(cx, 0.9);
    cx.simulate_mouse_move(further, Some(MouseButton::Left), Modifiers::default());
    near(
        case.drawn_value(workspace, document_id, cx),
        original,
        0.0001,
        &format!("{name}: the cancelled drag no longer shows values"),
    );
    cx.simulate_mouse_up(further, MouseButton::Left, Modifiers::default());
    assert_eq!(
        history(workspace, document_id, cx),
        before,
        "{name}: the release after Escape commits nothing"
    );
    near(
        case.stored_value(workspace, document_id, cx),
        original,
        0.0001,
        name,
    );
}

fn id(name: &str) -> MarkupId {
    MarkupId::new(name).unwrap()
}

fn rect_appearance() -> RectangleAppearance {
    RectangleAppearance::new("#2563eb", 1.5, Some("#dbeafe"), 1.)
        .unwrap()
        .with_fill_opacity(1.)
        .unwrap()
}

#[gpui::test]
fn the_ink_opacity_slider_updates_the_page_while_it_is_dragged(cx: &mut TestAppContext) {
    let pen = PenAnnotation::new_paths(
        id("pdf:live-pen"),
        0,
        vec![vec![
            PdfPoint::new(10., 10.).unwrap(),
            PdfPoint::new(60., 60.).unwrap(),
        ]],
        PenAppearance::new("#ff0000", 2., 1.).unwrap(),
        false,
    )
    .unwrap();
    let pen_id = pen.id.clone();
    let (workspace, document_id, cx) = open_with_selection(cx, vec![Annotation::Pen(pen)], &pen_id);
    let case = SliderCase {
        track: butter_paper::ink_property_inspector::INK_INSPECTOR_OPACITY_TRACK_ID,
        row: false,
        original: 1.,
        scale: 1.,
        number_scale: 100.,
        y_from: None,
        drawn: Box::new(|scene| scene.pens[0].appearance.opacity()),
        stored: Box::new(|snapshot| snapshot.pens[0].appearance.opacity()),
        number: Box::new(|workspace, cx| {
            workspace.read_with(cx, |workspace, cx| {
                workspace
                    .ink_property_inspector()
                    .unwrap()
                    .read(cx)
                    .opacity_input()
                    .read(cx)
                    .value()
                    .to_string()
            })
        }),
    };
    drag_slider(&workspace, document_id, cx, &case);
    drag_and_return(&workspace, document_id, cx, &case);
    escape_cancels_the_drag(&workspace, document_id, cx, &case);
}

#[gpui::test]
fn the_rectangle_opacity_and_fill_opacity_sliders_update_the_page_while_dragged(
    cx: &mut TestAppContext,
) {
    use butter_paper::rectangle_property_inspector::{
        RECTANGLE_INSPECTOR_FILL_OPACITY_ID, RECTANGLE_INSPECTOR_OPACITY_ID,
    };
    let rectangle = RectangleAnnotation {
        id: id("pdf:live-rectangle"),
        page_index: 0,
        rect: PdfRect::new(72., 96., 144., 96.).unwrap(),
        rotation_degrees: 0.,
        appearance: rect_appearance(),
        locked: false,
    };
    let rectangle_id = rectangle.id.clone();
    let (workspace, document_id, cx) =
        open_with_selection(cx, vec![Annotation::Rectangle(rectangle)], &rectangle_id);
    let number = |fill: bool| -> Box<dyn Fn(&Workspace, &gpui::VisualTestContext) -> String> {
        Box::new(move |workspace, cx| {
            workspace.read_with(cx, |workspace, cx| {
                let inspector = workspace.rectangle_property_inspector().unwrap();
                let inspector = inspector.read(cx);
                if fill {
                    inspector.fill_opacity_input()
                } else {
                    inspector.opacity_input()
                }
                .read(cx)
                .value()
                .to_string()
            })
        })
    };
    let opacity = SliderCase {
        track: RECTANGLE_INSPECTOR_OPACITY_ID,
        row: true,
        original: 1.,
        scale: 1.,
        number_scale: 100.,
        y_from: None,
        drawn: Box::new(|scene| scene.rectangles[0].appearance.opacity()),
        stored: Box::new(|snapshot| snapshot.rectangles[0].appearance.opacity()),
        number: number(false),
    };
    drag_slider(&workspace, document_id, cx, &opacity);
    drag_and_return(&workspace, document_id, cx, &opacity);
    escape_cancels_the_drag(&workspace, document_id, cx, &opacity);
    let fill_opacity = SliderCase {
        track: RECTANGLE_INSPECTOR_FILL_OPACITY_ID,
        row: true,
        original: 1.,
        scale: 1.,
        number_scale: 100.,
        y_from: None,
        drawn: Box::new(|scene| scene.rectangles[0].appearance.fill_opacity()),
        stored: Box::new(|snapshot| snapshot.rectangles[0].appearance.fill_opacity()),
        number: number(true),
    };
    drag_slider(&workspace, document_id, cx, &fill_opacity);
    drag_and_return(&workspace, document_id, cx, &fill_opacity);

    // The rotation slider: 0 to 359 degrees, the rectangle is upright.
    let rotation = SliderCase {
        track: "rectangle-property-inspector-rotation-slider",
        row: false,
        original: 0.,
        scale: 359.,
        number_scale: 1.,
        y_from: None,
        drawn: Box::new(|scene| scene.rectangles[0].rotation_degrees),
        stored: Box::new(|snapshot| snapshot.rectangles[0].rotation_degrees),
        number: Box::new(|workspace, cx| {
            workspace.read_with(cx, |workspace, cx| {
                workspace
                    .rectangle_property_inspector()
                    .unwrap()
                    .read(cx)
                    .rotation_input()
                    .read(cx)
                    .value()
                    .to_string()
            })
        }),
    };
    drag_slider(&workspace, document_id, cx, &rotation);
    drag_and_return(&workspace, document_id, cx, &rotation);
}

#[gpui::test]
fn the_ellipse_opacity_slider_updates_the_page_while_it_is_dragged(cx: &mut TestAppContext) {
    use butter_paper::rectangle_property_inspector::ELLIPSE_INSPECTOR_OPACITY_ID;
    let ellipse = EllipseAnnotation::new(
        id("pdf:live-ellipse"),
        0,
        PdfRect::new(120., 400., 160., 96.).unwrap(),
        rect_appearance(),
    )
    .unwrap();
    let ellipse_id = ellipse.id.clone();
    let (workspace, document_id, cx) =
        open_with_selection(cx, vec![Annotation::Ellipse(ellipse)], &ellipse_id);
    let case = SliderCase {
        track: ELLIPSE_INSPECTOR_OPACITY_ID,
        row: true,
        original: 1.,
        scale: 1.,
        number_scale: 100.,
        y_from: None,
        drawn: Box::new(|scene| scene.ellipses[0].appearance.opacity()),
        stored: Box::new(|snapshot| snapshot.ellipses[0].appearance.opacity()),
        number: Box::new(|workspace, cx| {
            workspace.read_with(cx, |workspace, cx| {
                workspace
                    .ellipse_property_inspector()
                    .unwrap()
                    .read(cx)
                    .opacity_input()
                    .read(cx)
                    .value()
                    .to_string()
            })
        }),
    };
    drag_slider(&workspace, document_id, cx, &case);
    drag_and_return(&workspace, document_id, cx, &case);
}

#[gpui::test]
fn the_line_opacity_slider_updates_the_page_while_it_is_dragged(cx: &mut TestAppContext) {
    let line = StraightLineAnnotation::new(
        id("pdf:live-line"),
        0,
        PdfPoint::new(72., 144.).unwrap(),
        PdfPoint::new(252., 240.).unwrap(),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let line_id = line.id.clone();
    let (workspace, document_id, cx) =
        open_with_selection(cx, vec![Annotation::StraightLine(line)], &line_id);
    let case = SliderCase {
        track:
            butter_paper::straight_line_property_inspector::STRAIGHT_LINE_INSPECTOR_OPACITY_TRACK_ID,
        row: false,
        original: 1.,
        scale: 1.,
        number_scale: 100.,
        y_from: None,
        drawn: Box::new(|scene| scene.straight_lines[0].appearance.opacity()),
        stored: Box::new(|snapshot| snapshot.straight_lines[0].appearance.opacity()),
        number: Box::new(|workspace, cx| {
            workspace.read_with(cx, |workspace, cx| {
                workspace
                    .straight_line_property_inspector()
                    .unwrap()
                    .read(cx)
                    .opacity_input()
                    .read(cx)
                    .value()
                    .to_string()
            })
        }),
    };
    drag_slider(&workspace, document_id, cx, &case);
    drag_and_return(&workspace, document_id, cx, &case);
    escape_cancels_the_drag(&workspace, document_id, cx, &case);
}

#[gpui::test]
fn the_polyline_opacity_slider_updates_the_page_while_it_is_dragged(cx: &mut TestAppContext) {
    let polyline = VertexPathAnnotation::new(
        id("pdf:live-polyline"),
        0,
        vec![
            PdfPoint::new(72., 144.).unwrap(),
            PdfPoint::new(252., 240.).unwrap(),
        ],
        VertexPathKind::Polyline,
        rect_appearance(),
    )
    .unwrap();
    let polyline_id = polyline.id.clone();
    let (workspace, document_id, cx) =
        open_with_selection(cx, vec![Annotation::VertexPath(polyline)], &polyline_id);
    let case = SliderCase {
        track: butter_paper::vertex_path_property_inspector::VERTEX_PATH_INSPECTOR_OPACITY_ID,
        row: false,
        original: 1.,
        scale: 1.,
        number_scale: 100.,
        y_from: None,
        drawn: Box::new(|scene| scene.vertex_paths[0].appearance.opacity()),
        stored: Box::new(|snapshot| snapshot.vertex_paths[0].appearance.opacity()),
        number: Box::new(|workspace, cx| {
            workspace.read_with(cx, |workspace, cx| {
                workspace
                    .vertex_path_property_inspector()
                    .unwrap()
                    .read(cx)
                    .opacity_input()
                    .read(cx)
                    .value()
                    .to_string()
            })
        }),
    };
    drag_slider(&workspace, document_id, cx, &case);
    drag_and_return(&workspace, document_id, cx, &case);
}

#[gpui::test]
fn the_dimension_opacity_slider_updates_the_page_while_it_is_dragged(cx: &mut TestAppContext) {
    let dimension = DimensionAnnotation::new(
        id("pdf:live-dimension"),
        0,
        PdfPoint::new(72., 192.).unwrap(),
        PdfPoint::new(216., 192.).unwrap(),
        60.,
        "1200 mm",
        DimensionAppearance::new(
            StraightLineAppearance::default_for(LineKind::Line),
            TextBoxStyle::new("Tinos", 12., "#000000", 1.).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let dimension_id = dimension.id.clone();
    let (workspace, document_id, cx) =
        open_with_selection(cx, vec![Annotation::Dimension(dimension)], &dimension_id);
    let case = SliderCase {
        track: butter_paper::dimension_property_inspector::DIMENSION_INSPECTOR_OPACITY_ID,
        row: false,
        original: 1.,
        scale: 1.,
        number_scale: 100.,
        y_from: None,
        drawn: Box::new(|scene| scene.dimensions[0].appearance.line().opacity()),
        stored: Box::new(|snapshot| snapshot.dimensions[0].appearance.line().opacity()),
        number: Box::new(|workspace, cx| {
            workspace.read_with(cx, |workspace, cx| {
                workspace
                    .dimension_property_inspector()
                    .unwrap()
                    .read(cx)
                    .opacity_input()
                    .read(cx)
                    .value()
                    .to_string()
            })
        }),
    };
    drag_slider(&workspace, document_id, cx, &case);
    drag_and_return(&workspace, document_id, cx, &case);
}

#[gpui::test]
fn the_text_box_opacity_slider_updates_the_page_while_it_is_dragged(cx: &mut TestAppContext) {
    let text_box = TextBoxAnnotation::new(
        id("pdf:live-text-box"),
        0,
        PdfRect::new(72., 180., 240., 72.).unwrap(),
        "Opacity",
        TextBoxStyle::new("Helvetica", 12., "#111827", 1.).unwrap(),
    )
    .unwrap();
    let text_box_id = text_box.id.clone();
    let (workspace, document_id, cx) =
        open_with_selection(cx, vec![Annotation::TextBox(text_box)], &text_box_id);
    let case = SliderCase {
        track: butter_paper::text_box_property_inspector::TEXT_BOX_INSPECTOR_OPACITY_TRACK_ID,
        row: false,
        original: 1.,
        scale: 1.,
        number_scale: 100.,
        y_from: None,
        drawn: Box::new(|scene| scene.text_boxes[0].style.opacity()),
        stored: Box::new(|snapshot| snapshot.text_boxes[0].style().opacity()),
        number: Box::new(|workspace, cx| {
            workspace.read_with(cx, |workspace, cx| {
                workspace
                    .text_box_property_inspector()
                    .unwrap()
                    .read(cx)
                    .opacity_input()
                    .read(cx)
                    .value()
                    .to_string()
            })
        }),
    };
    drag_slider(&workspace, document_id, cx, &case);
    drag_and_return(&workspace, document_id, cx, &case);
}

fn engineering_number() -> Box<dyn Fn(&Workspace, &gpui::VisualTestContext) -> String> {
    Box::new(|workspace, cx| {
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .engineering_visual_property_inspector()
                .unwrap()
                .read(cx)
                .opacity_input()
                .read(cx)
                .value()
                .to_string()
        })
    })
}

#[gpui::test]
fn the_arc_opacity_slider_updates_the_page_while_it_is_dragged(cx: &mut TestAppContext) {
    let arc = ArcAnnotation::new(
        id("pdf:live-arc"),
        0,
        PdfPoint::new(20., 20.).unwrap(),
        PdfPoint::new(100., 20.).unwrap(),
        PdfPoint::new(60., 60.).unwrap(),
        RectangleAppearance::new("#112233", 1., None::<String>, 1.).unwrap(),
    )
    .unwrap();
    let arc_id = arc.id.clone();
    let (workspace, document_id, cx) = open_with_selection(cx, vec![Annotation::Arc(arc)], &arc_id);
    let case = SliderCase {
        track: butter_paper::engineering_visual_property_inspector::ENGINEERING_VISUAL_INSPECTOR_OPACITY_TRACK_ID,
        row: false,
        original: 1.,
        scale: 1.,
        number_scale: 100.,
        y_from: None,
        drawn: Box::new(|scene| scene.arcs[0].appearance.opacity()),
        stored: Box::new(|snapshot| snapshot.arcs[0].appearance.opacity()),
        number: engineering_number(),
    };
    drag_slider(&workspace, document_id, cx, &case);
    drag_and_return(&workspace, document_id, cx, &case);
    escape_cancels_the_drag(&workspace, document_id, cx, &case);
}

#[gpui::test]
fn the_cloud_opacity_slider_updates_the_page_while_it_is_dragged(cx: &mut TestAppContext) {
    let cloud = CloudAnnotation::new(
        id("pdf:live-cloud"),
        0,
        vec![
            PdfPoint::new(120., 20.).unwrap(),
            PdfPoint::new(200., 20.).unwrap(),
            PdfPoint::new(200., 90.).unwrap(),
            PdfPoint::new(120., 90.).unwrap(),
        ],
        1.,
        RectangleAppearance::new("#112233", 1., None::<String>, 1.).unwrap(),
    )
    .unwrap();
    let cloud_id = cloud.id.clone();
    let (workspace, document_id, cx) =
        open_with_selection(cx, vec![Annotation::Cloud(cloud)], &cloud_id);
    let case = SliderCase {
        track: butter_paper::engineering_visual_property_inspector::ENGINEERING_VISUAL_INSPECTOR_OPACITY_TRACK_ID,
        row: false,
        original: 1.,
        scale: 1.,
        number_scale: 100.,
        y_from: None,
        drawn: Box::new(|scene| scene.clouds[0].appearance.opacity()),
        stored: Box::new(|snapshot| snapshot.clouds[0].appearance.opacity()),
        number: engineering_number(),
    };
    drag_slider(&workspace, document_id, cx, &case);
    drag_and_return(&workspace, document_id, cx, &case);
}

#[gpui::test]
fn the_image_and_snapshot_opacity_sliders_update_the_page_while_dragged(cx: &mut TestAppContext) {
    let image = ImageAnnotation::new(
        id("pdf:live-image"),
        0,
        PdfRect::new(20., 100., 80., 60.).unwrap(),
        DecodedRgbaAsset::new(1, 1, vec![255, 255, 255, 255]).unwrap(),
        false,
    )
    .unwrap();
    let snapshot_markup = SnapshotAnnotation::new(
        id("pdf:live-snapshot"),
        0,
        PdfRect::new(120., 100., 80., 60.).unwrap(),
        DecodedRgbaAsset::new(1, 1, vec![255, 255, 255, 255]).unwrap(),
        1.,
    )
    .unwrap();
    let image_id = image.id.clone();
    let snapshot_id = snapshot_markup.id.clone();
    let (workspace, document_id, cx) = open_with_selection(
        cx,
        vec![
            Annotation::Image(image),
            Annotation::Snapshot(snapshot_markup),
        ],
        &image_id,
    );
    let image_case = SliderCase {
        track: butter_paper::engineering_visual_property_inspector::ENGINEERING_VISUAL_INSPECTOR_OPACITY_TRACK_ID,
        row: false,
        original: 1.,
        scale: 1.,
        number_scale: 100.,
        y_from: None,
        drawn: Box::new(|scene| scene.images[0].opacity),
        stored: Box::new(|snapshot| snapshot.images[0].opacity()),
        number: engineering_number(),
    };
    drag_slider(&workspace, document_id, cx, &image_case);
    drag_and_return(&workspace, document_id, cx, &image_case);

    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(document_id, &snapshot_id, cx)
    }));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let snapshot_case = SliderCase {
        track: butter_paper::engineering_visual_property_inspector::ENGINEERING_VISUAL_INSPECTOR_OPACITY_TRACK_ID,
        row: false,
        original: 1.,
        scale: 1.,
        number_scale: 100.,
        y_from: None,
        drawn: Box::new(|scene| scene.snapshots[0].opacity),
        stored: Box::new(|snapshot| snapshot.snapshots[0].opacity()),
        number: engineering_number(),
    };
    drag_slider(&workspace, document_id, cx, &snapshot_case);
    drag_and_return(&workspace, document_id, cx, &snapshot_case);
}

fn caption_number(
    field: &'static str,
) -> Box<dyn Fn(&Workspace, &gpui::VisualTestContext) -> String> {
    Box::new(move |workspace, cx| {
        workspace.read_with(cx, |workspace, cx| {
            let inspector = workspace.caption_property_inspector().unwrap();
            let inspector = inspector.read(cx);
            match field {
                "opacity" => inspector.opacity_input(),
                _ => inspector.intensity_input(),
            }
            .read(cx)
            .value()
            .to_string()
        })
    })
}

#[gpui::test]
fn the_callout_opacity_slider_updates_the_page_while_it_is_dragged(cx: &mut TestAppContext) {
    let callout = CalloutAnnotation::new(
        id("pdf:live-callout"),
        0,
        vec![
            PdfPoint::new(80., 350.).unwrap(),
            PdfPoint::new(150., 380.).unwrap(),
            PdfPoint::new(220., 380.).unwrap(),
        ],
        PdfRect::new(220., 360., 120., 40.).unwrap(),
        "Callout",
        CalloutAppearance::new(
            StraightLineAppearance::new("#112233", 1., 1., StrokeStyle::Solid).unwrap(),
            TextBoxStyle::new("Helvetica", 12., "#112233", 1.).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let callout_id = callout.id.clone();
    let (workspace, document_id, cx) =
        open_with_selection(cx, vec![Annotation::Callout(callout)], &callout_id);
    let case = SliderCase {
        track: "caption-property-opacity-slider",
        row: false,
        original: 1.,
        scale: 1.,
        number_scale: 100.,
        y_from: None,
        drawn: Box::new(|scene| scene.callouts[0].appearance.line().opacity()),
        stored: Box::new(|snapshot| snapshot.callouts[0].appearance.line().opacity()),
        number: caption_number("opacity"),
    };
    drag_slider(&workspace, document_id, cx, &case);
    drag_and_return(&workspace, document_id, cx, &case);
    escape_cancels_the_drag(&workspace, document_id, cx, &case);
}

fn test_cloud_plus() -> CloudPlusAnnotation {
    CloudPlusAnnotation::new(
        id("pdf:live-cloud-plus"),
        0,
        vec![
            PdfPoint::new(100., 300.).unwrap(),
            PdfPoint::new(260., 300.).unwrap(),
            PdfPoint::new(260., 400.).unwrap(),
            PdfPoint::new(100., 400.).unwrap(),
        ],
        1.,
        vec![
            PdfPoint::new(180., 350.).unwrap(),
            PdfPoint::new(300., 350.).unwrap(),
            PdfPoint::new(330., 350.).unwrap(),
        ],
        PdfRect::new(330., 330., 120., 40.).unwrap(),
        "Cloud+",
        CloudPlusAppearance::new(
            RectangleAppearance::new("#112233", 1., None::<String>, 1.).unwrap(),
            StraightLineAppearance::new("#112233", 1., 1., StrokeStyle::Solid).unwrap(),
            TextBoxStyle::new("Helvetica", 12., "#112233", 1.).unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}

#[gpui::test]
fn the_cloud_plus_opacity_slider_updates_the_page_while_it_is_dragged(cx: &mut TestAppContext) {
    let cloud_plus = test_cloud_plus();
    let cloud_plus_id = cloud_plus.id.clone();
    let (workspace, document_id, cx) =
        open_with_selection(cx, vec![Annotation::CloudPlus(cloud_plus)], &cloud_plus_id);
    let opacity = SliderCase {
        track: "caption-property-opacity-slider",
        row: false,
        original: 1.,
        scale: 1.,
        number_scale: 100.,
        y_from: None,
        drawn: Box::new(|scene| scene.cloud_pluses[0].appearance.cloud().opacity()),
        stored: Box::new(|snapshot| snapshot.cloud_pluses[0].appearance.cloud().opacity()),
        number: caption_number("opacity"),
    };
    drag_slider(&workspace, document_id, cx, &opacity);
    drag_and_return(&workspace, document_id, cx, &opacity);
    escape_cancels_the_drag(&workspace, document_id, cx, &opacity);
}

// Typed fields: each keystroke previews the value on the markup, leaving the
// field or Enter commits one undo step, and Escape restores the old value.

#[cfg(target_os = "macos")]
const SELECT_ALL: &str = "cmd-a";
#[cfg(not(target_os = "macos"))]
const SELECT_ALL: &str = "ctrl-a";

/// Clicks the field `selector` and selects its text.
fn focus_field(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} must render"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.simulate_keystrokes(SELECT_ALL);
}

/// Leaves the field: the workspace takes the focus.
fn leave_field(workspace: &Workspace, cx: &mut gpui::VisualTestContext) {
    let focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| focus.focus(window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn text_box_fixture(
    cx: &mut TestAppContext,
) -> (Workspace, DocumentId, &mut gpui::VisualTestContext) {
    let text_box = TextBoxAnnotation::new(
        id("pdf:typed-text-box"),
        0,
        PdfRect::new(72., 180., 240., 72.).unwrap(),
        "Opacity",
        TextBoxStyle::new("Helvetica", 12., "#111827", 1.).unwrap(),
    )
    .unwrap();
    let text_box_id = text_box.id.clone();
    open_with_selection(cx, vec![Annotation::TextBox(text_box)], &text_box_id)
}

fn drawn_text(
    workspace: &Workspace,
    document_id: DocumentId,
    cx: &mut gpui::VisualTestContext,
) -> String {
    scene(workspace, document_id, cx).text_boxes[0]
        .content
        .clone()
}

#[gpui::test]
fn typing_in_a_text_field_shows_on_the_markup_and_commits_one_step_on_leaving(
    cx: &mut TestAppContext,
) {
    use butter_paper::text_box_property_inspector::TEXT_BOX_INSPECTOR_TEXT_ID;
    let (workspace, document_id, cx) = text_box_fixture(cx);
    let before = history(&workspace, document_id, cx);
    focus_field(cx, TEXT_BOX_INSPECTOR_TEXT_ID);
    for (typed, expected) in [("n", "n"), ("e", "ne"), ("w", "new")] {
        cx.simulate_keystrokes(typed);
        assert_eq!(
            drawn_text(&workspace, document_id, cx),
            expected,
            "each keystroke shows on the page"
        );
        assert_eq!(
            history(&workspace, document_id, cx),
            before,
            "nothing is committed while typing"
        );
    }
    leave_field(&workspace, cx);
    let after = history(&workspace, document_id, cx);
    assert_eq!(
        (after.undo_depth, after.dirty),
        (before.undo_depth + 1, true),
        "leaving the field is one undo step"
    );
    assert_eq!(
        snapshot(&workspace, document_id, cx).text_boxes[0].content(),
        "new"
    );
    assert_eq!(drawn_text(&workspace, document_id, cx), "new");
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    assert_eq!(drawn_text(&workspace, document_id, cx), "Opacity");
    assert_eq!(
        history(&workspace, document_id, cx).undo_depth,
        before.undo_depth
    );
}

#[gpui::test]
fn escape_in_a_text_field_restores_the_text_from_before_it_took_focus(cx: &mut TestAppContext) {
    use butter_paper::text_box_property_inspector::TEXT_BOX_INSPECTOR_TEXT_ID;
    let (workspace, document_id, cx) = text_box_fixture(cx);
    let before = history(&workspace, document_id, cx);
    focus_field(cx, TEXT_BOX_INSPECTOR_TEXT_ID);
    cx.simulate_keystrokes("x y");
    assert_eq!(drawn_text(&workspace, document_id, cx), "xy");
    cx.simulate_keystrokes("escape");
    assert_eq!(
        drawn_text(&workspace, document_id, cx),
        "Opacity",
        "Escape shows the original text"
    );
    let field = workspace.read_with(cx, |workspace, cx| {
        workspace
            .text_box_property_inspector()
            .unwrap()
            .read(cx)
            .content_input()
            .read(cx)
            .value()
            .to_string()
    });
    assert_eq!(field, "Opacity", "Escape restores the field");
    leave_field(&workspace, cx);
    assert_eq!(
        history(&workspace, document_id, cx),
        before,
        "leaving the field after Escape commits nothing"
    );
    assert_eq!(drawn_text(&workspace, document_id, cx), "Opacity");
}

#[gpui::test]
fn typing_a_line_width_previews_each_keystroke_and_keeps_the_last_valid_one(
    cx: &mut TestAppContext,
) {
    use butter_paper::straight_line_property_inspector::STRAIGHT_LINE_INSPECTOR_WIDTH_ID;
    let line = StraightLineAnnotation::new(
        id("pdf:typed-line"),
        0,
        PdfPoint::new(72., 144.).unwrap(),
        PdfPoint::new(252., 240.).unwrap(),
        LineKind::Line,
        StraightLineAppearance::new("#112233", 1., 1., StrokeStyle::Solid).unwrap(),
    )
    .unwrap();
    let line_id = line.id.clone();
    let (workspace, document_id, cx) =
        open_with_selection(cx, vec![Annotation::StraightLine(line)], &line_id);
    let before = history(&workspace, document_id, cx);
    let width = |cx: &mut gpui::VisualTestContext| {
        scene(&workspace, document_id, cx).straight_lines[0]
            .appearance
            .stroke_width_pt()
    };
    focus_field(cx, STRAIGHT_LINE_INSPECTOR_WIDTH_ID);
    cx.simulate_keystrokes("5");
    near(width(cx), 5., 0.0001, "after 5");
    // Emptying the field is not a value yet: the page keeps 5.
    cx.simulate_keystrokes("backspace");
    near(
        width(cx),
        5.,
        0.0001,
        "an empty field keeps the last preview",
    );
    cx.simulate_keystrokes("7");
    near(width(cx), 7., 0.0001, "after 7");
    // "7." reads as 7 and "7.5" as 7.5; "." alone is not a number.
    cx.simulate_keystrokes("backspace");
    cx.simulate_keystrokes(".");
    near(width(cx), 7., 0.0001, "a lone point keeps the last preview");
    cx.simulate_keystrokes("5");
    near(width(cx), 0.5, 0.0001, "after .5");
    cx.simulate_keystrokes("backspace backspace 7 . 5");
    near(width(cx), 7.5, 0.0001, "after 7.5");
    assert_eq!(history(&workspace, document_id, cx), before);
    cx.simulate_keystrokes("enter");
    let after = history(&workspace, document_id, cx);
    assert_eq!(
        (after.undo_depth, after.dirty),
        (before.undo_depth + 1, true),
        "Enter is one undo step"
    );
    near(
        snapshot(&workspace, document_id, cx).straight_lines[0]
            .appearance
            .stroke_width_pt(),
        7.5,
        0.0001,
        "committed",
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    near(width(cx), 1., 0.0001, "one undo restores the width");
}

#[gpui::test]
fn an_invalid_width_left_in_the_field_ends_the_preview_and_commits_nothing(
    cx: &mut TestAppContext,
) {
    use butter_paper::straight_line_property_inspector::STRAIGHT_LINE_INSPECTOR_WIDTH_ID;
    let line = StraightLineAnnotation::new(
        id("pdf:typed-line-invalid"),
        0,
        PdfPoint::new(72., 144.).unwrap(),
        PdfPoint::new(252., 240.).unwrap(),
        LineKind::Line,
        StraightLineAppearance::new("#112233", 1., 1., StrokeStyle::Solid).unwrap(),
    )
    .unwrap();
    let line_id = line.id.clone();
    let (workspace, document_id, cx) =
        open_with_selection(cx, vec![Annotation::StraightLine(line)], &line_id);
    let before = history(&workspace, document_id, cx);
    focus_field(cx, STRAIGHT_LINE_INSPECTOR_WIDTH_ID);
    cx.simulate_keystrokes("4");
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.property_preview_active(document_id, cx)
    }));
    cx.simulate_keystrokes("backspace");
    leave_field(&workspace, cx);
    assert_eq!(history(&workspace, document_id, cx), before);
    assert!(!workspace.read_with(cx, |workspace, cx| {
        workspace.property_preview_active(document_id, cx)
    }));
    near(
        scene(&workspace, document_id, cx).straight_lines[0]
            .appearance
            .stroke_width_pt(),
        1.,
        0.0001,
        "the committed width is shown again",
    );
}

// Size fields have − and + buttons and step with Up and Down.

/// Presses the − or + button of the number input `selector`: its buttons sit
/// at the two ends of its frame.
fn press_step_button(cx: &mut gpui::VisualTestContext, selector: &'static str, plus: bool) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} must render"));
    let x = if plus {
        bounds.right() - gpui::px(14.)
    } else {
        bounds.left() + gpui::px(14.)
    };
    cx.simulate_click(point(x, bounds.center().y), Modifiers::default());
}

fn line_fixture(cx: &mut TestAppContext) -> (Workspace, DocumentId, &mut gpui::VisualTestContext) {
    let line = StraightLineAnnotation::new(
        id("pdf:stepped-line"),
        0,
        PdfPoint::new(72., 144.).unwrap(),
        PdfPoint::new(252., 240.).unwrap(),
        LineKind::Line,
        StraightLineAppearance::new("#112233", 1., 1., StrokeStyle::Solid).unwrap(),
    )
    .unwrap();
    let line_id = line.id.clone();
    open_with_selection(cx, vec![Annotation::StraightLine(line)], &line_id)
}

fn line_width(
    workspace: &Workspace,
    document_id: DocumentId,
    cx: &mut gpui::VisualTestContext,
) -> f64 {
    scene(workspace, document_id, cx).straight_lines[0]
        .appearance
        .stroke_width_pt()
}

fn width_text(workspace: &Workspace, cx: &gpui::VisualTestContext) -> String {
    workspace.read_with(cx, |workspace, cx| {
        workspace
            .straight_line_property_inspector()
            .unwrap()
            .read(cx)
            .width_input()
            .read(cx)
            .value()
            .to_string()
    })
}

#[gpui::test]
fn the_colour_and_the_width_have_rows_of_their_own(cx: &mut TestAppContext) {
    use butter_paper::straight_line_property_inspector::{
        STRAIGHT_LINE_INSPECTOR_COLOR_TRIGGER_ID, STRAIGHT_LINE_INSPECTOR_WIDTH_ID,
    };
    let (_workspace, _document_id, cx) = line_fixture(cx);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let colour = cx
        .debug_bounds(STRAIGHT_LINE_INSPECTOR_COLOR_TRIGGER_ID)
        .unwrap();
    let width = cx.debug_bounds(STRAIGHT_LINE_INSPECTOR_WIDTH_ID).unwrap();
    assert!(
        width.top() >= colour.bottom(),
        "the width is below the colour: {colour:?} {width:?}"
    );
    assert!(
        width.size.width >= gpui::px(96.),
        "the width has room for its buttons: {width:?}"
    );
}

#[gpui::test]
fn the_plus_and_minus_buttons_step_the_width_and_commit_each_press(cx: &mut TestAppContext) {
    use butter_paper::straight_line_property_inspector::STRAIGHT_LINE_INSPECTOR_WIDTH_ID as WIDTH;
    let (workspace, document_id, cx) = line_fixture(cx);
    let start = history(&workspace, document_id, cx);
    press_step_button(cx, WIDTH, true);
    near(
        line_width(&workspace, document_id, cx),
        1.5,
        0.0001,
        "after +",
    );
    assert_eq!(width_text(&workspace, cx), "1.5");
    assert!(!workspace.read_with(cx, |workspace, cx| {
        workspace.property_preview_active(document_id, cx)
    }));
    let after_plus = history(&workspace, document_id, cx);
    assert_eq!(
        (after_plus.undo_depth, after_plus.dirty),
        (start.undo_depth + 1, true),
        "a press is one undo step"
    );
    near(
        snapshot(&workspace, document_id, cx).straight_lines[0]
            .appearance
            .stroke_width_pt(),
        1.5,
        0.0001,
        "committed",
    );
    press_step_button(cx, WIDTH, false);
    press_step_button(cx, WIDTH, false);
    near(
        line_width(&workspace, document_id, cx),
        0.5,
        0.0001,
        "after − −",
    );
    assert_eq!(
        history(&workspace, document_id, cx).undo_depth,
        start.undo_depth + 3,
        "each press is an undo step"
    );
    // One undo takes back the last press only.
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    near(line_width(&workspace, document_id, cx), 1., 0.0001, "undo");
}

#[gpui::test]
fn the_width_steps_stop_at_its_smallest_and_largest_values(cx: &mut TestAppContext) {
    use butter_paper::straight_line_property_inspector::STRAIGHT_LINE_INSPECTOR_WIDTH_ID as WIDTH;
    let (workspace, document_id, cx) = line_fixture(cx);
    press_step_button(cx, WIDTH, false);
    press_step_button(cx, WIDTH, false);
    near(
        line_width(&workspace, document_id, cx),
        0.25,
        0.0001,
        "held at the smallest",
    );
    let at_minimum = history(&workspace, document_id, cx);
    press_step_button(cx, WIDTH, false);
    assert_eq!(
        history(&workspace, document_id, cx),
        at_minimum,
        "a press at the smallest value changes nothing"
    );
    assert_eq!(width_text(&workspace, cx), "0.25");
    focus_field(cx, WIDTH);
    cx.simulate_keystrokes("2 3 . 8 enter");
    near(
        line_width(&workspace, document_id, cx),
        23.8,
        0.0001,
        "typed",
    );
    press_step_button(cx, WIDTH, true);
    near(
        line_width(&workspace, document_id, cx),
        24.,
        0.0001,
        "held at the largest",
    );
    let at_maximum = history(&workspace, document_id, cx);
    press_step_button(cx, WIDTH, true);
    assert_eq!(history(&workspace, document_id, cx), at_maximum);
}

#[gpui::test]
fn the_up_and_down_keys_step_the_width(cx: &mut TestAppContext) {
    use butter_paper::straight_line_property_inspector::STRAIGHT_LINE_INSPECTOR_WIDTH_ID as WIDTH;
    let (workspace, document_id, cx) = line_fixture(cx);
    let start = history(&workspace, document_id, cx);
    focus_field(cx, WIDTH);
    cx.simulate_keystrokes("up");
    near(
        line_width(&workspace, document_id, cx),
        1.5,
        0.0001,
        "after Up",
    );
    cx.simulate_keystrokes("up");
    near(
        line_width(&workspace, document_id, cx),
        2.,
        0.0001,
        "after Up Up",
    );
    cx.simulate_keystrokes("down");
    near(
        line_width(&workspace, document_id, cx),
        1.5,
        0.0001,
        "after Down",
    );
    assert_eq!(
        history(&workspace, document_id, cx).undo_depth,
        start.undo_depth + 3
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    near(line_width(&workspace, document_id, cx), 2., 0.0001, "undo");
}

#[gpui::test]
fn the_font_size_and_the_dimension_offset_step_by_their_own_amounts(cx: &mut TestAppContext) {
    use butter_paper::dimension_property_inspector::{
        DIMENSION_INSPECTOR_FONT_SIZE_ID, DIMENSION_INSPECTOR_OFFSET_ID,
    };
    let dimension = DimensionAnnotation::new(
        id("pdf:stepped-dimension"),
        0,
        PdfPoint::new(72., 192.).unwrap(),
        PdfPoint::new(216., 192.).unwrap(),
        60.,
        "1200 mm",
        DimensionAppearance::new(
            StraightLineAppearance::default_for(LineKind::Line),
            TextBoxStyle::new("Tinos", 12., "#000000", 1.).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let dimension_id = dimension.id.clone();
    let (workspace, document_id, cx) =
        open_with_selection(cx, vec![Annotation::Dimension(dimension)], &dimension_id);
    press_step_button(cx, DIMENSION_INSPECTOR_FONT_SIZE_ID, true);
    near(
        scene(&workspace, document_id, cx).dimensions[0]
            .appearance
            .text()
            .font_size_pt(),
        13.,
        0.0001,
        "font size steps by 1 pt",
    );
    press_step_button(cx, DIMENSION_INSPECTOR_OFFSET_ID, true);
    near(
        scene(&workspace, document_id, cx).dimensions[0].dimension_line_offset,
        61.,
        0.0001,
        "offset steps by 1 pt",
    );
}

// The wheel over a number or a slider steps it.

fn wheel(
    cx: &mut gpui::VisualTestContext,
    at: Point<Pixels>,
    delta: gpui::ScrollDelta,
    shift: bool,
) {
    cx.simulate_mouse_move(at, None, Modifiers::default());
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: at,
        delta,
        modifiers: Modifiers {
            shift,
            ..Default::default()
        },
        touch_phase: gpui::TouchPhase::Moved,
    });
}

/// A notch of the mouse wheel: up is positive.
fn notch(up: bool) -> gpui::ScrollDelta {
    let lines = butter_paper::mouse_wheel::notch_lines();
    gpui::ScrollDelta::Lines(point(0., if up { lines } else { -lines }))
}

fn field_centre(cx: &mut gpui::VisualTestContext, selector: &'static str) -> Point<Pixels> {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} must render"))
        .center()
}

/// The wheel has rested: the burst is committed.
fn rest(cx: &mut gpui::VisualTestContext) {
    cx.executor()
        .advance_clock(butter_paper::property_wheel::WHEEL_IDLE * 2);
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui::test]
fn a_wheel_notch_steps_the_width_shows_it_and_a_burst_is_one_undo_step(cx: &mut TestAppContext) {
    use butter_paper::straight_line_property_inspector::STRAIGHT_LINE_INSPECTOR_WIDTH_ID as WIDTH;
    let (workspace, document_id, cx) = line_fixture(cx);
    let start = history(&workspace, document_id, cx);
    let at = field_centre(cx, WIDTH);
    wheel(cx, at, notch(true), false);
    near(
        line_width(&workspace, document_id, cx),
        1.5,
        0.0001,
        "after a notch up",
    );
    assert_eq!(width_text(&workspace, cx), "1.5");
    assert_eq!(
        history(&workspace, document_id, cx),
        start,
        "the wheel previews until it rests"
    );
    wheel(cx, at, notch(true), false);
    wheel(cx, at, notch(true), false);
    near(
        line_width(&workspace, document_id, cx),
        2.5,
        0.0001,
        "after three notches up",
    );
    wheel(cx, at, notch(false), false);
    near(
        line_width(&workspace, document_id, cx),
        2.,
        0.0001,
        "after a notch down",
    );
    rest(cx);
    let after = history(&workspace, document_id, cx);
    assert_eq!(
        (after.undo_depth, after.dirty),
        (start.undo_depth + 1, true),
        "a burst of notches is one undo step"
    );
    near(
        snapshot(&workspace, document_id, cx).straight_lines[0]
            .appearance
            .stroke_width_pt(),
        2.,
        0.0001,
        "committed",
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    near(
        line_width(&workspace, document_id, cx),
        1.,
        0.0001,
        "one undo takes back the burst",
    );
}

#[gpui::test]
fn the_wheel_stops_at_the_smallest_and_largest_values_and_shift_steps_by_ten(
    cx: &mut TestAppContext,
) {
    use butter_paper::straight_line_property_inspector::STRAIGHT_LINE_INSPECTOR_WIDTH_ID as WIDTH;
    let (workspace, document_id, cx) = line_fixture(cx);
    let at = field_centre(cx, WIDTH);
    for _ in 0..4 {
        wheel(cx, at, notch(false), false);
    }
    near(
        line_width(&workspace, document_id, cx),
        0.25,
        0.0001,
        "held at the smallest",
    );
    rest(cx);
    wheel(cx, at, notch(true), true);
    near(
        line_width(&workspace, document_id, cx),
        5.25,
        0.0001,
        "Shift is ten steps",
    );
    wheel(cx, at, notch(true), true);
    wheel(cx, at, notch(true), true);
    wheel(cx, at, notch(true), true);
    wheel(cx, at, notch(true), true);
    near(
        line_width(&workspace, document_id, cx),
        24.,
        0.0001,
        "held at the largest",
    );
    rest(cx);
}

#[gpui::test]
fn trackpad_pixels_add_up_to_whole_steps(cx: &mut TestAppContext) {
    use butter_paper::straight_line_property_inspector::STRAIGHT_LINE_INSPECTOR_WIDTH_ID as WIDTH;
    let (workspace, document_id, cx) = line_fixture(cx);
    let at = field_centre(cx, WIDTH);
    let pixels = |amount: f32| gpui::ScrollDelta::Pixels(point(gpui::px(0.), gpui::px(amount)));
    wheel(cx, at, pixels(10.), false);
    wheel(cx, at, pixels(10.), false);
    near(
        line_width(&workspace, document_id, cx),
        1.,
        0.0001,
        "20 px is not a step",
    );
    wheel(cx, at, pixels(10.), false);
    near(
        line_width(&workspace, document_id, cx),
        1.5,
        0.0001,
        "30 px is a step",
    );
    wheel(cx, at, pixels(-30.), false);
    near(
        line_width(&workspace, document_id, cx),
        1.,
        0.0001,
        "30 px back is a step back",
    );
    rest(cx);
}

#[gpui::test]
fn a_panel_scroll_that_passes_over_a_field_does_not_change_it(cx: &mut TestAppContext) {
    use butter_paper::straight_line_property_inspector::STRAIGHT_LINE_INSPECTOR_WIDTH_ID as WIDTH;
    let (workspace, document_id, cx) = line_fixture(cx);
    let start = history(&workspace, document_id, cx);
    let field = field_centre(cx, WIDTH);
    // The scroll starts over the row's label, left of the field.
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let bounds = cx.debug_bounds(WIDTH).unwrap();
    let label = point(bounds.left() - gpui::px(24.), field.y);
    assert!(
        label.x < bounds.left(),
        "the label point is outside the field"
    );
    wheel(cx, label, notch(true), false);
    wheel(cx, field, notch(true), false);
    wheel(cx, field, notch(true), false);
    near(
        line_width(&workspace, document_id, cx),
        1.,
        0.0001,
        "the panel's scroll passes over",
    );
    assert_eq!(history(&workspace, document_id, cx), start);
    // After the gesture rests, a scroll that starts over the field is the field's.
    rest(cx);
    wheel(cx, field, notch(true), false);
    near(
        line_width(&workspace, document_id, cx),
        1.5,
        0.0001,
        "a new gesture over the field",
    );
    // And that gesture stays the field's while it continues.
    wheel(cx, field, notch(true), false);
    near(
        line_width(&workspace, document_id, cx),
        2.,
        0.0001,
        "it continues",
    );
    rest(cx);
}

#[gpui::test]
fn the_wheel_over_a_slider_steps_it(cx: &mut TestAppContext) {
    use butter_paper::straight_line_property_inspector::STRAIGHT_LINE_INSPECTOR_OPACITY_TRACK_ID as TRACK;
    let (workspace, document_id, cx) = line_fixture(cx);
    let start = history(&workspace, document_id, cx);
    let at = field_centre(cx, TRACK);
    let opacity = |cx: &mut gpui::VisualTestContext| {
        scene(&workspace, document_id, cx).straight_lines[0]
            .appearance
            .opacity()
    };
    wheel(cx, at, notch(false), false);
    near(
        opacity(cx),
        0.99,
        0.0001,
        "a notch down is one step of the slider",
    );
    wheel(cx, at, notch(false), true);
    near(opacity(cx), 0.89, 0.0001, "Shift is ten steps");
    wheel(cx, at, notch(true), false);
    near(opacity(cx), 0.9, 0.0001, "a notch up");
    assert_eq!(
        history(&workspace, document_id, cx),
        start,
        "previewed until it rests"
    );
    rest(cx);
    let after = history(&workspace, document_id, cx);
    assert_eq!(
        after.undo_depth,
        start.undo_depth + 1,
        "one undo step for the burst"
    );
    near(
        snapshot(&workspace, document_id, cx).straight_lines[0]
            .appearance
            .opacity(),
        0.9,
        0.0001,
        "committed",
    );
    assert_eq!(
        width_number(&workspace, cx),
        "90",
        "the number beside the slider follows"
    );
}

fn width_number(workspace: &Workspace, cx: &gpui::VisualTestContext) -> String {
    workspace.read_with(cx, |workspace, cx| {
        workspace
            .straight_line_property_inspector()
            .unwrap()
            .read(cx)
            .opacity_input()
            .read(cx)
            .value()
            .to_string()
    })
}

// Status toasts: only a refusal, a failure or a result the user cannot
// otherwise see shows as a toast. A routine confirmation stays in the status
// bar.

fn toast_count(cx: &mut gpui::VisualTestContext) -> usize {
    cx.run_until_parked();
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    cx.update(|window, cx| gpui_component::WindowExt::notifications(window, cx).len())
}

fn pen_fixture(
    cx: &mut TestAppContext,
    locked: bool,
) -> (Workspace, DocumentId, &mut gpui::VisualTestContext) {
    let mut pen = PenAnnotation::new_paths(
        id("pdf:toast-pen"),
        0,
        vec![vec![
            PdfPoint::new(10., 10.).unwrap(),
            PdfPoint::new(60., 60.).unwrap(),
        ]],
        PenAppearance::new("#ff0000", 2., 1.).unwrap(),
        false,
    )
    .unwrap();
    pen.locked = locked;
    let pen_id = pen.id.clone();
    open_with_selection(cx, vec![Annotation::Pen(pen)], &pen_id)
}

#[gpui::test]
fn deleting_a_markup_says_so_in_the_status_bar_and_shows_no_toast(cx: &mut TestAppContext) {
    use butter_paper::document_workspace::StatusSeverity;
    let (workspace, document_id, cx) = pen_fixture(cx, false);
    assert_eq!(toast_count(cx), 0);
    workspace
        .update(cx, |workspace, cx| {
            workspace.delete_selected_annotation(document_id, cx)
        })
        .unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.annotation_status()),
        Some("Deleted annotation".to_owned()),
        "the status bar says what happened"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.annotation_status_severity()),
        Some(StatusSeverity::Info)
    );
    assert_eq!(toast_count(cx), 0, "a routine confirmation is no toast");
}

#[gpui::test]
fn a_refused_action_shows_its_reason_as_a_toast(cx: &mut TestAppContext) {
    use butter_paper::document_workspace::StatusSeverity;
    let (workspace, document_id, cx) = pen_fixture(cx, true);
    assert_eq!(toast_count(cx), 0);
    let _ = workspace.update(cx, |workspace, cx| {
        workspace.delete_selected_annotation(document_id, cx)
    });
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.annotation_status()),
        Some("Annotation is locked".to_owned())
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.annotation_status_severity()),
        Some(StatusSeverity::Notice)
    );
    assert_eq!(toast_count(cx), 1, "the reason shows as a toast");
}

#[gpui::test]
fn a_failed_edit_shows_as_an_error_toast(cx: &mut TestAppContext) {
    use butter_paper::document_workspace::StatusSeverity;
    use butter_paper::engineering_visual_property_inspector::{
        EngineeringVisualPropertyEvent, EngineeringVisualPropertyKind,
        EngineeringVisualPropertyPatch,
    };
    let mut arc = ArcAnnotation::new(
        id("pdf:toast-arc"),
        0,
        PdfPoint::new(20., 20.).unwrap(),
        PdfPoint::new(100., 20.).unwrap(),
        PdfPoint::new(60., 60.).unwrap(),
        RectangleAppearance::new("#112233", 1., None::<String>, 1.).unwrap(),
    )
    .unwrap();
    arc.locked = true;
    let arc_id = arc.id.clone();
    let (workspace, document_id, cx) = open_with_selection(cx, vec![Annotation::Arc(arc)], &arc_id);
    assert_eq!(toast_count(cx), 0);
    let inspector = workspace
        .read_with(cx, |workspace, _| {
            workspace.engineering_visual_property_inspector()
        })
        .unwrap();
    let revision = snapshot(&workspace, document_id, cx).revision;
    inspector.update(cx, |_, cx| {
        cx.emit(EngineeringVisualPropertyEvent {
            document_id,
            annotation_id: arc_id.clone(),
            expected_revision: revision,
            expected_kind: EngineeringVisualPropertyKind::Arc,
            patch: EngineeringVisualPropertyPatch::WidthPt(3.),
        })
    });
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.annotation_status_severity()),
        Some(StatusSeverity::Error)
    );
    assert_eq!(toast_count(cx), 1, "a failure shows as a toast");
}
