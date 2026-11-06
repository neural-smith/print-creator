use std::{collections::BTreeMap, fs::File, io::BufReader, path::Path, process::Command};
use anyhow::{Context, Result};
use image::DynamicImage;

#[derive(Default, Clone, Debug)]
pub struct PhotoMetadata {
    pub orientation: u16,
    pub date: String,
    pub comment: String,
    pub exif: BTreeMap<String, String>,
}

pub fn read(path: &Path) -> PhotoMetadata {
    let mut metadata = PhotoMetadata { orientation: 1, ..PhotoMetadata::default() };
    let Ok(file) = File::open(path) else { return metadata };
    let Ok(exif) = exif::Reader::new().read_from_container(&mut BufReader::new(file)) else { return metadata };
    for field in exif.fields() {
        metadata.exif.insert(format!("{:?}", field.tag), field.display_value().to_string());
    }
    if let Some(field) = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY) {
        metadata.orientation = field.value.get_uint(0).unwrap_or(1) as u16;
    }
    for tag in [exif::Tag::DateTimeOriginal, exif::Tag::DateTimeDigitized, exif::Tag::DateTime] {
        if let Some(field) = exif.get_field(tag, exif::In::PRIMARY) {
            metadata.date = field.display_value().to_string().trim_matches('"').to_owned();
            if !metadata.date.is_empty() { break; }
        }
    }
    for tag in [exif::Tag::ImageDescription, exif::Tag::UserComment] {
        if let Some(field) = exif.get_field(tag, exif::In::PRIMARY) {
            metadata.comment = field.display_value().to_string().trim_matches('"').to_owned();
            if !metadata.comment.is_empty() { break; }
        }
    }
    metadata
}

pub fn open_photo(path: &Path) -> Result<DynamicImage> {
    match image::open(path) {
        Ok(img) => Ok(img),
        Err(original) => {
            // Optional RAW image fallback: the user's existing dcraw installation.
            // Unlike spawning a shell, Command passes file paths as literal arguments.
            let output = Command::new("dcraw").args(["-c", "-w", "-8"])
                .arg(path).output();
            match output {
                Ok(result) if result.status.success() =>
                    image::load_from_memory(&result.stdout)
                        .with_context(|| format!("RAW decoder returned an invalid image for {}", path.display())),
                _ => Err(anyhow::anyhow!("cannot decode {}: {original}; RAW files need dcraw in PATH", path.display())),
            }
        }
    }
}

