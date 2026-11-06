//! Lossless photographic PDF export: source samples are placed as PDF Image XObjects.
//! Photos are never scaled or JPEG-recompressed into a page bitmap. All crop/fit
//! and layout operations are PDF clipping and affine transforms. JPEG source
//! compression isn't retained: its decoded pixels are embedded with lossless Flate.
//! ICC soft-proofing/printer driver conversion is outside this module's scope.
use std::{fmt::Write as FmtWrite, io::{Seek, SeekFrom, Write}};
use anyhow::{bail, Context, Result};
use flate2::{write::ZlibEncoder, Compression};
use image::{DynamicImage, GenericImageView};
use crate::{layout, metadata, model::{self, CaptionType, CropMode, PrintJob, RectMm}, render};

fn mm_to_pt(mm: f32) -> f64 { mm as f64 * 72.0 / 25.4 }
fn mm_to_px(mm: f32, dpi: u32) -> u32 { (mm as f64 * dpi as f64 / 25.4).round().max(1.0) as u32 }

pub struct NativePdfWriter<W: Write + Seek> {
    output: W,
    offsets: Vec<u64>,
    page_ids: Vec<usize>,
    expected_pages: usize,
    width_pt: f64,
    height_pt: f64,
}

impl<W: Write + Seek> NativePdfWriter<W> {
    pub fn new(mut output: W, pages: usize, width_mm: f32, height_mm: f32) -> Result<Self> {
        if pages == 0 { bail!("empty PDF"); }
        output.write_all(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n")?;
        Ok(Self { output, offsets: vec![0; 3], page_ids: Vec::new(),
            expected_pages: pages, width_pt: mm_to_pt(width_mm), height_pt: mm_to_pt(height_mm) })
    }

    fn reserve(&mut self) -> usize {
        let id = self.offsets.len();
        self.offsets.push(0);
        id
    }

    fn start(&mut self, id: usize) -> Result<()> {
        self.offsets[id] = self.output.stream_position()?;
        writeln!(self.output, "{id} 0 obj")?;
        Ok(())
    }

    fn stream(&mut self, id: usize, dict: &str, bytes: &[u8]) -> Result<()> {
        self.start(id)?;
        writeln!(self.output, "<< {dict} /Length {} >>\nstream", bytes.len())?;
        self.output.write_all(bytes)?;
        self.output.write_all(b"\nendstream\nendobj\n")?;
        Ok(())
    }

    fn compress(data: &[u8]) -> Result<Vec<u8>> {
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(data)?;
        Ok(enc.finish()?)
    }

    /// Store the exact decoded RGB/RGBA samples. Preserve 16-bit depth for
    /// 16-bit PNG/TIFF sources; alpha is carried in an independent /SMask.
    fn add_image(&mut self, source: DynamicImage) -> Result<usize> {
        let (width, height) = source.dimensions();
        let is_16 = matches!(&source, DynamicImage::ImageLuma16(_) | DynamicImage::ImageLumaA16(_)
            | DynamicImage::ImageRgb16(_) | DynamicImage::ImageRgba16(_));
        let (rgb, mask, bits) = if is_16 {
            let rgba = source.to_rgba16();
            let mut rgb = Vec::with_capacity(width as usize*height as usize*6);
            let mut alpha = Vec::with_capacity(width as usize*height as usize*2);
            let mut transparent = false;
            for pixel in rgba.pixels() {
                for channel in &pixel.0[..3] { rgb.extend_from_slice(&channel.to_be_bytes()); }
                let a = pixel.0[3];
                alpha.extend_from_slice(&a.to_be_bytes());
                transparent |= a != u16::MAX;
            }
            (rgb, if transparent {Some(alpha)} else {None}, 16)
        } else {
            let rgba = source.to_rgba8();
            let mut rgb = Vec::with_capacity(width as usize*height as usize*3);
            let mut alpha = Vec::with_capacity(width as usize*height as usize);
            let mut transparent = false;
            for pixel in rgba.pixels() {
                rgb.extend_from_slice(&pixel.0[..3]);
                let a = pixel.0[3];
                alpha.push(a);
                transparent |= a != u8::MAX;
            }
            (rgb, if transparent {Some(alpha)} else {None}, 8)
        };
        let mask_id = if let Some(a) = mask {
            let id = self.reserve();
            let data = Self::compress(&a)?;
            let dict = format!("/Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceGray /BitsPerComponent {bits} /Filter /FlateDecode");
            self.stream(id, &dict, &data)?;
            Some(id)
        } else { None };
        let id = self.reserve();
        let data = Self::compress(&rgb)?;
        let smask = mask_id.map(|mask| format!("/SMask {mask} 0 R")).unwrap_or_default();
        let dict = format!("/Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent {bits} /Filter /FlateDecode {smask}");
        self.stream(id, &dict, &data)?;
        Ok(id)
    }

    fn draw_image(content: &mut String, name: &str, frame: RectMm, width: u32, height: u32,
                  mode: CropMode, zoom: f32, focus_x: f32, focus_y: f32, page_h: f64) -> Result<()> {
        if width == 0 || height == 0 { bail!("empty image"); }
        let (fw, fh) = (frame.width as f64, frame.height as f64);
        let (iw, ih) = (width as f64, height as f64);
        let scale = match mode {
            CropMode::Fit => (fw / iw).min(fh / ih),
            CropMode::Fill => (fw / iw).max(fh / ih) * zoom as f64,
        };
        let (dw, dh) = (iw * scale, ih * scale);
        let (x, y) = match mode {
            CropMode::Fit => (frame.x as f64 + (fw-dw)/2.0, frame.y as f64 + (fh-dh)/2.0),
            CropMode::Fill => (frame.x as f64 - (dw-fw)*focus_x as f64,
                               frame.y as f64 - (dh-fh)*focus_y as f64),
        };
        let clip_x = mm_to_pt(frame.x);
        let clip_y = page_h - mm_to_pt(frame.y + frame.height);
        let x = mm_to_pt(x as f32);
        let y = page_h - mm_to_pt((y+dh) as f32);
        writeln!(content, "q\n{clip_x:.8} {clip_y:.8} {:.8} {:.8} re W n\n{:.8} 0 0 {:.8} {x:.8} {y:.8} cm\n/{name} Do\nQ",
            mm_to_pt(frame.width), mm_to_pt(frame.height), mm_to_pt(dw as f32), mm_to_pt(dh as f32))?;
        Ok(())
    }

    pub fn add_page(&mut self, job: &PrintJob, page_index: usize) -> Result<()> {
        if self.page_ids.len() >= self.expected_pages { bail!("too many PDF pages"); }
        let indices = job.expanded_photo_indices();
        let capacity = job.capacity()?;
        let start = page_index.checked_mul(capacity).context("page index overflow")?;
        if start >= indices.len() { bail!("page index out of range"); }
        let selected = &indices[start..(start+capacity).min(indices.len())];
        let photos = selected.iter().map(|&i| &job.photos[i]).collect::<Vec<_>>();
        let frames = layout::job_slots(job, &photos)?;
        let color = model::parse_color(&job.background)?;
        let mut content = format!("q\n{:.8} {:.8} {:.8} rg\n0 0 {:.8} {:.8} re f\nQ\n",
            color[0] as f64/255., color[1] as f64/255., color[2] as f64/255., self.width_pt, self.height_pt);
        let mut xobjects = String::new();
        for (i, (&photo_id, frame)) in selected.iter().zip(frames.iter()).enumerate() {
            let photo = &job.photos[photo_id];
            let md = metadata::read(&photo.path);
            let raw = metadata::open_photo(&photo.path)
                .with_context(|| format!("opening {}", photo.path.display()))?;
            let original_size = raw.dimensions();
            let mut oriented = metadata::orient(raw, md.orientation);
            oriented = match photo.rotation.rem_euclid(360) {
                90 => oriented.rotate90(), 180 => oriented.rotate180(),
                270 => oriented.rotate270(), _ => oriented,
            };
            if photo.flip_h { oriented = oriented.fliph(); }
            if photo.flip_v { oriented = oriented.flipv(); }
            let allow_autorotate = job.auto_rotate && match &job.layout {
                model::Layout::Template { auto_rotate, .. } => *auto_rotate,
                _ => true,
            };
            if allow_autorotate {
                let ratio = oriented.width() as f64 / oriented.height().max(1) as f64;
                let target = frame.width as f64 / frame.height as f64;
                let score = (ratio/target).min(target/ratio);
                let rotated_score = ((1./ratio)/target).min(target/(1./ratio));
                if rotated_score > score+0.05 { oriented = oriented.rotate90(); }
            }
            let (iw, ih) = oriented.dimensions();
            let id = self.add_image(oriented)?;
            let label = format!("Im{i}");
            write!(&mut xobjects, "/{label} {id} 0 R ")?;
            Self::draw_image(&mut content, &label, *frame, iw, ih,
                job.effective_crop_mode(photo), photo.crop.zoom, photo.crop.focal_x, photo.crop.focal_y, self.height_pt)?;

            // Caption is its own transparent PDF XObject. The underlying image
            // retains native sample dimensions even when text is enabled.
            if photo.caption_override.is_some() || job.caption.kind != CaptionType::None {
                let caption = metadata::display_caption(photo, job, start+i, original_size, &md);
                if !caption.trim().is_empty() {
                    let cw = mm_to_px(frame.width,job.dpi);
                    let ch = mm_to_px(frame.height,job.dpi);
                    let overlay = render::caption_overlay(job,&caption,cw,ch)?;
                    let id = self.add_image(DynamicImage::ImageRgba8(overlay))?;
                    let name = format!("Cap{i}");
                    write!(&mut xobjects, "/{name} {id} 0 R ")?;
                    Self::draw_image(&mut content, &name, *frame, cw, ch, CropMode::Fit,
                        1.0, 0.5, 0.5, self.height_pt)?;
                }
            }
        }
        let content_id = self.reserve();
        self.stream(content_id, "", content.as_bytes())?;
        let page_id = self.reserve();
        self.start(page_id)?;
        writeln!(self.output, "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.8} {:.8}] /Resources << /XObject << {xobjects} >> >> /Contents {content_id} 0 R >>\nendobj",
            self.width_pt, self.height_pt)?;
        self.page_ids.push(page_id);
        Ok(())
    }

    pub fn finish(mut self) -> Result<W> {
        if self.page_ids.len() != self.expected_pages { bail!("incomplete PDF"); }
        self.start(2)?;
        write!(self.output, "<< /Type /Pages /Count {} /Kids [", self.expected_pages)?;
        for page in &self.page_ids { write!(self.output, " {page} 0 R")?; }
        self.output.write_all(b" ] >>\nendobj\n")?;
        self.start(1)?;
        self.output.write_all(b"<< /Type /Catalog /Pages 2 0 R >>\nendobj\n")?;
        let crossref = self.output.stream_position()?;
        writeln!(self.output, "xref\n0 {}", self.offsets.len())?;
        self.output.write_all(b"0000000000 65535 f \n")?;
        for &offset in self.offsets.iter().skip(1) {
            if offset > 9_999_999_999 { bail!("PDF exceeds classic xref limit"); }
            writeln!(self.output, "{offset:010} 00000 n ")?;
        }
        writeln!(self.output,"trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
            self.offsets.len(),crossref)?;
        self.output.flush()?;
        self.output.seek(SeekFrom::End(0))?;
        Ok(self.output)
    }
}
