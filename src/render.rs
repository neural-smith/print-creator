#[cfg(not(target_arch="wasm32"))]
use std::{path::PathBuf, process::Command};
use anyhow::{bail, Context, Result};
#[cfg(not(target_arch="wasm32"))]
use ab_glyph::FontArc;
use image::{imageops::{self, FilterType}, DynamicImage, GenericImageView, Rgba, RgbaImage};
#[cfg(not(target_arch="wasm32"))]
use imageproc::drawing::{draw_text_mut, text_size};
use crate::{layout, metadata, model::{self, CropMode, PrintJob, RectMm}};

fn mm_to_px(mm: f32, dpi: u32) -> i64 { (mm as f64 * dpi as f64 / 25.4).round() as i64 }

fn pixel_rect(rect: RectMm, dpi: u32, page_w: u32, page_h: u32) -> Result<(u32,u32,u32,u32)> {
    let left = mm_to_px(rect.x, dpi);
    let top = mm_to_px(rect.y, dpi);
    let right = mm_to_px(rect.x+rect.width, dpi);
    let bottom = mm_to_px(rect.y+rect.height, dpi);
    if left < 0 || top < 0 || right > page_w as i64 + 1 || bottom > page_h as i64 + 1
        || right <= left || bottom <= top { bail!("invalid photo frame outside page"); }
    let x = left as u32;
    let y = top as u32;
    let w = (right.min(page_w as i64) - left) as u32;
    let h = (bottom.min(page_h as i64) - top) as u32;
    Ok((x,y,w,h))
}

pub fn render_page(job: &PrintJob, page_index: usize, dpi: u32) -> Result<RgbaImage> {
    if !(36..=1200).contains(&dpi) { bail!("invalid render DPI"); }
    let (w, h) = job.page.pixels(dpi)?;
    let bg = model::parse_color(&job.background)?;
    let mut page = RgbaImage::from_pixel(w, h, Rgba([bg[0],bg[1],bg[2],255]));
    let photo_ids = job.expanded_photo_indices();
    let capacity = job.capacity()?;
    if capacity == 0 { bail!("layout has no slots"); }
    let offset = page_index.checked_mul(capacity).context("invalid page index")?;
    if offset >= photo_ids.len() { bail!("page number is out of range"); }
    let chosen = &photo_ids[offset..(offset+capacity).min(photo_ids.len())];
    let photos = chosen.iter().map(|&i| &job.photos[i]).collect::<Vec<_>>();
    let rectangles = layout::job_slots(job, &photos)?;
    #[cfg(not(target_arch="wasm32"))]
    let font = if chosen.iter().any(|&i| job.photos[i].caption_override.is_some())
        || job.caption.kind != model::CaptionType::None
        { Some(load_caption_font(job)?) } else { None };
    for (slot, (&photo_id, rect)) in chosen.iter().zip(rectangles.iter()).enumerate() {
        let photo = &job.photos[photo_id];
        let (x,y,sw,sh) = pixel_rect(*rect, dpi, w, h)?;
        let md = metadata::read(&photo.path);
        let raw = metadata::open_photo(&photo.path)
            .with_context(|| format!("decoding {}", photo.path.display()))?;
        let size = raw.dimensions();
        let oriented = metadata::prepare_for_print(
            raw, md.orientation, photo, job, rect.width as f64, rect.height as f64,
        );
        let image = fit_or_crop(oriented, sw, sh, job.effective_crop_mode(photo),
            photo.crop.focal_x, photo.crop.focal_y, photo.crop.zoom)?;
        let iw = image.width();
        let ih = image.height();
        let ox = x + (sw-iw)/2;
        let oy = y + (sh-ih)/2;
        imageops::overlay(&mut page, &image, ox.into(), oy.into());
        #[cfg(not(target_arch="wasm32"))]
        if let Some(font) = &font {
            let text = metadata::display_caption(photo, job, offset + slot, size, &md);
            if !text.trim().is_empty() {
                render_caption(&mut page, (x,y,sw,sh), &text, font, job, dpi)?;
            }
        }
        #[cfg(target_arch="wasm32")]
        {
            let text = metadata::display_caption(photo, job, offset + slot, size, &md);
            if !text.trim().is_empty() {
                render_web_caption(&mut page, (x,y,sw,sh), &text, job, dpi, false)?;
            }
        }
    }
    Ok(page)
}