pub fn orient(img: DynamicImage, exif_orientation: u16) -> DynamicImage {
    // Per EXIF orientation standard, including mirrored/transposed cases.
    match exif_orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

/// Apply EXIF orientation, then the user's explicit rotation and flips.
///
/// Automatic frame matching is only used for photos with no manual rotation.
/// Otherwise a portrait photo rotated to landscape (or vice versa) could be
/// silently rotated back to match the longer edge of the template.
///
/// Both the raster renderer and lossless PDF export use this function, so
/// their photo orientation and Fit/Fill placement always agree.
pub fn prepare_for_print(
    source: DynamicImage,
    exif_orientation: u16,
    photo: &crate::model::Photo,
    job: &crate::model::PrintJob,
    frame_width: f64,
    frame_height: f64,
) -> DynamicImage {
    use image::GenericImageView;

    let manual_rotation = photo.rotation.rem_euclid(360);
    let mut image = orient(source, exif_orientation);
    image = match manual_rotation {
        90 => image.rotate90(),
        180 => image.rotate180(),
        270 => image.rotate270(),
        _ => image,
    };
    if photo.flip_h { image = image.fliph(); }
    if photo.flip_v { image = image.flipv(); }

    let auto_rotate = manual_rotation == 0 && job.auto_rotate && match &job.layout {
        crate::model::Layout::Template { auto_rotate, .. } => *auto_rotate,
        _ => true,
    };
    if auto_rotate && frame_width > 0.0 && frame_height > 0.0 {
        let (width, height) = image.dimensions();
        if width > 0 && height > 0 {
            let ratio = width as f64 / height as f64;
            let target = frame_width / frame_height;
            let normal_score = (ratio / target).min(target / ratio);
            let swapped_ratio = 1.0 / ratio;
            let rotated_score = (swapped_ratio / target).min(target / swapped_ratio);
            if rotated_score > normal_score + 0.05 {
                image = image.rotate90();
            }
        }
    }
    image
}

pub fn display_caption(photo: &crate::model::Photo, job: &crate::model::PrintJob, index: usize, image_size: (u32, u32), metadata: &PhotoMetadata) -> String {
    use crate::model::CaptionType;
    if let Some(text) = &photo.caption_override { return text.clone(); }
    let filename = photo.path.file_name().unwrap_or_default().to_string_lossy();
    let stem = photo.path.file_stem().unwrap_or_default().to_string_lossy();
    match job.caption.kind {
        CaptionType::None => String::new(),
        CaptionType::Filename => filename.into_owned(),
        CaptionType::DateTime => metadata.date.clone(),
        CaptionType::Comment => metadata.comment.clone(),
        CaptionType::Custom => {
            let mut s = job.caption.custom.clone();
            for (key, value) in [
                ("{filename}", filename.to_string()),
                ("{stem}", stem.to_string()),
                ("{date}", metadata.date.clone()),
                ("{comment}", metadata.comment.clone()),
                ("{width}", image_size.0.to_string()),
                ("{height}", image_size.1.to_string()),
                ("{index}", (index + 1).to_string()),
            ] {
                s = s.replace(key, &value);
            }
            // {exif:Make}, {exif:Model}, {exif:FNumber}, {exif:ISOSpeed}, etc.
            for _ in 0..128 {
                let Some(start) = s.find("{exif:") else {break};
                let Some(end_offset) = s[start..].find('}') else { break; };
                let end = start + end_offset;
                let tag = &s[start + 6..end];
                let replacement = metadata.exif.get(tag).cloned().unwrap_or_default();
                s.replace_range(start..=end, &replacement);
            }
            s
        }
    }
}

#[cfg(test)]
mod print_orientation_tests {
    use super::*;
    use image::{GenericImageView, RgbImage};
    use crate::model::{Layout, Photo, PrintJob, RectMm};

    fn wide_image() -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::new(120, 60))
    }

    #[test]
    fn default_aligns_long_edge_to_long_edge() {
        let job = PrintJob::default();
        let photo = Photo::default();
        let out = prepare_for_print(wide_image(), 1, &photo, &job, 40.0, 80.0);
        assert_eq!(out.dimensions(), (60, 120));
    }

    #[test]
    fn manual_rotation_is_never_undone_by_autorotate() {
        let job = PrintJob::default();
        let mut photo = Photo::default();
        photo.rotation = 90;
        let out = prepare_for_print(wide_image(), 1, &photo, &job, 80.0, 40.0);
        assert_eq!(out.dimensions(), (60, 120));
        photo.rotation = 270;
        let out = prepare_for_print(wide_image(), 1, &photo, &job, 80.0, 40.0);
        assert_eq!(out.dimensions(), (60, 120));
        photo.rotation = 180;
        let out = prepare_for_print(wide_image(), 1, &photo, &job, 40.0, 80.0);
        assert_eq!(out.dimensions(), (120, 60));
    }

    #[test]
    fn template_autorotation_is_disabled_without_disabling_manual_rotation() {
        let mut job = PrintJob::default();
        job.layout = Layout::Template {
            name: "manual".into(),
            slots: vec![RectMm { x: 0.0, y: 0.0, width: 80.0, height: 40.0 }],
            auto_rotate: false,
        };
        let mut photo = Photo::default();
        photo.rotation = 90;
        let out = prepare_for_print(wide_image(), 1, &photo, &job, 80.0, 40.0);
        assert_eq!(out.dimensions(), (60, 120));
    }

    #[test]
    fn exif_orientation_is_applied_before_user_rotation() {
        let job = PrintJob::default();
        let mut photo = Photo::default();
        photo.rotation = 90;
        // EXIF 6 has already made the wide source portrait.
        let out = prepare_for_print(wide_image(), 6, &photo, &job, 40.0, 80.0);
        assert_eq!(out.dimensions(), (120, 60));
    }
}
