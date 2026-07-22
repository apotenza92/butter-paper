use std::fs;
use std::io::{self, BufRead, Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use image::ImageFormat;
use pdfium_auto::bind_pdfium_silent;
use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug)]
#[command(name = "butter-paper-pdfium-render-core")]
#[command(about = "Minimal PDFium-backed CLI for desktop proof integration")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Return basic document metadata.
    DocumentInfo {
        #[arg(long)]
        file: PathBuf,
    },
    /// Return size and rotation information for a page.
    PageInfo {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        page_index: u16,
    },
    /// Render a page into a PNG and return metadata plus raw PNG bytes.
    RenderPage {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        page_index: u16,
        #[arg(long)]
        width: u32,
        #[arg(long)]
        height: u32,
        #[arg(long)]
        rotation: Option<u16>,
        #[arg(long, value_enum, default_value_t = RenderMode::Full)]
        render_mode: RenderMode,
        #[arg(long)]
        crop_x: Option<f32>,
        #[arg(long)]
        crop_y: Option<f32>,
        #[arg(long)]
        crop_width: Option<f32>,
        #[arg(long)]
        crop_height: Option<f32>,
    },
    /// Run a long-lived render-page worker over newline-delimited JSON stdin.
    RenderWorker,
}

#[derive(Serialize)]
struct DocumentInfoResponse {
    #[serde(rename = "pageCount")]
    page_count: usize,
}

#[derive(Serialize)]
struct PageInfoResponse {
    width: f32,
    height: f32,
    rotation: u16,
}

#[derive(Serialize)]
struct RenderPageResponse {
    width: u32,
    height: u32,
    #[serde(rename = "byteLength")]
    byte_length: usize,
    timings: RenderPageTimings,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Deserialize)]
struct WorkerRenderPageRequest {
    id: String,
    file: PathBuf,
    #[serde(rename = "pageIndex")]
    page_index: u16,
    width: u32,
    height: u32,
    rotation: Option<u16>,
    #[serde(default, rename = "renderMode")]
    render_mode: RenderMode,
    #[serde(default, rename = "cropPdfRect")]
    crop_pdf_rect: Option<PdfCropRect>,
}

#[derive(Serialize)]
struct WorkerRenderPageResponse {
    id: String,
    width: u32,
    height: u32,
    #[serde(rename = "byteLength")]
    byte_length: usize,
    timings: RenderPageTimings,
}

#[derive(Serialize)]
struct WorkerErrorResponse {
    id: Option<String>,
    error: String,
}

struct RenderedPagePng {
    width: u32,
    height: u32,
    png_bytes: Vec<u8>,
    timings: RenderPageTimings,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PdfCropRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Default, Serialize)]