fn fit_or_crop(src: DynamicImage, target_w: u32, target_h: u32, mode: CropMode,
                focal_x: f32, focal_y: f32, zoom: f32) -> Result<RgbaImage> {
    let (w,h) = src.dimensions();
    if w == 0 || h == 0 || target_w == 0 || target_h == 0 { bail!("empty image"); }
    match mode {
        CropMode::Fit => {
            let scale = (target_w as f64 / w as f64).min(target_h as f64 / h as f64);
            let nw = ((w as f64 * scale).round() as u32).clamp(1,target_w);
            let nh = ((h as f64 * scale).round() as u32).clamp(1,target_h);
            Ok(src.resize_exact(nw,nh,FilterType::Lanczos3).to_rgba8())
        }
        CropMode::Fill => {
            let src_ratio = w as f64 / h as f64;
            let dst_ratio = target_w as f64 / target_h as f64;
            let (base_w,base_h) = if src_ratio > dst_ratio {
                (h as f64 * dst_ratio,h as f64)
            } else { (w as f64,w as f64 / dst_ratio) };
            let cw = ((base_w / zoom as f64).round() as u32).clamp(1,w);
            let ch = ((base_h / zoom as f64).round() as u32).clamp(1,h);
            let cx = ((w-cw) as f64 * focal_x.clamp(0.,1.) as f64).round() as u32;
            let cy = ((h-ch) as f64 * focal_y.clamp(0.,1.) as f64).round() as u32;
            Ok(src.crop_imm(cx.min(w-cw),cy.min(h-ch),cw,ch)
                .resize_exact(target_w,target_h,FilterType::Lanczos3).to_rgba8())
        }
    }
}

#[cfg(not(target_arch="wasm32"))]
fn load_caption_font(job: &PrintJob) -> Result<FontArc> {
    let mut candidates = Vec::<PathBuf>::new();
    if let Some(p) = &job.caption.font_file { candidates.push(p.clone()); }
    if let Ok(out) = Command::new("fc-match").args(["-f", "%{file}", &job.caption.font_family]).output() {
        if out.status.success() {
            let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !name.is_empty() { candidates.push(name.into()); }
        }
    }
    for name in [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
    ] { candidates.push(name.into()); }
    for p in candidates {
        if let Ok(bytes) = std::fs::read(p) {
            if let Ok(font) = FontArc::try_from_vec(bytes) { return Ok(font); }
        }
    }
    bail!("no TrueType/OpenType caption font found; set caption.font_file to a font on this system")
}

#[cfg(not(target_arch="wasm32"))]
fn render_caption(page: &mut RgbaImage, slot: (u32,u32,u32,u32), text: &str,
                  font: &FontArc, job: &PrintJob, dpi: u32) -> Result<()> {
    let (x,y,w,h) = slot;
    let rgb = model::parse_color(&job.caption.text_color)?;
    let font_px = (job.caption.size_pt * dpi as f32 / 72.).max(1.);
    let pad = (font_px * 0.45).ceil() as u32;
    if pad*2 >= w || pad*2 >= h { return Ok(()); }
    let mut lines = Vec::new();
    for para in text.lines() {
        let mut current = String::new();
        for word in para.split_whitespace() {
            let candidate = if current.is_empty() { word.to_owned() } else { format!("{current} {word}") };
            if text_size(font_px, font, &candidate).0 <= w.saturating_sub(2*pad) || current.is_empty() {
                current = candidate;
            } else {
                lines.push(current);
                current = word.to_owned();
            }
        }
        if !current.is_empty() { lines.push(current); }
        if lines.len() >= 3 { break; }
    }
    lines.truncate(3);
    if lines.is_empty() { return Ok(()); }
    let lh = (font_px * 1.35).ceil() as u32;
    let bar_h = (lh * lines.len() as u32 + pad*2).min(h);
    let bar_y = y+h-bar_h;
    let alpha = job.caption.backdrop_opacity as u16;
    for py in bar_y..(y+h).min(page.height()) {
        for px in x..(x+w).min(page.width()) {
            let p = page.get_pixel_mut(px,py);
            for c in 0..3 { p.0[c] = ((p.0[c] as u16 * (255-alpha))/255) as u8; }
        }
    }
    for (i, line) in lines.iter().enumerate() {
        let mut line = line.clone();
        let limit = w.saturating_sub(2 * pad);
        while text_size(font_px,font,&line).0 > limit && !line.is_empty() {
            line.pop();
        }
        let (tw,_) = text_size(font_px,font,&line);
        let tx = x + (w.saturating_sub(tw))/2;
        let ty = bar_y + pad + i as u32*lh;
        if ty + lh > y+h+2 { break; }
        draw_text_mut(page,Rgba([rgb[0],rgb[1],rgb[2],255]),tx as i32,ty as i32,font_px,font,&line);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crop() {
        let src = DynamicImage::ImageRgba8(RgbaImage::new(300,200));
        let im = fit_or_crop(src,100,100,CropMode::Fill,0.5,0.5,1.).unwrap();
        assert_eq!(im.dimensions(),(100,100));
    }
    #[test]
    fn fit_preserves_ratio() {
        let src = DynamicImage::ImageRgba8(RgbaImage::new(400,200));
        let im = fit_or_crop(src,100,100,CropMode::Fit,0.5,0.5,1.).unwrap();
        assert_eq!(im.dimensions(),(100,50));
    }
}

/// Create a transparent caption overlay for native-resolution PDF pages.
/// Only the overlay, never the photo, is rasterized at the requested DPI.
#[cfg(not(target_arch="wasm32"))]
pub(crate) fn caption_overlay(job: &PrintJob, text: &str, width: u32, height: u32) -> Result<RgbaImage> {
    let font = load_caption_font(job)?;
    let mut layer = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 0]));
    let font_px = (job.caption.size_pt * job.dpi as f32 / 72.).max(1.);
    let pad = (font_px * 0.45).ceil() as u32;
    if pad.saturating_mul(2) >= width || pad.saturating_mul(2) >= height { return Ok(layer); }
    let mut lines = Vec::<String>::new();
    for para in text.lines() {
        let mut current = String::new();
        for word in para.split_whitespace() {
            let candidate = if current.is_empty() { word.to_owned() } else { format!("{current} {word}") };
            if text_size(font_px, &font, &candidate).0 <= width.saturating_sub(2*pad) || current.is_empty() {
                current = candidate;
            } else {
                lines.push(current);
                current = word.to_owned();
            }
        }
        if !current.is_empty() { lines.push(current); }
        if lines.len() >= 3 { break; }
    }
    lines.truncate(3);
    if lines.is_empty() { return Ok(layer); }
    let lh = (font_px * 1.35).ceil() as u32;
    let bar_h = (lh * lines.len() as u32 + pad*2).min(height);
    let bar_y = height - bar_h;
    for y in bar_y..height {
        for x in 0..width {
            *layer.get_pixel_mut(x, y) = Rgba([0, 0, 0, job.caption.backdrop_opacity]);
        }
    }
    let rgb = model::parse_color(&job.caption.text_color)?;
    for (i, raw) in lines.iter().enumerate() {
        let mut line = raw.clone();
        let max_width = width.saturating_sub(2*pad);
        while text_size(font_px, &font, &line).0 > max_width && !line.is_empty() { line.pop(); }
        let text_width = text_size(font_px, &font, &line).0;
        let x = (width.saturating_sub(text_width)) / 2;
        let y = bar_y + pad + i as u32 * lh;
        if y.saturating_add(lh) > height.saturating_add(2) { break; }
        draw_text_mut(&mut layer, Rgba([rgb[0], rgb[1], rgb[2], 255]),
            x as i32, y as i32, font_px, &font, &line);
    }
    Ok(layer)
}

