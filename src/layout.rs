use anyhow::{bail, Result};
use crate::model::{Layout, Page, Photo, RectMm};

pub fn slots(layout: &Layout, page: Page, photos: &[&Photo]) -> Result<Vec<RectMm>> {
    match layout {
        Layout::Grid { rows, columns, margin_mm, gap_mm } =>
            grid(page, *rows, *columns, *margin_mm, *gap_mm),
        Layout::Template { slots, .. } => {
            if slots.is_empty() { bail!("empty template"); }
            for s in slots {
                if s.x < 0. || s.y < 0. || s.width <= 0. || s.height <= 0.
                    || s.x + s.width > page.width_mm + 0.1
                    || s.y + s.height > page.height_mm + 0.1
                { bail!("template slot is outside the page"); }
            }
            Ok(slots.clone())
        },
        Layout::Adaptive { photos_per_page, margin_mm, gap_mm } => {
            let count = photos.len().min(*photos_per_page as usize);
            if count == 0 { bail!("no photos to arrange"); }
            let ratios = photos.iter().take(count).map(|p| {
                let (mut w, mut h) = image::image_dimensions(&p.path).unwrap_or((4, 3));
                let exif_orientation = crate::metadata::read(&p.path).orientation;
                if matches!(exif_orientation, 5 | 6 | 7 | 8) {
                    std::mem::swap(&mut w, &mut h);
                }
                if matches!(p.rotation.rem_euclid(360), 90 | 270) {
                    std::mem::swap(&mut w, &mut h);
                }
                w as f64 / h.max(1) as f64
            }).collect::<Vec<_>>();
            let mut winner = None::<(f64, Vec<RectMm>)>;
            for rows in 1..=count as u32 {
                let columns = (count as u32 + rows - 1) / rows;
                let Ok(rects) = grid(page, rows, columns, *margin_mm, *gap_mm) else { continue };
                // Fit based on source aspect ratios; prefer fewer unused cells.
                let mut score = -((rects.len() - count) as f64) * 0.15;
                for (ratio, r) in ratios.iter().zip(rects.iter()) {
                    let cell_ratio = r.width as f64 / r.height as f64;
                    let efficiency = (*ratio / cell_ratio).min(cell_ratio / *ratio);
                    score += efficiency;
                }
                score /= count as f64;
                if winner.as_ref().is_none_or(|(best, _)| score > *best) {
                    winner = Some((score, rects));
                }
            }
            winner.map(|(_, r)| r).ok_or_else(|| anyhow::anyhow!("no adaptive layout fits on paper"))
        },
    }
}

