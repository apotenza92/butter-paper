//! GPUI-free generation and temporary ownership for native template documents.

use std::{
    fmt::{self, Write as _},
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
};

pub const MIN_DIMENSION_MM: f64 = 10.;
pub const MAX_DIMENSION_MM: f64 = 5_000.;
pub const MIN_PATTERN_SPACING_MM: f64 = 1.;
pub const MAX_PATTERN_SPACING_MM: f64 = 500.;
pub const MAX_PATTERN_ELEMENTS: usize = 50_000;
const STORE_SENTINEL: &str = ".butter-paper-generated-document-store-v1";
const GENERATED_FILE_NAME: &str = "Untitled.pdf";

#[derive(Clone, Debug, PartialEq)]
pub enum GeneratedPattern {
    SquareGrid { spacing_mm: f64, color: String },
    Dots { spacing_mm: f64, color: String },
    Ruled { spacing_mm: f64, color: String },
    Isometric { spacing_mm: f64, color: String },
    Triangle { spacing_mm: f64, color: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedDocumentRequest {
    pub title: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub pattern: Option<GeneratedPattern>,
}

impl GeneratedDocumentRequest {
    pub fn a3_landscape_blank() -> Self {
        Self {
            title: "Untitled".into(),
            width_mm: 420.,
            height_mm: 297.,
            pattern: None,
        }
    }

    pub fn to_pdf_bytes(&self) -> Result<Vec<u8>, GeneratedDocumentError> {
        validate_dimension(self.width_mm, "width_mm")?;
        validate_dimension(self.height_mm, "height_mm")?;
        if self.title.is_empty() {
            return Err(GeneratedDocumentError("title must not be empty".into()));
        }

        let width = millimetres_to_pdf_points(self.width_mm);
        let height = millimetres_to_pdf_points(self.height_mm);
        let (content, subject) = match &self.pattern {
            None => (String::new(), None),
            Some(pattern) => {
                let (spacing_mm, color) = pattern.spacing_and_color();
                validate_spacing(*spacing_mm)?;
                let color = parse_hex_color(color)?;
                let columns = interior_interval_count(self.width_mm, *spacing_mm);
                let rows = interior_interval_count(self.height_mm, *spacing_mm);
                let element_count =
                    pattern.element_count(self.width_mm, self.height_mm, columns, rows);
                if element_count > MAX_PATTERN_ELEMENTS {
                    return Err(GeneratedDocumentError(format!(
                        "the selected pattern exceeds {MAX_PATTERN_ELEMENTS} elements"
                    )));
                }
                let spacing = millimetres_to_pdf_points(*spacing_mm);
                let mut content = String::from("/Artifact BMC\n");
                pattern.write_artwork(&mut content, width, height, spacing, columns, rows, color);
                content.push_str("EMC\n");
                let subject = format!(
                    "butter-paper:page-grid:{{\"version\":1,\"type\":\"{}\",\"origin\":{{\"x\":0,\"y\":0}},\"spacing\":{spacing},\"width\":{width},\"height\":{height},\"rotationDegrees\":0,\"source\":\"generated\"}}",
                    pattern.subject_type(),
                );
                (content, Some(subject))
            }
        };
        Ok(pdf_bytes(
            &self.title,
            width,
            height,
            &content,
            subject.as_deref(),
        ))
    }
}

impl GeneratedPattern {
    fn spacing_and_color(&self) -> (&f64, &str) {
        match self {
            Self::SquareGrid { spacing_mm, color }
            | Self::Dots { spacing_mm, color }
            | Self::Ruled { spacing_mm, color }
            | Self::Isometric { spacing_mm, color }
            | Self::Triangle { spacing_mm, color } => (spacing_mm, color),
        }
    }

    fn subject_type(&self) -> &'static str {
        match self {
            Self::SquareGrid { .. } | Self::Dots { .. } => "rectangular",
            Self::Ruled { .. } => "ruled",
            Self::Isometric { .. } => "isometric",
            Self::Triangle { .. } => "triangle",
        }
    }

    fn line_angles(&self) -> Option<[f64; 3]> {
        match self {
            Self::Isometric { .. } => Some([
                std::f64::consts::PI / 6.,
                std::f64::consts::PI / 2.,
                5. * std::f64::consts::PI / 6.,
            ]),
            Self::Triangle { .. } => Some([
                0.,
                std::f64::consts::PI / 3.,
                2. * std::f64::consts::PI / 3.,
            ]),
            _ => None,
        }
    }

    fn element_count(&self, width_mm: f64, height_mm: f64, columns: usize, rows: usize) -> usize {
        match self {
            Self::SquareGrid { .. } => columns.saturating_add(rows),
            Self::Dots { .. } => columns.saturating_mul(rows),
            Self::Ruled { .. } => rows,
            Self::Isometric { spacing_mm, .. } | Self::Triangle { spacing_mm, .. } => self
                .line_angles()
                .unwrap()
                .into_iter()
                .map(|angle| line_family_indexes(width_mm, height_mm, *spacing_mm, angle).len())
                .sum(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn write_artwork(
        &self,
        content: &mut String,
        width: f64,
        height: f64,
        spacing: f64,
        columns: usize,
        rows: usize,
        color: (f64, f64, f64),
    ) {
        if matches!(self, Self::Dots { .. }) {
            let _ = writeln!(content, "{:.6} {:.6} {:.6} rg", color.0, color.1, color.2);
            for column in 1..=columns {
                for row in 1..=rows {
                    write_circle(content, column as f64 * spacing, row as f64 * spacing, 0.75);
                }
            }
            return;
        }

        let _ = writeln!(content, "{:.6} {:.6} {:.6} RG", color.0, color.1, color.2);
        content.push_str("0.25 w\n");
        match self {
            Self::SquareGrid { .. } => {
                for column in 1..=columns {
                    write_line(
                        content,
                        column as f64 * spacing,
                        0.,
                        column as f64 * spacing,
                        height,
                    );
                }
                for row in 1..=rows {
                    write_line(
                        content,
                        0.,
                        row as f64 * spacing,
                        width,
                        row as f64 * spacing,
                    );
                }
            }
            Self::Ruled { .. } => {
                for row in 1..=rows {
                    write_line(
                        content,
                        0.,
                        row as f64 * spacing,
                        width,
                        row as f64 * spacing,
                    );
                }
            }
            Self::Isometric { .. } | Self::Triangle { .. } => {
                for angle in self.line_angles().unwrap() {
                    for index in line_family_indexes(width, height, spacing, angle) {
                        if let Some((start, end)) =
                            line_segment_for_offset(width, height, angle, index as f64 * spacing)
                        {
                            write_line(content, start.0, start.1, end.0, end.1);
                        }
                    }
                }
            }
            Self::Dots { .. } => unreachable!(),
        }
    }
}

fn write_line(content: &mut String, x1: f64, y1: f64, x2: f64, y2: f64) {
    let _ = writeln!(content, "{x1:.4} {y1:.4} m");
    let _ = writeln!(content, "{x2:.4} {y2:.4} l");
    content.push_str("S\n");
}

fn write_circle(content: &mut String, x: f64, y: f64, radius: f64) {
    let control = radius * 0.552_284_749_830_793_6;
    let _ = writeln!(content, "{:.4} {:.4} m", x + radius, y);
    let _ = writeln!(
        content,
        "{:.4} {:.4} {:.4} {:.4} {:.4} {:.4} c",
        x + radius,
        y + control,
        x + control,
        y + radius,
        x,
        y + radius
    );
    let _ = writeln!(
        content,
        "{:.4} {:.4} {:.4} {:.4} {:.4} {:.4} c",
        x - control,
        y + radius,
        x - radius,
        y + control,
        x - radius,
        y
    );
    let _ = writeln!(
        content,
        "{:.4} {:.4} {:.4} {:.4} {:.4} {:.4} c",
        x - radius,
        y - control,
        x - control,
        y - radius,
        x,
        y - radius
    );
    let _ = writeln!(
        content,
        "{:.4} {:.4} {:.4} {:.4} {:.4} {:.4} c",
        x + control,
        y - radius,
        x + radius,
        y - control,
        x + radius,
        y
    );
    content.push_str("f\n");
}

fn line_family_indexes(width: f64, height: f64, spacing: f64, angle: f64) -> Vec<i64> {
    let normal = (-angle.sin(), angle.cos());
    let projections = [
        0.,
        normal.0 * width,
        normal.1 * height,
        normal.0 * width + normal.1 * height,
    ];
    let first = (projections.iter().copied().fold(f64::INFINITY, f64::min) / spacing).ceil() as i64;
    let last = (projections
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max)
        / spacing)
        .floor() as i64;
    (first..=last).collect()
}

fn line_segment_for_offset(
    width: f64,
    height: f64,
    angle: f64,
    offset: f64,
) -> Option<((f64, f64), (f64, f64))> {
    let direction = (angle.cos(), angle.sin());
    let normal = (-direction.1, direction.0);
    let point = (normal.0 * offset, normal.1 * offset);
    let mut candidates = Vec::with_capacity(4);
    const EPSILON: f64 = 1e-7;
    let mut append = |x: f64, y: f64| {
        if x < -EPSILON || x > width + EPSILON || y < -EPSILON || y > height + EPSILON {
            return;
        }
        let clamped = (x.clamp(0., width), y.clamp(0., height));
        if candidates.iter().any(|candidate: &(f64, f64, f64)| {
            (candidate.0 - clamped.0).hypot(candidate.1 - clamped.1) < EPSILON
        }) {
            return;
        }
        candidates.push((
            clamped.0,
            clamped.1,
            clamped.0 * direction.0 + clamped.1 * direction.1,
        ));
    };
    if direction.0.abs() > EPSILON {
        for x in [0., width] {
            let distance = (x - point.0) / direction.0;
            append(x, point.1 + distance * direction.1);
        }
    }
    if direction.1.abs() > EPSILON {
        for y in [0., height] {
            let distance = (y - point.1) / direction.1;
            append(point.0 + distance * direction.0, y);
        }
    }
    if candidates.len() < 2 {
        return None;
    }
    candidates.sort_by(|left, right| left.2.total_cmp(&right.2));
    let first = candidates.first().unwrap();
    let last = candidates.last().unwrap();
    Some(((first.0, first.1), (last.0, last.1)))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedGeneratedDocument {
    path: PathBuf,
    directory: PathBuf,
    store_root: PathBuf,
}

impl OwnedGeneratedDocument {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Clone, Debug)]
pub struct GeneratedDocumentStore {
    root: PathBuf,
}

impl GeneratedDocumentStore {
    pub fn new(root: PathBuf) -> Result<Self, GeneratedDocumentError> {
        if fs::symlink_metadata(&root).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(GeneratedDocumentError(
                "generated document store root must not be a symlink".into(),
            ));
        }
        fs::create_dir_all(&root)?;
        let sentinel = root.join(STORE_SENTINEL);
        if sentinel.exists() {
            if fs::read_to_string(&sentinel)? != "butter-paper-generated-document-store-v1\n" {
                return Err(GeneratedDocumentError(
                    "generated document store ownership sentinel is invalid".into(),
                ));
            }
        } else {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&sentinel)?;
            file.write_all(b"butter-paper-generated-document-store-v1\n")?;
            file.sync_all()?;
        }
        Ok(Self { root })
    }

    pub fn create(
        &self,
        document_key: &str,
        request: &GeneratedDocumentRequest,
    ) -> Result<OwnedGeneratedDocument, GeneratedDocumentError> {
        self.create_from_pdf_bytes(document_key, &request.to_pdf_bytes()?)
    }

    pub fn create_from_pdf_bytes(
        &self,
        document_key: &str,
        bytes: &[u8],
    ) -> Result<OwnedGeneratedDocument, GeneratedDocumentError> {
        validate_document_key(document_key)?;
        let directory = self.root.join(document_key);
        fs::create_dir(&directory)?;
        let path = directory.join(GENERATED_FILE_NAME);
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            Ok(OwnedGeneratedDocument {
                path,
                directory: directory.clone(),
                store_root: self.root.clone(),
            })
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&directory);
        }
        result
    }

    pub fn release(&self, source: &OwnedGeneratedDocument) -> Result<(), GeneratedDocumentError> {
        if source.store_root != self.root
            || source.directory.parent() != Some(self.root.as_path())
            || source.path != source.directory.join(GENERATED_FILE_NAME)
        {
            return Err(GeneratedDocumentError(
                "generated document source is not owned by this store".into(),
            ));
        }
        fs::remove_dir_all(&source.directory)?;
        Ok(())
    }

    pub fn release_path(&self, path: &Path) -> Result<(), GeneratedDocumentError> {
        let Some(directory) = path.parent() else {
            return Err(GeneratedDocumentError(
                "generated document path has no owned directory".into(),
            ));
        };
        if directory.parent() != Some(self.root.as_path())
            || path.file_name().and_then(|name| name.to_str()) != Some(GENERATED_FILE_NAME)
        {
            return Err(GeneratedDocumentError(
                "generated document path is outside the owned store".into(),
            ));
        }
        fs::remove_dir_all(directory)?;
        Ok(())
    }

    pub fn remove_if_empty(self) -> Result<(), GeneratedDocumentError> {
        let sentinel = self.root.join(STORE_SENTINEL);
        let mut entries = fs::read_dir(&self.root)?;
        if entries.any(|entry| entry.is_ok_and(|entry| entry.path() != sentinel)) {
            return Err(GeneratedDocumentError(
                "generated document store still owns live documents".into(),
            ));
        }
        fs::remove_file(sentinel)?;
        fs::remove_dir(self.root)?;
        Ok(())
    }
}

