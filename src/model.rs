use std::path::PathBuf;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Page {
    pub width_mm: f32,
    pub height_mm: f32,
}

impl Page {
    pub fn from_name(name: &str) -> Option<Self> {
        let (width_mm, height_mm) = match name.to_ascii_lowercase().as_str() {
            "a0" => (841., 1189.),
            "a1" => (594., 841.),
            "a2" => (420., 594.),
            "a3" => (297., 420.),
            "a4" => (210., 297.),
            "a5" => (148., 210.),
            "a6" => (105., 148.),
            "letter" => (215.9, 279.4),
            "legal" => (215.9, 355.6),
            "tabloid" => (279.4, 431.8),
            "9x13" => (90., 130.),
            "4x6" | "10x15" => (100., 150.),
            "5x7" | "13x18" => (130., 180.),
            "8x10" | "20.32x25.4" => (203.2, 254.),
            "20x25" => (200., 250.),
            "passport" => (90., 130.),
            _ => return None,
        };
        Some(Self { width_mm, height_mm })
    }

    pub fn pixels(self, dpi: u32) -> Result<(u32, u32)> {
        self.validate()?;
        let w = (self.width_mm as f64 * dpi as f64 / 25.4).round();
        let h = (self.height_mm as f64 * dpi as f64 / 25.4).round();
        if w < 1. || h < 1. || w * h > 120_000_000. {
            bail!("page would exceed the 120 megapixel page safety limit");
        }
        Ok((w as u32, h as u32))
    }

