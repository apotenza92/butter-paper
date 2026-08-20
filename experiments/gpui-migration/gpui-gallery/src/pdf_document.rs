use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::PathBuf,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct PdfDocument {
    pub id: u64,
    pub path: PathBuf,
    pub title: String,
    pub page_count: usize,
    pub current_page: usize,
    pub page_width: f32,
    pub page_height: f32,
    cache_dir: PathBuf,
    viewport_pixel_width: usize,
    viewport_image_path: Option<PathBuf>,
}

pub struct RenderedViewport {
    pub page: usize,
    pub page_width: f32,
    pub page_height: f32,
    pub pixel_width: usize,
    pub image_path: PathBuf,
}

impl PdfDocument {
    pub fn open(id: u64, path: PathBuf) -> Result<Self, String> {
        if !path.is_file()
            || path
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
                != Some("pdf")
        {
            return Err("Choose a readable PDF file.".into());
        }

        let mut command = Command::new(poppler_tool("pdfinfo"));
        command.arg(&path);
        let output = run_command(command, "PDF metadata", Duration::from_secs(5))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        let metadata = String::from_utf8_lossy(&output.stdout);
        let page_count = parse_page_count(&metadata)
            .ok_or_else(|| "pdfinfo did not report a page count.".to_string())?;
        let (page_width, page_height) = parse_page_size(&metadata)
            .ok_or_else(|| "pdfinfo did not report the first page size.".to_string())?;

        let title = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Untitled PDF")
            .to_string();
        let file_metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        file_metadata.len().hash(&mut hasher);
        file_metadata.modified().ok().hash(&mut hasher);
        let cache_dir = cache_root().join(format!("{:016x}", hasher.finish()));
        fs::create_dir_all(&cache_dir).map_err(|error| error.to_string())?;

        let document = Self {
            id,
            path,
            title,
            page_count,
            current_page: 1,
            page_width,
            page_height,
            cache_dir,
            viewport_pixel_width: 1310,
            viewport_image_path: None,
        };
        Ok(document)
    }

    pub fn select_page(&mut self, page: usize) -> usize {
        let page = page.clamp(1, self.page_count);
        self.current_page = page;
        page
    }

    pub fn render_viewport(
        &self,
        page: usize,
        zoom_percent: f32,
    ) -> Result<RenderedViewport, String> {
        let page = page.clamp(1, self.page_count);
        let (page_width, page_height) = self.read_page_size(page)?;
        let pixel_width = viewport_pixel_width(page_width, zoom_percent);
        let image_path = self.render_page(page, RenderSize::Viewport(pixel_width))?;
        Ok(RenderedViewport {
            page,
            page_width,
            page_height,
            pixel_width,
            image_path,
        })
    }

    pub fn apply_rendered_viewport(&mut self, rendered: RenderedViewport) {
        self.current_page = rendered.page;
        self.page_width = rendered.page_width;
        self.page_height = rendered.page_height;
        self.viewport_pixel_width = rendered.pixel_width;
        self.viewport_image_path = Some(rendered.image_path);
    }

    pub fn viewport_image(&self) -> Option<PathBuf> {
        self.viewport_image_path
            .clone()
            .filter(|path| path.is_file())
    }

    pub fn thumbnail_image(&self, page: usize) -> Option<PathBuf> {
        let path = self.image_path(page.clamp(1, self.page_count), RenderSize::Thumbnail);
        path.is_file().then_some(path)
    }

    pub fn render_thumbnail(&self, page: usize) -> Result<PathBuf, String> {
        self.render_page(page.clamp(1, self.page_count), RenderSize::Thumbnail)
    }

    fn render_page(&self, page: usize, size: RenderSize) -> Result<PathBuf, String> {
        let image_path = self.image_path(page, size);
        if image_path.is_file() {
            return Ok(image_path);
        }
        let prefix = image_path.with_extension("");
        let mut command = Command::new(poppler_tool("pdftoppm"));
        command
            .args([
                "-f",
                &page.to_string(),
                "-l",
                &page.to_string(),
                "-singlefile",
                "-png",
                "-scale-to-x",
                &size.pixel_width().to_string(),
                "-scale-to-y",
                "-1",
            ])
            .arg(&self.path)
            .arg(&prefix);
        let output = match run_command(command, "PDF page rendering", Duration::from_secs(15)) {
            Ok(output) => output,
            Err(error) => {
                let _ = fs::remove_file(&image_path);
                return Err(error);
            }
        };
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(image_path)
    }

    fn image_path(&self, page: usize, size: RenderSize) -> PathBuf {
        self.cache_dir
            .join(format!("page-{page:04}-{}.png", size.label()))
    }

