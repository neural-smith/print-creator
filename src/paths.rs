//! Desktop-aware default output locations. Stored jobs keep their existing paths.
use std::{env, fs, path::{Path, PathBuf}};

fn expand_home(value: &str, home: &Path) -> Option<PathBuf> {
    // XDG user-dirs format normally contains "$HOME/Documents". Avoid executing
    // or interpreting arbitrary shell expansions from a configuration file.
    let value = value.trim().trim_matches('"').replace("\\ ", " ");
    for prefix in ["$HOME/", "${HOME}/", "~/"] {
        if let Some(suffix) = value.strip_prefix(prefix) {
            return Some(home.join(suffix));
        }
    }
    let value = Path::new(&value);
    value.is_absolute().then(|| value.to_path_buf())
}

fn document_dir_from_config(config: &str, home: &Path) -> Option<PathBuf> {
    config.lines()
        .filter_map(|line| {
            let line = line.trim();
            (!line.starts_with('#'))
                .then(|| line.strip_prefix("XDG_DOCUMENTS_DIR="))
                .flatten()
        })
        .find_map(|value| expand_home(value, home))
}

/// Prefer the configured XDG Documents directory, then ~/Documents, then home.
/// Never creates directories simply because the application is opened.
pub fn documents_dir() -> PathBuf {
    let Some(home) = env::var_os("HOME").map(PathBuf::from) else {
        return env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    };
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let xdg = fs::read_to_string(config.join("user-dirs.dirs"))
        .ok().and_then(|s| document_dir_from_config(&s, &home));
    for candidate in [xdg, Some(home.join("Documents"))].into_iter().flatten() {
        if candidate.is_dir() { return candidate; }
    }
    home
}

pub fn default_pdf_path() -> PathBuf { documents_dir().join("print-layout.pdf") }
pub fn default_images_dir() -> PathBuf { documents_dir().join("Print Creator") }

/// Use an existing parent, not a possibly invalid user-typed pathname, as the
/// initial folder in native file dialogs.
pub fn existing_dialog_directory(path: &Path, fallback: &Path) -> PathBuf {
    if path.is_dir() { return path.to_path_buf(); }
    let mut parent = path.parent();
    while let Some(p) = parent {
        if p.is_dir() { return p.to_path_buf(); }
        parent = p.parent();
    }
    fallback.to_path_buf()
}

pub fn with_pdf_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_none() { path.with_extension("pdf") } else { path }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xdg_documents_honors_custom_locations() {
        let home = Path::new("/home/sample");
        assert_eq!(document_dir_from_config(
            "# settings\nXDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_DOCUMENTS_DIR=\"$HOME/My\\ Docs\"",
            home), Some(home.join("My Docs")));
        assert_eq!(document_dir_from_config("XDG_DOCUMENTS_DIR=\"${HOME}/Documentos\"",home),
            Some(home.join("Documentos")));
    }
    #[test]
    fn xdg_configuration_cannot_execute_commands() {
        assert_eq!(document_dir_from_config("XDG_DOCUMENTS_DIR=\"$(touch /tmp/nope)\"",
            Path::new("/home/sample")), None);
    }
    #[test]
    fn missing_output_extension_is_filled() {
        assert_eq!(with_pdf_extension(PathBuf::from("layout")),PathBuf::from("layout.pdf"));
        assert_eq!(with_pdf_extension(PathBuf::from("layout.PDF")),PathBuf::from("layout.PDF"));
    }
    #[test]
    fn picks_existing_parent_directory() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(existing_dialog_directory(&tmp.path().join("nonexistent.pdf"),tmp.path()),tmp.path());
    }
}
