//! Browser-only in-memory photo store. No uploaded image leaves this tab.
use std::{cell::RefCell, collections::BTreeMap, path::{Path, PathBuf}};
use anyhow::{bail, Context, Result};

thread_local! {
    static STORE: RefCell<BTreeMap<PathBuf, Vec<u8>>> = RefCell::new(BTreeMap::new());
    static NEXT_ID: RefCell<u64> = const { RefCell::new(0) };
}

const MAX_FILE_BYTES: usize = 80 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 350 * 1024 * 1024;

pub fn contains(path: &Path) -> bool {
    STORE.with(|store| store.borrow().contains_key(path))
}

pub fn get(path: &Path) -> Option<Vec<u8>> {
    STORE.with(|store| store.borrow().get(path).cloned())
}

pub fn remove(path: &Path) {
    STORE.with(|store| { store.borrow_mut().remove(path); });
}

pub fn clear() {
    STORE.with(|store| store.borrow_mut().clear());
}

pub fn paths() -> Vec<PathBuf> {
    STORE.with(|store| store.borrow().keys().cloned().collect())
}

/// Verify image format and dimensions before storing bytes; path is virtual.
pub fn insert(name: &str, bytes: Vec<u8>) -> Result<PathBuf> {
    if bytes.is_empty() || bytes.len() > MAX_FILE_BYTES {
        bail!("image exceeds 80 MiB or is empty");
    }
    let filename = name.chars()
        .map(|c| if c == '/' || c == '\\' || c == '\0' {'_'} else {c})
        .collect::<String>();
    if filename.trim().is_empty() { bail!("image filename is empty"); }
    let format = image::guess_format(&bytes).context("unrecognized image file")?;
    if !matches!(format, image::ImageFormat::Jpeg | image::ImageFormat::Png
        | image::ImageFormat::Tiff | image::ImageFormat::WebP | image::ImageFormat::Bmp) {
        bail!("unsupported browser image format");
    }
    use image::ImageReader;
    let (w,h) = ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()?.into_dimensions().context("reading image dimensions")?;
    if w == 0 || h == 0 || (w as u64)*(h as u64) > 120_000_000 {
        bail!("image exceeds the 120-megapixel safety limit");
    }
    STORE.with(|store| -> Result<()> {
        let total: usize = store.borrow().values().map(Vec::len).sum();
        if total.saturating_add(bytes.len()) > MAX_TOTAL_BYTES {
            bail!("photo memory limit reached (350 MiB); remove photos before importing more");
        }
        Ok(())
    })?;
    let seq = NEXT_ID.with(|counter| {
        let mut next = counter.borrow_mut();
        *next += 1;
        *next
    });
    let path = PathBuf::from(format!("browser-images/{seq}/{filename}"));
    STORE.with(|store| { store.borrow_mut().insert(path.clone(),bytes); });
    Ok(path)
}