    fn read_page_size(&self, page: usize) -> Result<(f32, f32), String> {
        let mut command = Command::new(poppler_tool("pdfinfo"));
        command
            .args(["-f", &page.to_string(), "-l", &page.to_string()])
            .arg(&self.path);
        let output = run_command(command, "PDF page metadata", Duration::from_secs(5))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        parse_page_size(&String::from_utf8_lossy(&output.stdout))
            .ok_or_else(|| format!("pdfinfo did not report the size of page {page}."))
    }
}

fn poppler_tool(name: &str) -> PathBuf {
    if let Some(path) = std::env::var_os("BP_POPPLER_BIN_DIR")
        .map(PathBuf::from)
        .map(|directory| directory.join(name))
        .filter(|path| path.is_file())
    {
        return path;
    }

    if let Some(path) = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(PathBuf::from))
        .map(|macos| macos.join("../Resources/poppler/bin").join(name))
        .filter(|path| path.is_file())
    {
        return path;
    }

    for directory in ["/opt/homebrew/bin", "/usr/local/bin"] {
        let path = PathBuf::from(directory).join(name);
        if path.is_file() {
            return path;
        }
    }

    PathBuf::from(name)
}

fn cache_root() -> PathBuf {
    if let Some(path) = std::env::var_os("BP_GPUI_CACHE_DIR") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(PathBuf::from))
        .filter(|directory| directory.ends_with("Contents/MacOS"))
        .map(|macos| macos.join("../Resources/pdf-cache"))
    {
        return path;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/pdf-cache")
}

fn run_command(mut command: Command, operation: &str, timeout: Duration) -> Result<Output, String> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start {operation}: {error}"))?;
    let deadline = Instant::now() + timeout;

    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|error| format!("Could not finish {operation}: {error}"));
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{operation} exceeded the {} second safety limit.",
                    timeout.as_secs()
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Could not monitor {operation}: {error}"));
            }
        }
    }
}

fn parse_page_count(metadata: &str) -> Option<usize> {
    metadata.lines().find_map(|line| {
        line.strip_prefix("Pages:")
            .and_then(|value| value.trim().parse().ok())
    })
}

fn parse_page_size(metadata: &str) -> Option<(f32, f32)> {
    metadata.lines().find_map(|line| {
        let values = line
            .split_once("size:")?
            .1
            .split_whitespace()
            .collect::<Vec<_>>();
        if values.get(1).copied() != Some("x") {
            return None;
        }
        Some((values.first()?.parse().ok()?, values.get(2)?.parse().ok()?))
    })
}

fn viewport_pixel_width(page_width: f32, zoom_percent: f32) -> usize {
    ((page_width * zoom_percent / 100.0 * 2.0).ceil() as usize).clamp(256, 4096)
}

#[derive(Copy, Clone)]
enum RenderSize {
    Viewport(usize),
    Thumbnail,
}

impl RenderSize {
    fn label(self) -> String {
        match self {
            Self::Viewport(width) => format!("viewport-{width}"),
            Self::Thumbnail => "thumb".into(),
        }
    }

    fn pixel_width(self) -> usize {
        match self {
            Self::Viewport(width) => width,
            Self::Thumbnail => 228,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{process::Command, time::Duration};

    use super::{parse_page_count, parse_page_size, run_command, viewport_pixel_width};

    #[test]
    fn parses_poppler_page_count() {
        assert_eq!(
            parse_page_count("Title: Example\nPages:           935\nEncrypted: no"),
            Some(935)
        );
    }

    #[test]
    fn rejects_missing_or_invalid_page_count() {
        assert_eq!(parse_page_count("Title: Example\nPages: many"), None);
        assert_eq!(parse_page_count("Title: Example"), None);
    }

    #[test]
    fn parses_first_and_numbered_page_sizes() {
        assert_eq!(
            parse_page_size("Page size:       336 x 388.8 pts"),
            Some((336.0, 388.8))
        );
        assert_eq!(
            parse_page_size("Page    8 size:  576 x 666 pts"),
            Some((576.0, 666.0))
        );
    }

    #[test]
    fn computes_retina_viewport_width_with_limits() {
        assert_eq!(viewport_pixel_width(336.0, 194.0), 1304);
        assert_eq!(viewport_pixel_width(10.0, 10.0), 256);
        assert_eq!(viewport_pixel_width(2000.0, 400.0), 4096);
    }

    #[test]
    fn stops_a_renderer_that_exceeds_its_time_budget() {
        let command = Command::new("/bin/sleep");
        let mut command = command;
        command.arg("1");
        let error = run_command(command, "test renderer", Duration::from_millis(20)).unwrap_err();
        assert!(error.contains("safety limit"));
    }
}
