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