struct RenderPageTimings {
    #[serde(rename = "resolvePdfPathMs")]
    resolve_pdf_path_ms: f64,
    #[serde(rename = "loadDocumentMs")]
    load_document_ms: f64,
    #[serde(rename = "getPageMs")]
    get_page_ms: f64,
    #[serde(rename = "buildRenderConfigMs")]
    build_render_config_ms: f64,
    #[serde(rename = "pdfiumRenderMs")]
    pdfium_render_ms: f64,
    #[serde(rename = "bitmapToImageMs")]
    bitmap_to_image_ms: f64,
    #[serde(rename = "pngEncodeMs")]
    png_encode_ms: f64,
    #[serde(rename = "nativeTotalMs")]
    native_total_ms: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
enum RenderMode {
    Full,
    Preview,
}

impl Default for RenderMode {
    fn default() -> Self {
        Self::Full
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let payload = ErrorResponse {
                error: format!("{error:#}"),
            };

            match serde_json::to_string(&payload) {
                Ok(json) => {
                    println!("{json}");
                }
                Err(_) => {
                    println!("{{\"error\":\"failed to serialize error\"}}");
                }
            }

            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::DocumentInfo { file } => {
            let pdfium = bind_pdfium()?;
            let document = open_document(&pdfium, &file)?;
            print_json(&DocumentInfoResponse {
                page_count: document.pages().len() as usize,
            })
        }
        Command::PageInfo { file, page_index } => {
            let pdfium = bind_pdfium()?;
            let document = open_document(&pdfium, &file)?;
            let page = document
                .pages()
                .get(page_index as PdfPageIndex)
                .with_context(|| format!("page index out of range: {page_index}"))?;

            print_json(&PageInfoResponse {
                width: page.width().value,
                height: page.height().value,
                rotation: page_rotation_to_degrees(page.rotation()?),
            })
        }
        Command::RenderPage {
            file,
            page_index,
            width,
            height,
            rotation,
            render_mode,
            crop_x,
            crop_y,
            crop_width,
            crop_height,
        } => {
            let pdfium = bind_pdfium()?;
            let crop_pdf_rect = parse_cli_crop_rect(crop_x, crop_y, crop_width, crop_height)?;
            let rendered = render_page_png(
                &pdfium,
                &file,
                page_index,
                width,
                height,
                rotation,
                render_mode,
                crop_pdf_rect,
            )?;

            print_json_with_binary(
                &RenderPageResponse {
                    width: rendered.width,
                    height: rendered.height,
                    byte_length: rendered.png_bytes.len(),
                    timings: rendered.timings,
                },
                &rendered.png_bytes,
            )
        }
        Command::RenderWorker => run_render_worker(),
    }
}

fn run_render_worker() -> Result<()> {
    let pdfium = bind_pdfium()?;
    let stdin = io::stdin();

    for line in stdin.lock().lines() {
        let line = line.context("failed to read worker request")?;
        if line.trim().is_empty() {
            continue;
        }

        let request = match serde_json::from_str::<WorkerRenderPageRequest>(&line) {
            Ok(request) => request,
            Err(error) => {
                print_json(&WorkerErrorResponse {
                    id: None,
                    error: format!("invalid worker request: {error}"),
                })?;
                continue;
            }
        };

        let request_id = request.id.clone();
        match render_page_png(
            &pdfium,
            &request.file,
            request.page_index,
            request.width,
            request.height,
            request.rotation,
            request.render_mode,
            request.crop_pdf_rect,
        ) {
            Ok(rendered) => {
                print_json_with_binary(
                    &WorkerRenderPageResponse {
                        id: request_id,
                        width: rendered.width,
                        height: rendered.height,
                        byte_length: rendered.png_bytes.len(),
                        timings: rendered.timings,
                    },
                    &rendered.png_bytes,
                )?;
            }
            Err(error) => {
                print_json(&WorkerErrorResponse {
                    id: Some(request_id),
                    error: format!("{error:#}"),
                })?;
            }
        }
    }

    Ok(())
}

fn render_page_png(
    pdfium: &Pdfium,
    file: &Path,
    page_index: u16,
    width: u32,
    height: u32,
    rotation: Option<u16>,
    render_mode: RenderMode,
    crop_pdf_rect: Option<PdfCropRect>,
) -> Result<RenderedPagePng> {
    let native_start = Instant::now();
    let mut timings = RenderPageTimings::default();

    if width == 0 || height == 0 {
        return Err(anyhow!("width and height must be greater than zero"));
    }

    let stage_start = Instant::now();
    let canonical_path = resolve_pdf_path(file)?;
    timings.resolve_pdf_path_ms = elapsed_ms(stage_start);

    let stage_start = Instant::now();
    let document = load_document_from_canonical_path(pdfium, &canonical_path)?;
    timings.load_document_ms = elapsed_ms(stage_start);

    let stage_start = Instant::now();
    let page = document
        .pages()
        .get(page_index as PdfPageIndex)
        .with_context(|| format!("page index out of range: {page_index}"))?;
    timings.get_page_ms = elapsed_ms(stage_start);

    let stage_start = Instant::now();
    let page_width = page.width().value.max(1.0);
    let page_height = page.height().value.max(1.0);
    let crop_pdf_rect = match crop_pdf_rect {
        Some(crop) => Some(normalize_crop_rect(crop, page_width, page_height)?),
        None => None,
    };
    let source_width = crop_pdf_rect.map(|crop| crop.width).unwrap_or(page_width);
    let source_height = crop_pdf_rect.map(|crop| crop.height).unwrap_or(page_height);
    let scale = (width as f32 / source_width).min(height as f32 / source_height);
    let output_width = (source_width * scale).round().max(1.0) as u32;
    let output_height = (source_height * scale).round().max(1.0) as u32;
    let mut render_config = if let Some(crop) = crop_pdf_rect {
        PdfRenderConfig::new()
            .set_fixed_size(output_width as i32, output_height as i32)
            .scale_page_by_factor(scale)
            .translate(PdfPoints::new(-crop.x), PdfPoints::new(-crop.y))?
    } else {
        PdfRenderConfig::new().scale_page_by_factor(scale)
    }
    .set_format(PdfBitmapFormat::BGRA);
    if matches!(render_mode, RenderMode::Preview) {
        render_config = render_config
            .use_print_quality(false)
            .set_image_smoothing(false)
            .render_annotations(false)
            .render_form_data(false);
    }

    if let Some(rotation) = rotation {
        render_config = render_config.rotate(rotation_from_degrees(rotation)?, false);
    }
    timings.build_render_config_ms = elapsed_ms(stage_start);

    let stage_start = Instant::now();
    let bitmap = page
        .render_with_config(&render_config)
        .context("failed to render page with PDFium")?;
    timings.pdfium_render_ms = elapsed_ms(stage_start);

    let stage_start = Instant::now();
    let image = bitmap.as_image();
    let actual_width = image.width();
    let actual_height = image.height();
    timings.bitmap_to_image_ms = elapsed_ms(stage_start);

    let mut png_bytes = Vec::new();
    let stage_start = Instant::now();
    image
        .write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
        .context("failed to encode rendered page as PNG")?;
    timings.png_encode_ms = elapsed_ms(stage_start);
    timings.native_total_ms = elapsed_ms(native_start);

    Ok(RenderedPagePng {
        width: actual_width,
        height: actual_height,
        png_bytes,
        timings,
    })
}

fn parse_cli_crop_rect(
    crop_x: Option<f32>,
    crop_y: Option<f32>,
    crop_width: Option<f32>,
    crop_height: Option<f32>,
) -> Result<Option<PdfCropRect>> {
    match (crop_x, crop_y, crop_width, crop_height) {
        (None, None, None, None) => Ok(None),
        (Some(x), Some(y), Some(width), Some(height)) => Ok(Some(PdfCropRect {
            x,
            y,
            width,
            height,
        })),
        _ => Err(anyhow!(
            "crop-x, crop-y, crop-width, and crop-height must be provided together"
        )),
    }
}

fn normalize_crop_rect(crop: PdfCropRect, page_width: f32, page_height: f32) -> Result<PdfCropRect> {
    if !crop.x.is_finite()
        || !crop.y.is_finite()
        || !crop.width.is_finite()
        || !crop.height.is_finite()
        || crop.width <= 0.0
        || crop.height <= 0.0
    {
        return Err(anyhow!("cropPdfRect must contain finite positive PDF-space bounds"));
    }

    let left = crop.x.clamp(0.0, page_width);
    let bottom = crop.y.clamp(0.0, page_height);
    let right = (crop.x + crop.width).clamp(left, page_width);
    let top = (crop.y + crop.height).clamp(bottom, page_height);
    let width = right - left;
    let height = top - bottom;
    if width <= 0.0 || height <= 0.0 {
        return Err(anyhow!("cropPdfRect does not intersect the page bounds"));
    }

    Ok(PdfCropRect {
        x: left,
        y: bottom,
        width,
        height,
    })
}

fn bind_pdfium() -> Result<Pdfium> {
    bind_pdfium_silent().context(
        "failed to bind PDFium; first run may need network access unless PDFIUM_LIB_PATH points to an existing library",
    )
}

fn open_document<'a>(pdfium: &'a Pdfium, path: &Path) -> Result<PdfDocument<'a>> {
    let canonical_path = resolve_pdf_path(path)?;
    load_document_from_canonical_path(pdfium, &canonical_path)
}

