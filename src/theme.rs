//! Catppuccin desktop styling and light/dark system preference detection.
//!
//! Both flavors are installed in separate egui style slots. The color scheme
//! can therefore switch without replacing the print job or rerendering pages.
//! KDE's XWayland windows may not receive a native system-theme event, so a
//! small background reader checks the XDG appearance portal and kdeglobals.

use eframe::egui;
use std::{path::PathBuf, sync::mpsc, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    Mocha,
    Latte,
}

impl Flavor {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mocha => "Mocha · dark",
            Self::Latte => "Latte · light",
        }
    }

    pub fn is_dark(self) -> bool { matches!(self, Self::Mocha) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    System,
    Dark,
    Light,
}

impl Appearance {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "Follow desktop",
            Self::Dark => "Mocha (dark)",
            Self::Light => "Latte (light)",
        }
    }

    pub fn resolve(self, desktop: Option<Flavor>, native: Option<Flavor>) -> Flavor {
        match self {
            Self::System => desktop.or(native).unwrap_or(Flavor::Mocha),
            Self::Dark => Flavor::Mocha,
            Self::Light => Flavor::Latte,
        }
    }
}

// Official Catppuccin palette, with Sapphire as the primary accent.
// https://catppuccin.com/palette/
#[derive(Clone, Copy)]
pub struct Palette {
    pub base: egui::Color32,
    pub mantle: egui::Color32,
    pub crust: egui::Color32,
    pub surface0: egui::Color32,
    pub surface1: egui::Color32,
    pub surface2: egui::Color32,
    pub overlay0: egui::Color32,
    pub text: egui::Color32,
    pub subtext: egui::Color32,
    pub sapphire: egui::Color32,
    pub peach: egui::Color32,
    pub red: egui::Color32,
    pub green: egui::Color32,
}

fn rgb(hex: u32) -> egui::Color32 {
    egui::Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub fn palette(flavor: Flavor) -> Palette {
    match flavor {
        Flavor::Mocha => Palette {
            base: rgb(0x1e1e2e), mantle: rgb(0x181825), crust: rgb(0x11111b),
            surface0: rgb(0x313244), surface1: rgb(0x45475a), surface2: rgb(0x585b70),
            overlay0: rgb(0x6c7086), text: rgb(0xcdd6f4), subtext: rgb(0xa6adc8),
            sapphire: rgb(0x74c7ec), peach: rgb(0xfab387), red: rgb(0xf38ba8),
            green: rgb(0xa6e3a1),
        },
        Flavor::Latte => Palette {
            base: rgb(0xeff1f5), mantle: rgb(0xe6e9ef), crust: rgb(0xdce0e8),
            surface0: rgb(0xccd0da), surface1: rgb(0xbcc0cc), surface2: rgb(0xacb0be),
            overlay0: rgb(0x9ca0b0), text: rgb(0x4c4f69), subtext: rgb(0x6c6f85),
            sapphire: rgb(0x209fb5), peach: rgb(0xfe640b), red: rgb(0xd20f39),
            green: rgb(0x40a02b),
        },
    }
}

fn themed_visuals(flavor: Flavor) -> egui::Visuals {
    let p = palette(flavor);
    let mut v = if flavor.is_dark() { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.override_text_color = Some(p.text);
    v.panel_fill = p.base;
    v.window_fill = p.mantle;
    v.faint_bg_color = p.mantle;
    v.extreme_bg_color = p.crust;
    v.code_bg_color = p.surface0;
    v.warn_fg_color = p.peach;
    v.error_fg_color = p.red;
    v.hyperlink_color = p.sapphire;
    v.selection.bg_fill = p.sapphire.gamma_multiply(0.30);
    v.selection.stroke = egui::Stroke::new(1.0_f32, p.sapphire);
    v.widgets.noninteractive.bg_fill = p.base;
    v.widgets.noninteractive.weak_bg_fill = p.base;
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, p.surface1);
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, p.text);
    v.widgets.inactive.bg_fill = p.surface0;
    v.widgets.inactive.weak_bg_fill = p.surface0;
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, p.surface1);
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, p.text);
    v.widgets.hovered.bg_fill = p.surface1;
    v.widgets.hovered.weak_bg_fill = p.surface1;
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.5_f32, p.sapphire);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, p.text);
    v.widgets.active.bg_fill = p.surface2;
    v.widgets.active.weak_bg_fill = p.surface2;
    v.widgets.active.bg_stroke = egui::Stroke::new(1.5_f32, p.sapphire);
    v.widgets.active.fg_stroke = egui::Stroke::new(1.5_f32, p.text);
    v.widgets.open.bg_fill = p.surface1;
    v.widgets.open.weak_bg_fill = p.surface1;
    v.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, p.sapphire);
    v.widgets.open.fg_stroke = egui::Stroke::new(1.0_f32, p.text);
    v.window_stroke = egui::Stroke::new(1.0_f32, p.surface1);
    v
}

/// Install both egui theme slots, so popups and menus use the correct flavor.
pub fn install(ctx: &egui::Context) {
    ctx.set_visuals_of(egui::Theme::Dark, themed_visuals(Flavor::Mocha));
    ctx.set_visuals_of(egui::Theme::Light, themed_visuals(Flavor::Latte));
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, |style| {
            style.spacing.item_spacing = egui::vec2(8.0, 7.0);
            style.spacing.button_padding = egui::vec2(11.0, 5.0);
            style.spacing.interact_size.y = 28.0;
        });
    }
}