fn validate_document_key(document_key: &str) -> Result<(), GeneratedDocumentError> {
    if document_key.is_empty()
        || !document_key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(GeneratedDocumentError(
            "generated document key contains unsupported characters".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedDocumentError(String);

impl fmt::Display for GeneratedDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for GeneratedDocumentError {}

impl From<std::io::Error> for GeneratedDocumentError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

pub fn millimetres_to_pdf_points(millimetres: f64) -> f64 {
    millimetres * 72. / 25.4
}

fn validate_dimension(value: f64, name: &str) -> Result<(), GeneratedDocumentError> {
    if !value.is_finite() || !(MIN_DIMENSION_MM..=MAX_DIMENSION_MM).contains(&value) {
        return Err(GeneratedDocumentError(format!(
            "{name} must be between 10 and 5000 millimetres"
        )));
    }
    Ok(())
}

fn validate_spacing(value: f64) -> Result<(), GeneratedDocumentError> {
    if !value.is_finite() || !(MIN_PATTERN_SPACING_MM..=MAX_PATTERN_SPACING_MM).contains(&value) {
        return Err(GeneratedDocumentError(
            "pattern spacing must be between 1 and 500 millimetres".into(),
        ));
    }
    Ok(())
}

fn parse_hex_color(value: &str) -> Result<(f64, f64, f64), GeneratedDocumentError> {
    if value.len() != 7
        || !value.starts_with('#')
        || !value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(GeneratedDocumentError(
            "pattern color must be a six-digit hexadecimal colour".into(),
        ));
    }
    let channel = |range| u8::from_str_radix(&value[range], 16).map(f64::from);
    Ok((
        channel(1..3).map_err(|error| GeneratedDocumentError(error.to_string()))? / 255.,
        channel(3..5).map_err(|error| GeneratedDocumentError(error.to_string()))? / 255.,
        channel(5..7).map_err(|error| GeneratedDocumentError(error.to_string()))? / 255.,
    ))
}

fn interior_interval_count(length_mm: f64, spacing_mm: f64) -> usize {
    (length_mm / spacing_mm).ceil().max(1.) as usize - 1
}

fn pdf_bytes(
    title: &str,
    width: f64,
    height: f64,
    content: &str,
    subject: Option<&str>,
) -> Vec<u8> {
    let title = escape_pdf_string(title);
    let subject = subject
        .map(|subject| format!(" /Subject ({})", escape_pdf_string(subject)))
        .unwrap_or_default();
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width:.4} {height:.4}] /Resources <<>> /Contents 4 0 R >>"
        ),
        format!(
            "<< /Length {} >>\nstream\n{}endstream",
            content.len(),
            content
        ),
        format!("<< /Title ({title}) /Creator (Butter Paper) /Producer (Butter Paper){subject} >>"),
    ];
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        let _ = write!(pdf, "{} 0 obj\n{}\nendobj\n", index + 1, object);
    }
    let xref_offset = pdf.len();
    let _ = write!(pdf, "xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        let _ = writeln!(pdf, "{offset:010} 00000 n ");
    }
    let _ = write!(
        pdf,
        "trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
        objects.len() + 1
    );
    pdf.into_bytes()
}

fn escape_pdf_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}