fn resolve_pdf_path(path: &Path) -> Result<PathBuf> {
    let canonical_path = fs::canonicalize(path)
        .with_context(|| format!("failed to resolve PDF path: {}", path.display()))?;

    if !canonical_path.is_file() {
        return Err(anyhow!(
            "PDF file does not exist: {}",
            canonical_path.display()
        ));
    }

    Ok(canonical_path)
}

fn load_document_from_canonical_path<'a>(
    pdfium: &'a Pdfium,
    canonical_path: &Path,
) -> Result<PdfDocument<'a>> {
    pdfium
        .load_pdf_from_file(canonical_path, None)
        .with_context(|| format!("failed to open PDF file: {}", canonical_path.display()))
}

fn elapsed_ms(start: Instant) -> f64 {
    (start.elapsed().as_secs_f64() * 1000.0 * 1000.0).round() / 1000.0
}

fn rotation_from_degrees(degrees: u16) -> Result<PdfPageRenderRotation> {
    match degrees {
        0 => Ok(PdfPageRenderRotation::None),
        90 => Ok(PdfPageRenderRotation::Degrees90),
        180 => Ok(PdfPageRenderRotation::Degrees180),
        270 => Ok(PdfPageRenderRotation::Degrees270),
        _ => Err(anyhow!(
            "rotation must be one of 0, 90, 180, or 270 degrees"
        )),
    }
}

