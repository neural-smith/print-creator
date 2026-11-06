pub mod model;
pub mod template;
pub mod layout;
pub mod metadata;
pub mod render;
pub mod pdf;
pub mod output;
pub mod paths;
#[cfg(feature="gui")]
pub mod theme;
#[cfg(feature="gui")]
pub mod gui;

use std::path::PathBuf;
use walkdir::WalkDir;

/// Recursively collect local photo files, maintaining caller order. Filenames are
/// passed straight to the image decoder; unsupported RAW variants need dcraw.
pub fn collect_images(paths: &[PathBuf]) -> Vec<PathBuf> {
    fn supported(p: &std::path::Path) -> bool {
        let extension = p.extension().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
        matches!(extension.as_str(),
            "jpg"|"jpeg"|"png"|"tif"|"tiff"|"bmp"|"webp"|
            "dng"|"nef"|"cr2"|"cr3"|"arw"|"raf"|"rw2"|"orf"|"raw")
    }
    let mut files = Vec::new();
    for path in paths {
        if path.is_file() {
            if supported(path) { files.push(path.clone()); }
        } else if path.is_dir() {
            let mut nested = WalkDir::new(path).follow_links(false).into_iter()
                .filter_map(Result::ok)
                .map(|e| e.into_path())
                .filter(|p| p.is_file() && supported(p))
                .collect::<Vec<_>>();
            nested.sort();
            files.extend(nested);
        }
    }
    files
}

pub mod native_pdf;

pub mod cli_export;
pub const APP_ID: &str = "org.neural_smith.PrintCreator";
/// Name shown in the GUI and KDE window title.
pub const APP_NAME: &str = "Print Creator";

#[cfg(test)]
mod image_import_tests {
    use super::*;
    #[test]
    fn recursively_imports_supported_files_in_stable_order() {
        let folder = tempfile::tempdir().unwrap();
        let nested = folder.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        for name in ["z.JPG", "ignore.txt", "nested/b.png", "nested/a.webp"] {
            std::fs::write(folder.path().join(name), b"test").unwrap();
        }
        let result = collect_images(&[folder.path().to_path_buf()]);
        let relative = result.iter().map(|p| p.strip_prefix(folder.path()).unwrap().to_path_buf()).collect::<Vec<_>>();
        assert_eq!(relative, vec![
            PathBuf::from("nested/a.webp"), PathBuf::from("nested/b.png"),
            PathBuf::from("z.JPG"),
        ]);
    }
}