    pub fn validate(self) -> Result<()> {
        if !self.width_mm.is_finite() || !self.height_mm.is_finite()
            || self.width_mm <= 0. || self.height_mm <= 0.
            || self.width_mm > 2000. || self.height_mm > 2000.
        {
            bail!("invalid page size in centimeters (each dimension must be greater than 0 and at most 200 cm)");
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct RectMm {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Layout {
    Grid {
        rows: u32,
        columns: u32,
        margin_mm: f32,
        gap_mm: f32,
    },
    Adaptive {
        photos_per_page: u32,
        margin_mm: f32,
        gap_mm: f32,
    },
    Template {
        name: String,
        slots: Vec<RectMm>,
        auto_rotate: bool,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CropMode { Fill, Fit }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Crop {
    pub mode: CropMode,
    pub focal_x: f32,
    pub focal_y: f32,
    pub zoom: f32,
}

impl Default for Crop {
    fn default() -> Self {
        Self { mode: CropMode::Fill, focal_x: 0.5, focal_y: 0.5, zoom: 1. }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Photo {
    pub path: PathBuf,
    pub copies: u32,
    pub rotation: i32,
    pub flip_h: bool,
    pub flip_v: bool,
    pub crop: Crop,
    pub caption_override: Option<String>,
}

impl Default for Photo {
    fn default() -> Self {
        Self {
            path: PathBuf::new(), copies: 1, rotation: 0, flip_h: false,
            flip_v: false, crop: Crop::default(), caption_override: None,
        }
    }
}

impl Photo {
    pub fn from_path(path: PathBuf) -> Self { Self { path, ..Self::default() } }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptionType { None, Filename, DateTime, Comment, Custom }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Caption {
    pub kind: CaptionType,
    pub custom: String,
    pub font_family: String,
    pub font_file: Option<PathBuf>,
    pub size_pt: f32,
    pub text_color: String,
    pub backdrop_opacity: u8,
}

impl Default for Caption {
    fn default() -> Self {
        Self {
            kind: CaptionType::None,
            custom: "{filename} — {date}".into(),
            font_family: "Sans".into(), font_file: None,
            size_pt: 10., text_color: "#FFFFFF".into(), backdrop_opacity: 160,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputTarget { Pdf, Images, Gimp, Printer }

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat { Jpeg, Png, Tiff }

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictRule { Overwrite, Skip, Rename, Error }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Output {
    pub target: OutputTarget,
    pub path: PathBuf,
    pub image_format: ImageFormat,
    pub jpeg_quality: u8,
    /// PDF: embed image pixels at native dimensions using lossless compression.
    pub preserve_original_quality: bool,
    pub conflict: ConflictRule,
    pub open_folder: bool,
    pub printer_name: Option<String>,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            target: OutputTarget::Pdf,
            path: crate::paths::default_pdf_path(),
            image_format: ImageFormat::Png,
            jpeg_quality: 95,
            preserve_original_quality: false,
            conflict: ConflictRule::Overwrite,
            open_folder: false,
            printer_name: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PrintJob {
    pub page: Page,
    pub layout: Layout,
    pub photos: Vec<Photo>,
    pub dpi: u32,
    pub auto_rotate: bool,
    /// If true, every photo fits inside its frame without trimming any edges.
    /// Missing in older JSON jobs is treated as false for backward compatibility.
    pub disable_crop: bool,
    /// Expand digiKam template photo frames to eliminate outer borders and inner gutters.
    pub borderless_templates: bool,
    pub background: String,
    pub caption: Caption,
    pub output: Output,
}

impl Default for PrintJob {
    fn default() -> Self {
        Self {
            page: Page::from_name("a4").unwrap(),
            layout: Layout::Grid { rows: 2, columns: 2, margin_mm: 7., gap_mm: 3. },
            photos: Vec::new(), dpi: 300, auto_rotate: true, disable_crop: false, borderless_templates: false,
            background: "#FFFFFF".into(), caption: Caption::default(),
            output: Output::default(),
        }
    }
}

impl PrintJob {
    pub fn validate(&self) -> Result<()> {
        self.page.validate()?;
        if !(36..=1200).contains(&self.dpi) { bail!("DPI must be between 36 and 1200"); }
        self.page.pixels(self.dpi)?;
        if self.photos.is_empty() { bail!("add at least one photo"); }
        if self.photos.iter().any(|p| p.copies == 0 || p.copies > 10000) {
            bail!("copies must be between 1 and 10000");
        }
        if self.photos.iter().try_fold(0u32, |n, p| n.checked_add(p.copies)).unwrap_or(u32::MAX) > 10000 {
            bail!("maximum total image copies per job is 10000");
        }
        for p in &self.photos {
            if p.path.as_os_str().is_empty() || !p.path.is_file() { bail!("missing image: {}", p.path.display()); }
            if p.rotation.rem_euclid(90) != 0 { bail!("photo rotation must be divisible by 90"); }
            if !p.crop.zoom.is_finite() || !(1.0..=10.0).contains(&p.crop.zoom)
                || !p.crop.focal_x.is_finite() || !p.crop.focal_y.is_finite()
                || !(0.0..=1.0).contains(&p.crop.focal_x) || !(0.0..=1.0).contains(&p.crop.focal_y)
            { bail!("invalid crop settings for {}", p.path.display()); }
        }
        if self.capacity()? == 0 { bail!("layout contains no image slots"); }
        if self.caption.size_pt <= 0. || self.caption.size_pt > 144. { bail!("caption point size must be in 0..144"); }
        if !(1..=100).contains(&self.output.jpeg_quality) { bail!("JPEG quality must be between 1 and 100"); }
        parse_color(&self.background)?;
        parse_color(&self.caption.text_color)?;
        Ok(())
    }

    /// digiKam-style global "Do not crop" overrides individual crop-to-fill settings.
    pub fn effective_crop_mode(&self, photo: &Photo) -> CropMode {
        if self.disable_crop { CropMode::Fit } else { photo.crop.mode }
    }

    pub fn capacity(&self) -> Result<usize> {
        match &self.layout {
            Layout::Grid { rows, columns, .. } => {
                let n = rows.checked_mul(*columns).unwrap_or(0);
                if !(1..=1000).contains(&n) { bail!("grid requires 1..=1000 slots"); }
                Ok(n as usize)
            },
            Layout::Adaptive { photos_per_page, .. } => {
                if !(1..=1000).contains(photos_per_page) { bail!("adaptive layout requires 1..=1000 images per page"); }
                Ok(*photos_per_page as usize)
            },
            Layout::Template { slots, .. } => {
                if slots.is_empty() || slots.len() > 1000 { bail!("template requires 1..=1000 frames"); }
                Ok(slots.len())
            },
        }
    }

    pub fn expanded_photo_indices(&self) -> Vec<usize> {
        self.photos.iter().enumerate().flat_map(|(i, p)| std::iter::repeat(i).take(p.copies as usize)).collect()
    }

    pub fn page_count(&self) -> usize {
        let c = self.capacity().unwrap_or(0);
        if c == 0 { 0 } else { (self.photos.iter().map(|p| p.copies as usize).sum::<usize>() + c - 1) / c }
    }
}

pub fn parse_color(s: &str) -> Result<[u8; 3]> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    if hex.len() != 6 { bail!("expected RRGGBB hex color, got {s}"); }
    let value = u32::from_str_radix(hex, 16)?;
    Ok([(value >> 16) as u8, (value >> 8) as u8, value as u8])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_dimensions() {
        let (w, h) = Page::from_name("a4").unwrap().pixels(300).unwrap();
        assert_eq!((w, h), (2480, 3508));
    }
    #[test]
    fn disable_crop_overrides_individual_settings() {
        let mut job = PrintJob::default();
        let photo = Photo::default();
        assert_eq!(job.effective_crop_mode(&photo), CropMode::Fill);
        job.disable_crop = true;
        assert_eq!(job.effective_crop_mode(&photo), CropMode::Fit);
    }
    #[test]
    fn old_jobs_default_to_cropping() {
        let mut value = serde_json::to_value(PrintJob::default()).unwrap();
        value.as_object_mut().unwrap().remove("disable_crop");
        let job: PrintJob = serde_json::from_value(value).unwrap();
        assert!(!job.disable_crop);
    }
    #[test]
    fn color_parse() { assert_eq!(parse_color("#00AACC").unwrap(), [0, 170, 204]); }
}