fn page_rotation_to_degrees(rotation: PdfPageRenderRotation) -> u16 {
    match rotation {
        PdfPageRenderRotation::None => 0,
        PdfPageRenderRotation::Degrees90 => 90,
        PdfPageRenderRotation::Degrees180 => 180,
        PdfPageRenderRotation::Degrees270 => 270,
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let json = serde_json::to_string(value).context("failed to serialize JSON response")?;
    println!("{json}");
    Ok(())
}

fn print_json_with_binary<T: Serialize>(value: &T, bytes: &[u8]) -> Result<()> {
    let json = serde_json::to_string(value).context("failed to serialize JSON response")?;
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(json.as_bytes())
        .context("failed to write JSON response")?;
    stdout
        .write_all(b"\n")
        .context("failed to write JSON response separator")?;
    stdout
        .write_all(bytes)
        .context("failed to write binary response bytes")?;
    stdout.flush().context("failed to flush response")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_crop_rect_requires_all_fields() {
        assert!(parse_cli_crop_rect(None, None, None, None).unwrap().is_none());
        assert!(parse_cli_crop_rect(Some(1.0), None, Some(2.0), Some(3.0)).is_err());

        let crop = parse_cli_crop_rect(Some(1.0), Some(2.0), Some(3.0), Some(4.0))
            .unwrap()
            .unwrap();
        assert_eq!(crop.x, 1.0);
        assert_eq!(crop.y, 2.0);
        assert_eq!(crop.width, 3.0);
        assert_eq!(crop.height, 4.0);
    }

    #[test]
    fn normalize_crop_rect_clamps_to_page_bounds() {
        let crop = normalize_crop_rect(
            PdfCropRect {
                x: -10.0,
                y: 20.0,
                width: 80.0,
                height: 120.0,
            },
            100.0,
            100.0,
        )
        .unwrap();

        assert_eq!(crop.x, 0.0);
        assert_eq!(crop.y, 20.0);
        assert_eq!(crop.width, 70.0);
        assert_eq!(crop.height, 80.0);
    }

    #[test]
    fn normalize_crop_rect_rejects_invalid_or_empty_crops() {
        assert!(normalize_crop_rect(
            PdfCropRect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 10.0,
            },
            100.0,
            100.0,
        )
        .is_err());

        assert!(normalize_crop_rect(
            PdfCropRect {
                x: 150.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            100.0,
            100.0,
        )
        .is_err());
    }
}