/// The WASM build cannot read system font files. Render lightweight 8x8 bitmap
/// captions instead; full system-font selection remains a desktop-only feature.
#[cfg(target_arch="wasm32")]
fn render_web_caption(page: &mut RgbaImage, slot: (u32,u32,u32,u32), text: &str,
                      job: &PrintJob, dpi: u32, overlay: bool) -> Result<()> {
    use font8x8::{UnicodeFonts, BASIC_FONTS};
    let (x,y,w,h)=slot;
    let scale = ((job.caption.size_pt*dpi as f32/72.0)/8.0).round().clamp(1.0,8.0) as u32;
    let char_w=8*scale;
    let char_h=8*scale;
    let max_chars=(w.saturating_sub(10)/(char_w.max(1))) as usize;
    if max_chars==0 || h<char_h+6 {return Ok(());}
    let short = text.lines().next().unwrap_or("").chars().take(max_chars).collect::<String>();
    if short.is_empty() {return Ok(());}
    let bar_h=(char_h+8).min(h);
    let rgb=model::parse_color(&job.caption.text_color)?;
    for py in y+h-bar_h..y+h {
        for px in x..x+w {
            if overlay {
                *page.get_pixel_mut(px,py)=Rgba([0,0,0,job.caption.backdrop_opacity]);
            } else {
                let p=page.get_pixel_mut(px,py);
                let a=job.caption.backdrop_opacity as u16;
                for c in 0..3 {p.0[c]=((p.0[c] as u16*(255-a))/255) as u8;}
            }
        }
    }
    let tx=x+(w-(short.chars().count() as u32*char_w).min(w))/2;
    let ty=y+h-bar_h+4;
    for (i,c) in short.chars().enumerate() {
        let glyph=BASIC_FONTS.get(c).or_else(||BASIC_FONTS.get('?'));
        if let Some(rows)=glyph {
            for (dy,bits) in rows.iter().enumerate() {
                for dx in 0..8 {
                    if ((*bits >> dx) & 1) == 1 {
                        for sy in 0..scale {for sx in 0..scale {
                            let px=tx+i as u32*char_w+dx*scale+sx;
                            let py=ty+dy as u32*scale+sy;
                            if px<x+w && py<y+h {
                                *page.get_pixel_mut(px,py)=Rgba([rgb[0],rgb[1],rgb[2],255]);
                            }
                        }}
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(target_arch="wasm32")]
pub(crate) fn caption_overlay(job: &PrintJob, text: &str, width: u32, height: u32) -> Result<RgbaImage> {
    let mut page=RgbaImage::from_pixel(width,height,Rgba([0,0,0,0]));
    render_web_caption(&mut page,(0,0,width,height),text,job,job.dpi,true)?;
    Ok(page)
}