pub fn grid(page: Page, rows: u32, columns: u32, margin: f32, gap: f32) -> Result<Vec<RectMm>> {
    if rows == 0 || columns == 0 || rows.saturating_mul(columns) > 1000
        || !margin.is_finite() || !gap.is_finite() || margin < 0. || gap < 0.
    { bail!("invalid grid settings"); }
    let cell_w = (page.width_mm - 2. * margin - gap * (columns-1) as f32) / columns as f32;
    let cell_h = (page.height_mm - 2. * margin - gap * (rows-1) as f32) / rows as f32;
    if cell_w <= 0. || cell_h <= 0. { bail!("grid does not fit on the page"); }
    let mut out = Vec::with_capacity((rows*columns) as usize);
    for r in 0..rows {
        for c in 0..columns {
            out.push(RectMm {
                x: margin + c as f32 * (cell_w + gap),
                y: margin + r as f32 * (cell_h + gap),
                width: cell_w, height: cell_h,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a4_grid() {
        let sheet = Page::from_name("a4").unwrap();
        let s = grid(sheet, 2, 2, 5., 4.).unwrap();
        assert_eq!(s.len(), 4);
        assert!(s[3].x+s[3].width <= sheet.width_mm);
    }
}

/// Extend template slots to the paper edges and the halfway point of gaps.
/// Adjacent slots in the same row/column meet without visible white gutters.
/// A slot may still show background if the photo uses Fit instead of Fill.
pub fn borderless_template_slots(slots: &[RectMm], page: Page) -> Vec<RectMm> {
    const EPS: f32 = 0.001;
    slots.iter().enumerate().map(|(i, s)| {
        let mut left = 0.0_f32;
        let mut right = page.width_mm;
        let mut top = 0.0_f32;
        let mut bottom = page.height_mm;
        for (j, other) in slots.iter().enumerate() {
            if i == j { continue; }
            let vertical_overlap = s.y < other.y + other.height - EPS && other.y < s.y + s.height - EPS;
            let horizontal_overlap = s.x < other.x + other.width - EPS && other.x < s.x + s.width - EPS;
            if vertical_overlap && other.x + other.width <= s.x + EPS {
                left = left.max((other.x + other.width + s.x) / 2.0);
            }
            if vertical_overlap && other.x >= s.x + s.width - EPS {
                right = right.min((s.x + s.width + other.x) / 2.0);
            }
            if horizontal_overlap && other.y + other.height <= s.y + EPS {
                top = top.max((other.y + other.height + s.y) / 2.0);
            }
            if horizontal_overlap && other.y >= s.y + s.height - EPS {
                bottom = bottom.min((s.y + s.height + other.y) / 2.0);
            }
        }
        // Diagonally positioned collage frames may share neither a row nor a
        // column. Use the nearest non-overlapping edge as a fallback, avoiding
        // the otherwise full-page overlapping expansion of both frames.
        for (j, other) in slots.iter().enumerate() {
            if i == j { continue; }
            if left <= EPS && other.x + other.width <= s.x + EPS {
                left = left.max((other.x + other.width + s.x) / 2.0);
            }
            if right >= page.width_mm - EPS && other.x >= s.x + s.width - EPS {
                right = right.min((s.x + s.width + other.x) / 2.0);
            }
            if top <= EPS && other.y + other.height <= s.y + EPS {
                top = top.max((other.y + other.height + s.y) / 2.0);
            }
            if bottom >= page.height_mm - EPS && other.y >= s.y + s.height - EPS {
                bottom = bottom.min((s.y + s.height + other.y) / 2.0);
            }
        }
        // Overlapping custom templates have no unambiguous gap to remove.
        if left >= right || top >= bottom { *s } else {
            RectMm { x: left, y: top, width: right-left, height: bottom-top }
        }
    }).collect()
}

pub fn job_slots(job: &crate::model::PrintJob, photos: &[&Photo]) -> Result<Vec<RectMm>> {
    let frames = slots(&job.layout, job.page, photos)?;
    if job.borderless_templates && matches!(&job.layout, Layout::Template {..}) {
        Ok(borderless_template_slots(&frames,job.page))
    } else { Ok(frames) }
}

#[cfg(test)]
mod borderless_tests {
    use super::*;
    #[test]
    fn one_template_slot_fills_whole_page() {
        let page = Page::from_name("a4").unwrap();
        let slots = [RectMm{x:5., y:7., width:190., height:280.}];
        assert_eq!(borderless_template_slots(&slots,page), vec![RectMm{x:0.,y:0.,width:210.,height:297.}]);
    }
    #[test]
    fn diagonally_placed_slots_do_not_overlap_after_expansion() {
        let page=Page {width_mm:100.,height_mm:100.};
        let a=[RectMm{x:5.,y:5.,width:35.,height:35.},
               RectMm{x:60.,y:60.,width:35.,height:35.}];
        let b=borderless_template_slots(&a,page);
        assert!(b[0].x+b[0].width <= b[1].x);
        assert!(b[0].y+b[0].height <= b[1].y);
    }
    #[test]
    fn grid_template_has_no_margins_or_gaps() {
        let page=Page {width_mm:100.,height_mm:100.};
        let a=[RectMm{x:5.,y:5.,width:40.,height:40.},RectMm{x:55.,y:5.,width:40.,height:40.},
            RectMm{x:5.,y:55.,width:40.,height:40.},RectMm{x:55.,y:55.,width:40.,height:40.}];
        let b=borderless_template_slots(&a,page);
        for (i,r) in b.iter().enumerate() {
            assert_eq!(r.width,50.); assert_eq!(r.height,50.);
            assert_eq!(r.x,(i%2) as f32*50.);assert_eq!(r.y,(i/2) as f32*50.);
        }
    }
}