fn from_egui(theme: egui::Theme) -> Flavor {
    match theme { egui::Theme::Light => Flavor::Latte, egui::Theme::Dark => Flavor::Mocha }
}

pub fn native_flavor(ctx: &egui::Context) -> Option<Flavor> {
    ctx.system_theme().map(from_egui)
}

/// Periodically reads the desktop preference off the UI thread. On XWayland,
/// native winit theme changes are not always delivered to the window.
pub fn watch_desktop() -> mpsc::Receiver<Option<Flavor>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut previous = None;
        loop {
            let current = read_desktop_theme();
            if previous != Some(current) {
                if tx.send(current).is_err() { break; }
                previous = Some(current);
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
    rx
}

/// `gdbus` is normally provided by GLib on Linux. The XDG portal works even
/// when an app cannot access host KDE configuration (e.g. inside Flatpak).
#[cfg(target_os = "linux")]
fn read_desktop_theme() -> Option<Flavor> {
    let portal = std::process::Command::new("gdbus")
        .args([
            "call", "--session", "--timeout", "1",
            "--dest", "org.freedesktop.portal.Desktop",
            "--object-path", "/org/freedesktop/portal/desktop",
            "--method", "org.freedesktop.portal.Settings.Read",
            "org.freedesktop.appearance", "color-scheme",
        ])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|s| parse_portal_scheme(&s));
    portal.or_else(read_kdeglobals)
}

#[cfg(not(target_os = "linux"))]
fn read_desktop_theme() -> Option<Flavor> { None }

/// Portal convention: 1=prefer dark, 2=prefer light, 0=no preference.
fn parse_portal_scheme(output: &str) -> Option<Flavor> {
    let (_, suffix) = output.split_once("uint32")?;
    let number: String = suffix.trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
    match number.as_str() {
        "1" => Some(Flavor::Mocha),
        "2" => Some(Flavor::Latte),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn read_kdeglobals() -> Option<Flavor> {
    let path = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
    })?.join("kdeglobals");
    let text = std::fs::read_to_string(path).ok()?;
    parse_kdeglobals(&text)
}

/// Prefer the actual window background over theme names, which may be custom.
fn parse_kdeglobals(config: &str) -> Option<Flavor> {
    let mut section = "";
    let mut scheme = None;
    let mut background = None;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            section = line;
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            if section == "[Colors:Window]" && key.trim() == "BackgroundNormal" {
                let values = value.split(',').map(|v| v.trim().parse::<u8>())
                    .collect::<Result<Vec<_>, _>>().ok();
                if let Some(rgb) = values.filter(|v| v.len() >= 3) {
                    let brightness = 0.2126 * f32::from(rgb[0])
                        + 0.7152 * f32::from(rgb[1]) + 0.0722 * f32::from(rgb[2]);
                    background = Some(if brightness < 128.0 { Flavor::Mocha } else { Flavor::Latte });
                }
            }
            if section == "[General]" && key.trim() == "ColorScheme" {
                let name = value.trim().to_ascii_lowercase();
                if name.contains("dark") || name.contains("mocha") || name.contains("macchiato") {
                    scheme = Some(Flavor::Mocha);
                } else if name.contains("light") || name.contains("latte") || name.contains("breeze") {
                    scheme = Some(Flavor::Latte);
                }
            }
        }
    }
    background.or(scheme)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_values_match_catppuccin() {
        let mocha = palette(Flavor::Mocha);
        let latte = palette(Flavor::Latte);
        assert_eq!(mocha.sapphire, rgb(0x74c7ec));
        assert_eq!(latte.sapphire, rgb(0x209fb5));
        assert_eq!(mocha.base, rgb(0x1e1e2e));
        assert_eq!(latte.base, rgb(0xeff1f5));
        assert!(themed_visuals(Flavor::Mocha).dark_mode);
        assert!(!themed_visuals(Flavor::Latte).dark_mode);
    }

    #[test]
    fn portal_variants_are_parsed() {
        assert_eq!(parse_portal_scheme("(<uint32 1>,)"), Some(Flavor::Mocha));
        assert_eq!(parse_portal_scheme("(<uint32 2>,)"), Some(Flavor::Latte));
        assert_eq!(parse_portal_scheme("(<uint32 0>,)"), None);
        assert_eq!(parse_portal_scheme("not a response"), None);
    }

    #[test]
    fn kde_theme_background_takes_precedence_over_name() {
        assert_eq!(parse_kdeglobals("[General]\nColorScheme=Breeze\n[Colors:Window]\nBackgroundNormal=27,28,42\n"), Some(Flavor::Mocha));
        assert_eq!(parse_kdeglobals("[Colors:Window]\nBackgroundNormal=240,242,247\n"), Some(Flavor::Latte));
        assert_eq!(parse_kdeglobals("[General]\nColorScheme=CatppuccinMochaSapphire\n"), Some(Flavor::Mocha));
        assert_eq!(parse_kdeglobals("[General]\nColorScheme=CatppuccinLatteSapphire\n"), Some(Flavor::Latte));
    }

    #[test]
    fn manual_override_takes_priority() {
        assert_eq!(Appearance::Dark.resolve(Some(Flavor::Latte), None), Flavor::Mocha);
        assert_eq!(Appearance::Light.resolve(Some(Flavor::Mocha), None), Flavor::Latte);
        assert_eq!(Appearance::System.resolve(None, Some(Flavor::Latte)), Flavor::Latte);
    }
}
