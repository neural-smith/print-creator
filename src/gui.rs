#![cfg(feature = "gui")]
use std::{path::PathBuf, sync::mpsc, time::Duration};
use anyhow::Result;
use eframe::egui;
use crate::{model::{CaptionType,ConflictRule,CropMode,ImageFormat,Layout,OutputTarget,Page,Photo,PrintJob},render,template::{self,TemplateDef},output,paths,theme::{self,Appearance,Flavor}};

pub fn launch() -> Result<()> {
    // winit 0.30, which is used by eframe 0.31, does not deliver file-drop
    // events on native Wayland. XWayland does. When DISPLAY is available,
    // prefer it for working Dolphin-to-window drag-and-drop. Opt out with
    // RPC_NATIVE_WAYLAND=1 to retain native rendering and use file dialogs.
    #[cfg(target_os = "linux")]
    if std::env::var_os("RPC_NATIVE_WAYLAND").is_none()
        && std::env::var_os("DISPLAY").is_some()
        && (std::env::var_os("WAYLAND_DISPLAY").is_some()
            || std::env::var_os("WAYLAND_SOCKET").is_some())
    {
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("WAYLAND_SOCKET");
    }
    let icon_bytes = include_bytes!("../assets/icons/org.neural_smith.PrintCreator-256.png");
    let icon_image = image::load_from_memory(icon_bytes)
        .map_err(|e|anyhow::anyhow!("application icon: {e}"))?.to_rgba8();
    let (width, height) = icon_image.dimensions();
    let icon = egui::IconData { rgba: icon_image.into_raw(), width, height };
    let options=eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id(crate::APP_ID).with_icon(icon)
            .with_drag_and_drop(true)
            .with_inner_size([1330.,850.]).with_min_inner_size([950.,600.]),
        ..Default::default()
    };
    eframe::run_native(crate::APP_NAME,options,Box::new(|ctx| {
        theme::install(&ctx.egui_ctx);
        Ok(Box::new(PrintCreatorApp::default()))
    })).map_err(|e|anyhow::anyhow!("GUI startup failed: {e}"))
}

struct PrintCreatorApp {
    job: PrintJob,
    templates: Vec<TemplateDef>,
    preview_page: usize,
    preview: Option<egui::TextureHandle>,
    preview_key: String,
    preview_pending: bool,
    preview_tx: mpsc::Sender<PreviewRequest>,
    preview_rx: mpsc::Receiver<PreviewResponse>,
    import_tx: mpsc::Sender<Vec<PathBuf>>,
    import_rx: mpsc::Receiver<Vec<PathBuf>>,
    imports_pending: usize,
    status: String,
    pending: Option<mpsc::Receiver<Result<Vec<PathBuf>,String>>>,
    dialog_rx: Option<mpsc::Receiver<DialogSelection>>,
    recent_image_dir: PathBuf,
    pdf_path: PathBuf,
    image_dir: PathBuf,
    template_filter: String,
    appearance: Appearance,
    applied_preference: Option<Appearance>,
    active_flavor: Flavor,
    desktop_theme: Option<Flavor>,
    desktop_theme_rx: mpsc::Receiver<Option<Flavor>>,
    show_about: bool,
}

struct PreviewRequest {
    key: String,
    job: PrintJob,
    page: usize,
}

struct PreviewResponse {
    key: String,
    rendered: std::result::Result<image::RgbaImage, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DialogKind {
    AddImages, AddFolder, ImportTemplates, ExportTemplate, SaveJob, LoadJob,
    FontFile, OutputPdf, OutputFolder,
}

struct DialogSelection {
    kind: DialogKind,
    paths: Vec<PathBuf>,
}

// A dropped file may be a whole directory with thousands of entries.
// Discover files outside the UI event loop, preserving drop and folder order.
fn spawn_image_importer() -> (mpsc::Sender<Vec<PathBuf>>, mpsc::Receiver<Vec<PathBuf>>) {
    let (request_tx, request_rx) = mpsc::channel::<Vec<PathBuf>>();
    let (result_tx, result_rx) = mpsc::channel::<Vec<PathBuf>>();
    std::thread::spawn(move || {
        while let Ok(paths) = request_rx.recv() {
            if result_tx.send(crate::collect_images(&paths)).is_err() { break; }
        }
    });
    (request_tx, result_rx)
}

fn local_dropped_paths(files: &[egui::DroppedFile]) -> Vec<PathBuf> {
    files.iter().filter_map(|file| file.path.clone()).collect()
}

fn spawn_preview_worker() -> (mpsc::Sender<PreviewRequest>, mpsc::Receiver<PreviewResponse>) {
    let (request_tx, request_rx) = mpsc::channel::<PreviewRequest>();
    let (result_tx, result_rx) = mpsc::channel::<PreviewResponse>();
    std::thread::spawn(move || {
        while let Ok(mut request) = request_rx.recv() {
            // When rapidly dragging a slider or resizing a panel, skip superseded jobs.
            while let Ok(newer) = request_rx.try_recv() { request = newer; }
            let rendered = render::render_page(&request.job, request.page, 72)
                .map_err(|e| format!("{e:#}"));
            if result_tx.send(PreviewResponse {key: request.key, rendered}).is_err() { break; }
        }
    });
    (request_tx, result_rx)
}

impl Default for PrintCreatorApp {
    fn default() -> Self {
        let (preview_tx, preview_rx) = spawn_preview_worker();
        let (import_tx, import_rx) = spawn_image_importer();
        let job = PrintJob::default();
        Self {
            pdf_path: job.output.path.clone(),
            image_dir: paths::default_images_dir(),
            recent_image_dir: paths::documents_dir(),
            job, templates: template::builtin(),
            preview_page: 0, preview: None, preview_key: String::new(),
            preview_pending: false, preview_tx, preview_rx,
            import_tx, import_rx, imports_pending: 0,
            status: "Add images to start.".into(), pending: None, dialog_rx: None,
            template_filter: String::new(),
            appearance: Appearance::System,
            applied_preference: None,
            active_flavor: Flavor::Mocha,
            desktop_theme: None,
            desktop_theme_rx: theme::watch_desktop(),
            show_about: false,
        }
    }
}

// Internal print geometry remains in millimeters to preserve existing JSON jobs.
// Only the GUI's physical-size inputs are presented in centimeters.
fn cm_field(ui: &mut egui::Ui, label: &str, millimeters: &mut f32, min_cm: f32, max_cm: f32) {
    let mut centimeters = *millimeters / 10.0;
    ui.vertical(|ui| {
        ui.label(format!("{label} (cm)"));
        if ui.add_sized([ui.available_width().max(65.0), 21.0],
            egui::DragValue::new(&mut centimeters)
                .range(min_cm..=max_cm).speed(0.05).fixed_decimals(2)).changed() {
            *millimeters = centimeters * 10.0;
        }
    });
}

fn cm_pair(ui: &mut egui::Ui, a_label: &str, a: &mut f32,
           b_label: &str, b: &mut f32, min: f32, max: f32) {
    ui.columns(2, |columns| {
        cm_field(&mut columns[0], a_label, a, min, max);
        cm_field(&mut columns[1], b_label, b, min, max);
    });
}

fn output_target_name(target: OutputTarget) -> &'static str {
    match target {
        OutputTarget::Pdf => "PDF document",
        OutputTarget::Images => "Page images",
        OutputTarget::Gimp => "Export and open in GIMP",
        OutputTarget::Printer => "CUPS printer",
    }
}

fn image_format_name(format: ImageFormat) -> &'static str {
    match format { ImageFormat::Png => "PNG (lossless)", ImageFormat::Jpeg => "JPEG", ImageFormat::Tiff => "TIFF (lossless)" }
}

fn conflict_name(rule: ConflictRule) -> &'static str {
    match rule {
        ConflictRule::Overwrite => "Overwrite existing files",
        ConflictRule::Skip => "Skip existing files",
        ConflictRule::Rename => "Rename new files",
        ConflictRule::Error => "Stop if a file exists",
    }
}

fn page_preset_name(page: Page) -> String {
    let names = ["A0", "A1", "A2", "A3", "A4", "A5", "A6", "Letter", "Legal",
        "Tabloid", "9x13", "10x15", "13x18", "20x25", "20.32x25.4"];
    for name in names {
        if let Some(preset) = Page::from_name(name) {
            if (page.width_mm - preset.width_mm).abs() < 0.02
                && (page.height_mm - preset.height_mm).abs() < 0.02 {
                return name.replace('x', " × ");
            }
            if (page.width_mm - preset.height_mm).abs() < 0.02
                && (page.height_mm - preset.width_mm).abs() < 0.02 {
                return format!("{} (landscape)", name.replace('x', " × "));
            }
        }
    }
    "Custom size".into()
}

fn section_heading(ui: &mut egui::Ui, title: &str, accent: egui::Color32) {
    ui.add_space(5.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("▍").size(18.0).color(accent));
        ui.label(egui::RichText::new(title).size(17.0).strong());
    });
    ui.add_space(3.0);
}

// Drawing guides belong to the preview only: never modify the rendered
// bitmap, exported PDF, image output, or actual printer job.
fn preview_mm_rect(page: Page, paper: egui::Rect, measured: crate::model::RectMm) -> Option<egui::Rect> {
    if page.width_mm <= 0.0 || page.height_mm <= 0.0
        || !page.width_mm.is_finite() || !page.height_mm.is_finite()
        || ![measured.x, measured.y, measured.width, measured.height].iter().all(|v| v.is_finite())
        || measured.width <= 0.0 || measured.height <= 0.0
    { return None; }
    let left = paper.left() + paper.width() * measured.x / page.width_mm;
    let top = paper.top() + paper.height() * measured.y / page.height_mm;
    let right = paper.left() + paper.width() * (measured.x + measured.width) / page.width_mm;
    let bottom = paper.top() + paper.height() * (measured.y + measured.height) / page.height_mm;
    Some(egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom)))
}

fn preview_outline(painter: &egui::Painter, rect: egui::Rect, stroke: egui::Stroke, dashed: bool) {
    let sides = [
        (rect.left_top(), rect.right_top()),
        (rect.right_top(), rect.right_bottom()),
        (rect.right_bottom(), rect.left_bottom()),
        (rect.left_bottom(), rect.left_top()),
    ];
    for (start, end) in sides {
        if !dashed {
            painter.line_segment([start, end], stroke);
            continue;
        }
        // Use a fixed-size on-screen dash so guides remain readable at any zoom.
        let direction = end - start;
        let length = direction.length();
        if length < 1.0 { continue; }
        let unit = direction / length;
        let mut pos = 0.0;
        while pos < length {
            let next = (pos + 6.0).min(length);
            painter.line_segment([start + unit * pos, start + unit * next], stroke);
            pos += 10.0;
        }
    }
}

fn preview_layout_area(page: Page, layout: &Layout) -> Option<crate::model::RectMm> {
    let margin = match layout {
        Layout::Grid { margin_mm, .. } | Layout::Adaptive { margin_mm, .. } => *margin_mm,
        Layout::Template { .. } => return None,
    };
    if !margin.is_finite() || margin < 0.0 || margin * 2.0 >= page.width_mm
        || margin * 2.0 >= page.height_mm { return None; }
    Some(crate::model::RectMm {
        x: margin, y: margin,
        width: page.width_mm - 2.0 * margin,
        height: page.height_mm - 2.0 * margin,
    })
}

impl PrintCreatorApp {
    fn draw_preview_guides(&self, ui: &egui::Ui, paper: egui::Rect) {
        let painter = ui.painter();
        // Preview-only Catppuccin guides: page border and template frames.
        // These are never composited into the printed or exported image.
        let colors = theme::palette(self.active_flavor);
        preview_outline(painter, paper, egui::Stroke::new(2.0_f32, colors.sapphire), false);
        let inside = egui::Stroke::new(1.5_f32, colors.peach);
        if let Some(area) = preview_layout_area(self.job.page, &self.job.layout) {
            // Zero-margin layout is the entire paper, so the paper edge alone is enough.
            if area.x > 0.0 {
                if let Some(rect) = preview_mm_rect(self.job.page, paper, area) {
                    preview_outline(painter, rect, inside, true);
                }
            }
        }
        if let Layout::Template { slots, .. } = &self.job.layout {
            let adjusted = if self.job.borderless_templates {
                crate::layout::borderless_template_slots(slots, self.job.page)
            } else { slots.clone() };
            // A custom digiKam layout does not have one common margin. The
            // individual slot outlines are its actual image placement regions.
            for slot in &adjusted {
                if let Some(rect) = preview_mm_rect(self.job.page, paper, *slot) {
                    preview_outline(painter, rect, inside, true);
                }
            }
        }
    }

    fn add_paths(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() { return; }
        let first = &paths[0];
        self.recent_image_dir = paths::existing_dialog_directory(first, &paths::documents_dir());
        if self.import_tx.send(paths).is_ok() {
            self.imports_pending += 1;
            self.status = "Scanning images...".into();
        } else {
            self.status = "Image import worker is unavailable.".into();
        }
    }

    fn poll_import(&mut self, ctx: &egui::Context) {
        while let Ok(paths) = self.import_rx.try_recv() {
            self.imports_pending = self.imports_pending.saturating_sub(1);
            if paths.is_empty() {
                self.status = "No supported image files found in the selection.".into();
            } else {
                let count = paths.len();
                self.job.photos.extend(paths.into_iter().map(Photo::from_path));
                self.status = format!("Added {count} image(s). Total: {}", self.job.photos.len());
            }
        }
        if self.imports_pending > 0 { ctx.request_repaint_after(Duration::from_millis(100)); }
    }

    fn handle_dropped_images(&mut self, ctx: &egui::Context) {
        let files = ctx.input(|input| input.raw.dropped_files.clone());
        if files.is_empty() { return; }
        let paths = local_dropped_paths(&files);
        if paths.is_empty() {
            self.status = "Drop local image files or folders (file paths are required).".into();
        } else {
            self.add_paths(paths);
        }
    }

    fn show_drop_overlay(&self, ctx: &egui::Context) {
        if !ctx.input(|input| !input.raw.hovered_files.is_empty()) { return; }
        let rect = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground, egui::Id::new("photo-import-drop-overlay")));
        let colors = theme::palette(self.active_flavor);
        painter.rect_filled(rect, 0, egui::Color32::from_rgba_unmultiplied(
            colors.crust.r(), colors.crust.g(), colors.crust.b(), 225));
        painter.text(rect.center(), egui::Align2::CENTER_CENTER,
            "Drop images or folders to add them to this print job",
            egui::FontId::proportional(23.0), colors.sapphire);
    }

    fn set_output_target(&mut self, target: OutputTarget) {
        if target == self.job.output.target { return; }
        match self.job.output.target {
            OutputTarget::Pdf => self.pdf_path = self.job.output.path.clone(),
            OutputTarget::Images | OutputTarget::Gimp => self.image_dir = self.job.output.path.clone(),
            OutputTarget::Printer => {},
        }
        self.job.output.target = target;
        self.job.output.path = match target {
            OutputTarget::Pdf => self.pdf_path.clone(),
            OutputTarget::Images | OutputTarget::Gimp => self.image_dir.clone(),
            OutputTarget::Printer => self.pdf_path.clone(),
        };
    }

    fn start_dialog(&mut self, kind: DialogKind) {
        if self.dialog_rx.is_some() { return; }
        let documents = paths::documents_dir();
        let directory = match kind {
            DialogKind::AddImages | DialogKind::AddFolder => self.recent_image_dir.clone(),
            DialogKind::OutputPdf | DialogKind::OutputFolder =>
                paths::existing_dialog_directory(&self.job.output.path, &documents),
            DialogKind::FontFile => self.job.caption.font_file.as_ref()
                .map_or_else(|| documents.clone(), |p| paths::existing_dialog_directory(p, &documents)),
            _ => documents,
        };
        let pdf_filename = self.job.output.path.file_name()
            .map(|x| x.to_string_lossy().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "print-layout.pdf".into());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let dialog = rfd::FileDialog::new().set_directory(&directory);
            let paths: Vec<PathBuf> = match kind {
                DialogKind::AddImages => dialog.set_title("Select photos")
                    .add_filter("Images", &["jpg", "jpeg", "png", "tif", "tiff", "bmp", "webp", "dng", "nef", "cr2", "arw"])
                    .pick_files().unwrap_or_default(),
                DialogKind::AddFolder => dialog.set_title("Add an image folder")
                    .pick_folder().into_iter().collect(),
                DialogKind::ImportTemplates => dialog.set_title("Import digiKam layout templates")
                    .add_filter("digiKam XML", &["xml"]).pick_file().into_iter().collect(),
                DialogKind::ExportTemplate => dialog.set_title("Save digiKam template")
                    .add_filter("digiKam XML", &["xml"]).set_file_name("my-photo-layout.xml")
                    .save_file().into_iter().collect(),
                DialogKind::SaveJob => dialog.set_title("Save print job")
                    .add_filter("JSON print job", &["json"]).set_file_name("print-job.json")
                    .save_file().into_iter().collect(),
                DialogKind::LoadJob => dialog.set_title("Open print job")
                    .add_filter("JSON print job", &["json"]).pick_file().into_iter().collect(),
                DialogKind::FontFile => dialog.set_title("Choose a caption font")
                    .add_filter("Font", &["ttf", "otf"]).pick_file().into_iter().collect(),
                DialogKind::OutputPdf => dialog.set_title("Save print layout PDF")
                    .add_filter("PDF document", &["pdf"]).set_file_name(pdf_filename)
                    .save_file().into_iter().collect(),
                DialogKind::OutputFolder => dialog.set_title("Choose output folder")
                    .pick_folder().into_iter().collect(),
            };
            let _ = tx.send(DialogSelection {kind, paths});
        });
        self.dialog_rx = Some(rx);
        self.status = "Select a location in the file dialog...".into();
    }

    fn poll_dialog(&mut self, ctx: &egui::Context) {
        let selection = self.dialog_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(selection) = selection {
            self.dialog_rx = None;
            let Some(path) = selection.paths.first().cloned() else {
                self.status = "File selection canceled.".into();
                return;
            };
            match selection.kind {
                DialogKind::AddImages | DialogKind::AddFolder => {
                    self.recent_image_dir = paths::existing_dialog_directory(
                        &path, &paths::documents_dir());
                    self.add_paths(selection.paths);
                }
                DialogKind::ImportTemplates => match template::load_xml(&path) {
                    Ok(new) => {
                        let count = new.len();
                        self.templates.extend(new);
                        self.status = format!("Imported {count} digiKam layout(s)");
                    }
                    Err(e) => self.status = format!("Template import failed: {e:#}"),
                },
                DialogKind::ExportTemplate => {
                    match template::write_compatible_template(&self.job, "My Photo Layout", &path) {
                        Ok(()) => self.status = format!("Saved template: {}", path.display()),
                        Err(e) => self.status = format!("Template export failed: {e:#}"),
                    }
                }
                DialogKind::SaveJob => match serde_json::to_string_pretty(&self.job)
                    .map_err(anyhow::Error::from)
                    .and_then(|text| std::fs::write(&path, text).map_err(anyhow::Error::from)) {
                    Ok(()) => self.status = format!("Saved {}", path.display()),
                    Err(e) => self.status = format!("Save failed: {e:#}"),
                },
                DialogKind::LoadJob => match std::fs::read_to_string(&path)
                    .map_err(anyhow::Error::from)
                    .and_then(|text| serde_json::from_str::<PrintJob>(&text).map_err(anyhow::Error::from)) {
                    Ok(job) => {
                        self.job = job;
                        self.preview_page = 0;
                        match self.job.output.target {
                            OutputTarget::Pdf | OutputTarget::Printer => self.pdf_path = self.job.output.path.clone(),
                            OutputTarget::Images | OutputTarget::Gimp => self.image_dir = self.job.output.path.clone(),
                        }
                        self.status = format!("Loaded {}", path.display());
                    }
                    Err(e) => self.status = format!("Load failed: {e:#}"),
                },
                DialogKind::FontFile => {
                    self.job.caption.font_file = Some(path);
                    self.status = "Caption font selected.".into();
                }
                DialogKind::OutputPdf => {
                    if self.job.output.target == OutputTarget::Pdf {
                        self.job.output.path = paths::with_pdf_extension(path);
                        self.pdf_path = self.job.output.path.clone();
                        self.status = format!("PDF destination: {}", self.job.output.path.display());
                    }
                }
                DialogKind::OutputFolder => {
                    if matches!(self.job.output.target, OutputTarget::Images | OutputTarget::Gimp) {
                        self.job.output.path = path;
                        self.image_dir = self.job.output.path.clone();
                        self.status = format!("Output folder: {}", self.job.output.path.display());
                    }
                }
            }
        } else if self.dialog_rx.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn start_export(&mut self) {
        if self.pending.is_some() {return;}
        if self.job.output.target != OutputTarget::Printer
            && self.job.output.path.as_os_str().is_empty() {
            self.status = "Choose an output file or folder before exporting.".into();
            return;
        }
        if self.job.output.target == OutputTarget::Pdf {
            self.job.output.path = paths::with_pdf_extension(self.job.output.path.clone());
        }
        if let Err(e)=self.job.validate() {self.status=format!("Invalid job: {e:#}");return;}
        let job=self.job.clone();
        let (tx,rx)=mpsc::channel();
        std::thread::spawn(move || {
            let result=output::export(&job).map_err(|e|format!("{e:#}"));
            let _=tx.send(result);
        });
        self.pending=Some(rx);
        self.status="Rendering and exporting...".into();
    }

    fn poll_export(&mut self,ctx:&egui::Context) {
        let result=self.pending.as_ref().and_then(|rx|rx.try_recv().ok());
        if let Some(result)=result {
            self.pending=None;
            self.status=match result {
                Ok(paths) if paths.is_empty() => "Finished (sent to printer or skipped existing output).".into(),
                Ok(paths) => format!("Export complete: {} file(s). Last: {}",paths.len(),paths.last().unwrap().display()),
                Err(e) => format!("Export error: {e}"),
            };
        } else if self.pending.is_some() {ctx.request_repaint_after(Duration::from_millis(150));}
    }

    fn queue_preview(&mut self, ctx: &egui::Context) {
        let page_count = self.job.page_count();
        if page_count == 0 { self.preview_page = 0; }
        else { self.preview_page = self.preview_page.min(page_count - 1); }
        // Exclude output destination, compression, UI-only selections and status.
        // Editing a filename must never cause the photos to be decoded again.
        let key = serde_json::to_string(&(
            self.preview_page, &self.job.page, &self.job.layout, &self.job.photos,
            &self.job.auto_rotate, &self.job.disable_crop, &self.job.borderless_templates,
            &self.job.background, &self.job.caption,
        )).unwrap_or_default();
        if self.preview_key == key { return; }
        self.preview_key = key.clone();
        if self.job.photos.is_empty() || page_count == 0 {
            self.preview = None;
            self.preview_pending = false;
            return;
        }
        let request = PreviewRequest {key, page: self.preview_page, job: self.job.clone()};
        if self.preview_tx.send(request).is_ok() {
            self.preview_pending = true;
            ctx.request_repaint_after(Duration::from_millis(100));
        } else {
            self.preview_pending = false;
            self.status = "Preview rendering service stopped.".into();
        }
    }

    fn poll_preview(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.preview_rx.try_recv() {
            if result.key != self.preview_key || !self.preview_pending { continue; }
            self.preview_pending = false;
            match result.rendered {
                Ok(page) => {
                    let size = [page.width() as usize, page.height() as usize];
                    let img = egui::ColorImage::from_rgba_unmultiplied(size, page.as_raw());
                    self.preview = Some(ctx.load_texture("preview-page", img, egui::TextureOptions::LINEAR));
                }
                Err(e) => {
                    self.preview = None;
                    self.status = format!("Preview unavailable: {e}");
                }
            }
        }
        if self.preview_pending { ctx.request_repaint_after(Duration::from_millis(100)); }
    }

    fn sync_appearance(&mut self, ctx: &egui::Context) {
        while let Ok(preference) = self.desktop_theme_rx.try_recv() {
            self.desktop_theme = preference;
        }

        // Use the portal or KDE's actual window colors where available.
        // Otherwise leave egui in System mode so native events can be used.
        let desired = match self.appearance {
            Appearance::System => match self.desktop_theme {
                Some(Flavor::Mocha) => Appearance::Dark,
                Some(Flavor::Latte) => Appearance::Light,
                None => Appearance::System,
            },
            manual => manual,
        };
        if self.applied_preference != Some(desired) {
            ctx.set_theme(match desired {
                Appearance::System => egui::ThemePreference::System,
                Appearance::Dark => egui::ThemePreference::Dark,
                Appearance::Light => egui::ThemePreference::Light,
            });
            self.applied_preference = Some(desired);
        }
        self.active_flavor = if desired == Appearance::System {
            theme::native_flavor(ctx).unwrap_or(Flavor::Mocha)
        } else {
            desired.resolve(None, None)
        };
        // Poll only for visual preference changes. Never rerender print pages
        // just because the desktop theme switched.
        ctx.request_repaint_after(Duration::from_millis(800));
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(self.dialog_rx.is_none(), |ui| {
                if ui.button("Add images").clicked() { self.start_dialog(DialogKind::AddImages); }
                if ui.button("Add folder").clicked() { self.start_dialog(DialogKind::AddFolder); }
                if ui.button("Import digiKam templates").clicked() { self.start_dialog(DialogKind::ImportTemplates); }
                if ui.button("Save template XML").clicked() { self.start_dialog(DialogKind::ExportTemplate); }
                ui.separator();
                if ui.button("Load job").clicked() { self.start_dialog(DialogKind::LoadJob); }
                if ui.button("Save job").clicked() { self.start_dialog(DialogKind::SaveJob); }
            });
            if ui.button("Copy CLI command")
                .on_hover_text("Copy a Bash command that reproduces the current job, including all photo settings")
                .clicked() {
                match crate::cli_export::shell_command(&self.job) {
                    Ok(cmd) => { ui.ctx().copy_text(cmd); self.status = "CLI command copied to clipboard.".into(); }
                    Err(e) => self.status = format!("Cannot copy CLI command: {e:#}"),
                }
            }
            ui.separator();
            let action = match self.job.output.target {
                OutputTarget::Pdf => "Export PDF",
                OutputTarget::Images => "Export pages",
                OutputTarget::Gimp => "Export to GIMP",
                OutputTarget::Printer => "Print",
            };
            let colors = theme::palette(self.active_flavor);
            let action_text = egui::RichText::new(action).strong().color(
                if self.active_flavor.is_dark() { colors.crust } else { colors.text });
            if ui.add_enabled(self.pending.is_none(),
                egui::Button::new(action_text).fill(colors.sapphire)).clicked() {
                self.start_export();
            }
            if self.pending.is_some() || self.dialog_rx.is_some() { ui.spinner(); }
        });
    }

    fn settings(&mut self,ui:&mut egui::Ui) {
        section_heading(ui, "Page", theme::palette(self.active_flavor).sapphire);
        let known = ["A0", "A1", "A2", "A3", "A4", "A5", "A6", "Letter", "Legal",
            "Tabloid", "9x13", "10x15", "13x18", "20x25", "20.32x25.4"];
        ui.label("Paper preset");
        egui::ComboBox::from_id_salt("page_size")
            .selected_text(format!("{} — {:.2} × {:.2} cm", page_preset_name(self.job.page),
                self.job.page.width_mm / 10.0, self.job.page.height_mm / 10.0))
            .width(ui.available_width().max(120.0))
            .show_ui(ui, |ui| {
                for name in known {
                    if let Some(page) = Page::from_name(name) {
                        let selected = (page.width_mm - self.job.page.width_mm).abs() < 0.02
                            && (page.height_mm - self.job.page.height_mm).abs() < 0.02;
                        let display = format!("{} — {:.2} × {:.2} cm",
                            name.replace('x', " × "), page.width_mm / 10.0, page.height_mm / 10.0);
                        if ui.selectable_label(selected, display).clicked() { self.job.page = page; }
                    }
                }
            });
        cm_pair(ui, "Width", &mut self.job.page.width_mm,
            "Height", &mut self.job.page.height_mm, 0.5, 200.0);
        ui.horizontal(|ui| {
            if ui.button("Rotate paper").clicked() {
                std::mem::swap(&mut self.job.page.width_mm,&mut self.job.page.height_mm);
            }
            ui.label("DPI");ui.add(egui::DragValue::new(&mut self.job.dpi).range(36..=1200).speed(1.));
        });
        ui.horizontal(|ui| {ui.checkbox(&mut self.job.auto_rotate,"Auto-rotate photos");});
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.job.disable_crop, "Do not crop photos (fit entire image)")
                .on_hover_text("Like digiKam's global no-crop option: keep every photo fully visible. Empty frame space shows the paper background.");
        });
        ui.label("Paper color (hex RGB)");
        ui.text_edit_singleline(&mut self.job.background);
        ui.separator();

        section_heading(ui, "Photo layout", theme::palette(self.active_flavor).sapphire);
        let selected=match self.job.layout {Layout::Grid{..}=>"Grid",Layout::Adaptive{..}=>"Adaptive",Layout::Template{..}=>"digiKam template"};
        ui.label("Layout type");
        egui::ComboBox::from_id_salt("layout_mode")
            .selected_text(selected).width(ui.available_width().max(120.0)).show_ui(ui,|ui| {
            if ui.selectable_label(selected == "Grid","Grid").clicked() {
                if selected != "Grid" { self.job.layout=Layout::Grid{rows:2,columns:2,margin_mm:5.,gap_mm:3.}; }
            }
            if ui.selectable_label(selected == "Adaptive","Adaptive").clicked() {
                if selected != "Adaptive" { self.job.layout=Layout::Adaptive{photos_per_page:4,margin_mm:5.,gap_mm:3.}; }
            }
            if ui.selectable_label(selected == "digiKam template","digiKam template").clicked() {
                if selected != "digiKam template" {
                    if let Some(template)=self.templates.first() {template.apply(&mut self.job);}
                }
            }
        });
        match &mut self.job.layout {
            Layout::Grid {rows,columns,margin_mm,gap_mm} => {
                ui.horizontal(|ui|{ui.label("Rows");ui.add(egui::DragValue::new(rows).range(1..=50));ui.label("Columns");ui.add(egui::DragValue::new(columns).range(1..=50));});
                cm_pair(ui, "Margin", margin_mm, "Gap", gap_mm, 0.0, 20.0);
            }
            Layout::Adaptive {photos_per_page,margin_mm,gap_mm} => {
                ui.horizontal(|ui|{ui.label("Photos/page");ui.add(egui::DragValue::new(photos_per_page).range(1..=100));});
                cm_pair(ui, "Margin", margin_mm, "Gap", gap_mm, 0.0, 20.0);
            }
            Layout::Template {name, slots, auto_rotate} => {
                ui.checkbox(&mut self.job.borderless_templates, "Borderless template (remove outer margins and gaps)")
                    .on_hover_text("Expand template frames to the paper edges and remove the spacing between adjacent frames. Select Fill/crop for actual edge-to-edge photos. Does not override physical printer margins.");
                ui.label("Current template");
                egui::ComboBox::from_id_salt("template_choice")
                    .selected_text(if name.chars().count() > 32 {
                        format!("{}…", name.chars().take(31).collect::<String>())
                    } else { name.clone() })
                    .width(ui.available_width().max(120.0))
                    .show_ui(ui, |ui| {
                        ui.label("Filter by name, paper or dimensions:");
                        ui.text_edit_singleline(&mut self.template_filter);
                        let query = self.template_filter.trim().to_lowercase();
                        egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
                            let mut found = false;
                            for template in &self.templates {
                                let display = format!("{} — {:.2} × {:.2} cm ({} photos)",
                                    template.label, template.paper.width_mm/10.0,
                                    template.paper.height_mm/10.0, template.slots.len());
                                if !query.is_empty() && !display.to_lowercase().contains(&query)
                                    && !template.name.to_lowercase().contains(&query) { continue; }
                                found = true;
                                if ui.selectable_label(template.name == *name, display).clicked() {
                                    *name=template.name.clone(); *slots=template.slots.clone();
                                    *auto_rotate=template.auto_rotate;
                                    self.job.page=template.paper;
                                    if let Some(dpi)=template.dpi {self.job.dpi=dpi;}
                                }
                            }
                            if !found { ui.label("No matching templates"); }
                        });
                    });
                ui.checkbox(auto_rotate,"Template auto-rotate");
                ui.label(format!("{} photo frames",slots.len()));
                egui::CollapsingHeader::new("Edit individual frames (cm)").show(ui,|ui| {
                    for (i,s) in slots.iter_mut().enumerate() {
                        ui.label(format!("Frame #{}",i+1));
                        cm_pair(ui, "X", &mut s.x, "Y", &mut s.y, 0.0, 200.0);
                        cm_pair(ui, "Width", &mut s.width, "Height", &mut s.height, 0.01, 200.0);
                    }
                });
                if ui.button("Add frame").clicked() {
                    slots.push(crate::model::RectMm{x:5.,y:5.,width:40.,height:50.});
                }
                if ui.button("Remove last frame").clicked() && slots.len()>1 {slots.pop();}
            }
        }
        ui.separator();

        section_heading(ui, &format!("Photos ({})", self.job.photos.len()),
            theme::palette(self.active_flavor).sapphire);
        ui.small("Drop images or folders anywhere in the window to add them.");
        let mut action=None::<(usize,i32)>;
        let no_crop = self.job.disable_crop;
        for (i,photo) in self.job.photos.iter_mut().enumerate() {
            let filename = photo.path.file_name().unwrap_or_default().to_string_lossy();
            // Long camera filenames must not force the resizable sidebar wider.
            let compact_filename = if filename.chars().count() > 32 {
                format!("{}…", filename.chars().take(31).collect::<String>())
            } else { filename.into_owned() };
            egui::CollapsingHeader::new(format!("{}. {} (x{})", i+1, compact_filename, photo.copies))
                .id_salt(format!("photo-{i}")).show(ui,|ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.small_button("Up").clicked(){action=Some((i,-1));}
                        if ui.small_button("Down").clicked(){action=Some((i,1));}
                        if ui.small_button("Remove").clicked(){action=Some((i,0));}
                        ui.label("Copies");ui.add(egui::DragValue::new(&mut photo.copies).range(1..=10000));
                    });
                    ui.horizontal_wrapped(|ui| {
                        if ui.small_button("Rotate left").clicked() {photo.rotation-=90;}
                        if ui.small_button("Rotate right").clicked() {photo.rotation+=90;}
                        ui.label(format!("{}°",photo.rotation.rem_euclid(360)));
                    });
                    ui.horizontal(|ui| {ui.checkbox(&mut photo.flip_h,"Flip H");ui.checkbox(&mut photo.flip_v,"Flip V");});
                    ui.add_enabled_ui(!no_crop, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.selectable_value(&mut photo.crop.mode,CropMode::Fill,"Fill / crop");
                            ui.selectable_value(&mut photo.crop.mode,CropMode::Fit,"Fit / no crop");
                        });
                    });
                    if no_crop {
                        ui.small("Global no-crop mode is active; this photo will fit entirely.");
                    }
                    if photo.crop.mode==CropMode::Fill && !no_crop {
                        ui.add(egui::Slider::new(&mut photo.crop.zoom,1.0..=10.).text("Zoom"));
                        ui.add(egui::Slider::new(&mut photo.crop.focal_x,0.0..=1.).text("Crop X"));
                        ui.add(egui::Slider::new(&mut photo.crop.focal_y,0.0..=1.).text("Crop Y"));
                    }
                    let override_text=photo.caption_override.get_or_insert_with(String::new);
                    ui.label("Custom caption");
                    ui.add_sized([ui.available_width().max(80.0), 22.0],
                        egui::TextEdit::singleline(override_text).hint_text("Optional per-photo text"));
                    if override_text.is_empty(){photo.caption_override=None;}
                });
        }
        if let Some((index,direction))=action {
            match direction {
                0 => {self.job.photos.remove(index);},
                -1 if index>0 => self.job.photos.swap(index,index-1),
                1 if index+1<self.job.photos.len() => self.job.photos.swap(index,index+1),
                _ => {},
            }
        }
        ui.separator();

        section_heading(ui, "Captions", theme::palette(self.active_flavor).sapphire);
        egui::ComboBox::from_id_salt("caption_type")
            .selected_text(match self.job.caption.kind {
                CaptionType::None => "No captions", CaptionType::Filename => "Filename",
                CaptionType::DateTime => "EXIF date/time", CaptionType::Comment => "EXIF comment",
                CaptionType::Custom => "Custom text",
            }).width(ui.available_width().max(120.0))
            .show_ui(ui,|ui| {
                for (kind,label) in [
                    (CaptionType::None,"No captions"),(CaptionType::Filename,"Filename"),
                    (CaptionType::DateTime,"EXIF date/time"),(CaptionType::Comment,"EXIF comment"),
                    (CaptionType::Custom,"Custom placeholders")
                ] {ui.selectable_value(&mut self.job.caption.kind,kind,label);}
            });
        if self.job.caption.kind==CaptionType::Custom {
            ui.text_edit_singleline(&mut self.job.caption.custom);
            ui.small("{filename} {stem} {date} {comment} {width} {height} {index} {exif:Make} {exif:Model} ...");
        }
        ui.label("Font family");
        ui.add_sized([ui.available_width().max(80.0), 22.0],
            egui::TextEdit::singleline(&mut self.job.caption.font_family));
        ui.horizontal(|ui| {
            if ui.add_enabled(self.dialog_rx.is_none(), egui::Button::new("Choose font file")).clicked() {
                self.start_dialog(DialogKind::FontFile);
            }
            if let Some(path)=&self.job.caption.font_file {ui.small(path.file_name().unwrap_or_default().to_string_lossy().to_string());}
        });
        ui.horizontal_wrapped(|ui| {
            let mut cm = self.job.caption.size_pt * 2.54 / 72.0;
            ui.label("Font size (cm)");
            if ui.add(egui::DragValue::new(&mut cm).range(0.035..=5.08).speed(0.01)).changed() {
                self.job.caption.size_pt = cm * 72.0 / 2.54;
            }
            ui.label("Color");ui.text_edit_singleline(&mut self.job.caption.text_color);
        });
        ui.add(egui::Slider::new(&mut self.job.caption.backdrop_opacity,0..=255).text("Caption dark backdrop"));
        ui.separator();

        section_heading(ui, "Output", theme::palette(self.active_flavor).sapphire);
        ui.label("Destination type");
        let mut target_choice = self.job.output.target;
        egui::ComboBox::from_id_salt("output_target")
            .selected_text(output_target_name(target_choice))
            .width(ui.available_width().max(120.0))
            .show_ui(ui, |ui| {
                for target in [OutputTarget::Pdf, OutputTarget::Images,
                    OutputTarget::Gimp, OutputTarget::Printer] {
                    ui.selectable_value(&mut target_choice, target, output_target_name(target));
                }
            });
        if target_choice != self.job.output.target { self.set_output_target(target_choice); }

        match self.job.output.target {
            OutputTarget::Pdf | OutputTarget::Images | OutputTarget::Gimp => {
                let is_pdf = self.job.output.target == OutputTarget::Pdf;
                ui.label(if is_pdf { "PDF file" } else { "Output folder" });
                let mut typed = self.job.output.path.to_string_lossy().into_owned();
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 94.0).max(80.0);
                    let field = egui::TextEdit::singleline(&mut typed)
                        .hint_text(if is_pdf { "Choose a PDF filename" } else { "Choose a folder" });
                    if ui.add_sized([width, 24.0], field).changed() {
                        self.job.output.path = PathBuf::from(&typed);
                        if is_pdf { self.pdf_path = self.job.output.path.clone(); }
                        else { self.image_dir = self.job.output.path.clone(); }
                    }
                    if ui.add_enabled(self.dialog_rx.is_none(),
                        egui::Button::new("Browse...")).clicked() {
                        self.start_dialog(if is_pdf {DialogKind::OutputPdf} else {DialogKind::OutputFolder});
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.small_button("Use Documents").clicked() {
                        self.job.output.path = if is_pdf { paths::default_pdf_path() }
                            else { paths::default_images_dir() };
                        if is_pdf { self.pdf_path = self.job.output.path.clone(); }
                        else { self.image_dir = self.job.output.path.clone(); }
                    }
                    if is_pdf {
                        ui.small("Multipage PDF (one file)");
                    } else {
                        ui.small("One page-0001.* file per sheet");
                    }
                });
                if !self.job.output.path.as_os_str().is_empty() {
                    let path = &self.job.output.path;
                    let filename = path.file_name().unwrap_or_default().to_string_lossy();
                    let short_name = if filename.chars().count() > 36 {
                        format!("{}…", filename.chars().take(35).collect::<String>())
                    } else { filename.into_owned() };
                    ui.small(format!("Destination: {short_name}"))
                        .on_hover_text(path.display().to_string());
                } else {
                    ui.colored_label(theme::palette(self.active_flavor).peach,
                        "Choose a destination before exporting");
                }
            }
            OutputTarget::Printer => {
                ui.label("CUPS printer (blank = system default)");
                let printer = self.job.output.printer_name.get_or_insert_with(String::new);
                ui.add_sized([ui.available_width().max(80.0), 22.0],
                    egui::TextEdit::singleline(printer).hint_text("System default printer"));
                ui.small("Sends a temporary PDF to CUPS; no PDF file is saved.");
            }
        }

        if matches!(self.job.output.target, OutputTarget::Images | OutputTarget::Gimp) {
            ui.label("Page image format");
            egui::ComboBox::from_id_salt("image_format")
                .selected_text(image_format_name(self.job.output.image_format))
                .width(ui.available_width().max(120.0))
                .show_ui(ui, |ui| {
                    for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Tiff] {
                        ui.selectable_value(&mut self.job.output.image_format,
                            format, image_format_name(format));
                    }
                });
            if self.job.output.image_format != ImageFormat::Jpeg {
                ui.small("The chosen page image format uses lossless compression.");
            }
        }
        if matches!(self.job.output.target, OutputTarget::Pdf | OutputTarget::Printer) {
            ui.checkbox(&mut self.job.output.preserve_original_quality,
                "Preserve source image pixels (lossless PDF)")
                .on_hover_text("Embed the original pixel dimensions without scaling or JPEG recompression. Cropping is performed in PDF geometry, not on source pixels.");
            if self.job.output.preserve_original_quality {
                ui.small("Native photo resolution, but larger PDFs. Existing source artifacts and printer processing are unchanged.");
            }
        }
        let show_jpeg_quality = match self.job.output.target {
            OutputTarget::Pdf | OutputTarget::Printer => !self.job.output.preserve_original_quality,
            OutputTarget::Images | OutputTarget::Gimp => self.job.output.image_format == ImageFormat::Jpeg,
        };
        if show_jpeg_quality {
            ui.add(egui::Slider::new(&mut self.job.output.jpeg_quality, 1..=100)
                .text("JPEG quality"));
        }

        if self.job.output.target != OutputTarget::Printer {
            ui.label("If the destination already exists");
            egui::ComboBox::from_id_salt("file_conflicts")
                .selected_text(conflict_name(self.job.output.conflict))
                .width(ui.available_width().max(120.0))
                .show_ui(ui, |ui| {
                    for rule in [ConflictRule::Overwrite, ConflictRule::Skip,
                        ConflictRule::Rename, ConflictRule::Error] {
                        ui.selectable_value(&mut self.job.output.conflict, rule, conflict_name(rule));
                    }
                });
            ui.checkbox(&mut self.job.output.open_folder, "Open destination folder after export");
        }

    }
}

impl eframe::App for PrintCreatorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync_appearance(ctx);
        self.poll_export(ctx);
        self.poll_dialog(ctx);
        self.poll_import(ctx);
        self.handle_dropped_images(ctx);
        let colors = theme::palette(self.active_flavor);
        egui::TopBottomPanel::top("top-toolbar")
            .frame(egui::Frame::default().fill(colors.mantle))
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("PRINT CREATOR")
                        .strong().size(18.0).color(colors.sapphire));
                    ui.label(egui::RichText::new("Photo print layout studio")
                        .color(colors.subtext));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("appearance_choice")
                            .selected_text(self.appearance.label())
                            .width(155.0)
                            .show_ui(ui, |ui| {
                                for mode in [Appearance::System, Appearance::Dark, Appearance::Light] {
                                    ui.selectable_value(&mut self.appearance, mode, mode.label());
                                }
                            });
                        ui.label("Theme");
                        if ui.button("About").clicked() { self.show_about = true; }
                    });
                });
                ui.add_space(7.0);
                self.top_bar(ui);
                ui.add_space(8.0);
            });
        if self.show_about {
            egui::Window::new("About Print Creator")
                .open(&mut self.show_about)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.heading(crate::APP_NAME);
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.label(format!("Author: {}", env!("CARGO_PKG_AUTHORS")));
                    ui.label(format!("License: {}", env!("CARGO_PKG_LICENSE")));
                });
        }
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::default().fill(colors.crust))
            .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(&self.status);
                ui.separator();
                ui.label(egui::RichText::new(self.active_flavor.label())
                    .color(colors.sapphire));
                ui.separator();
                ui.label(format!("{} page(s) · {} images (with copies)",
                    self.job.page_count(), self.job.photos.iter().map(|p| p.copies as usize).sum::<usize>()));
                if self.preview_pending { ui.spinner(); ui.label("Updating preview..."); }
                if self.imports_pending > 0 { ui.spinner(); ui.label("Importing images..."); }
            });
        });
        // Reserve a substantial area for the preview even if the sidebar is
        // resized. Nested controls may not inflate the sidebar beyond this cap.
        let available = ctx.available_rect().width();
        let max_sidebar = (available - 430.0).max(300.0).min(560.0);
        let min_sidebar = 300.0_f32.min(max_sidebar);
        egui::SidePanel::left("settings-panel")
            .frame(egui::Frame::default().fill(colors.mantle))
            .resizable(true)
            .default_width(405.0)
            .width_range(min_sidebar..=max_sidebar)
            .show(ctx, |ui| {
                ui.set_min_width(0.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_width(ui.available_width().max(1.0));
                    self.settings(ui);
                });
            });
        self.queue_preview(ctx);
        self.poll_preview(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(colors.base))
            .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("Print preview");
                let n = self.job.page_count();
                if n > 0 {
                    if ui.add_enabled(self.preview_page > 0, egui::Button::new("Previous")).clicked() {
                        self.preview_page -= 1;
                    }
                    ui.label(format!("Page {} / {n}", self.preview_page + 1));
                    if ui.add_enabled(self.preview_page + 1 < n, egui::Button::new("Next")).clicked() {
                        self.preview_page += 1;
                    }
                }
            });
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!("Paper: {:.2} × {:.2} cm",
                    self.job.page.width_mm / 10.0, self.job.page.height_mm / 10.0));
                if let Some(area) = preview_layout_area(self.job.page, &self.job.layout) {
                    ui.separator();
                    ui.label(format!("Layout area: {:.2} × {:.2} cm",
                        area.width / 10.0, area.height / 10.0));
                } else if let Layout::Template { slots, .. } = &self.job.layout {
                    ui.separator();
                    ui.label(format!("{} template frame(s)", slots.len()));
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("■ Paper edge").color(colors.sapphire));
                ui.label(egui::RichText::new("■ Image frames").color(colors.peach));
                ui.label(egui::RichText::new("Preview guides are never printed").color(colors.subtext));
            });
            if let Some(image) = &self.preview {
                ui.add_space(8.0);
                let max = (ui.available_size() - egui::vec2(20.0, 20.0)).max(egui::vec2(1.0, 1.0));
                let source = image.size_vec2();
                let scale = (max.x / source.x).min(max.y / source.y).min(1.0);
                let desired = source * scale;
                ui.vertical_centered(|ui| {
                    let response = ui.add(egui::Image::new(image).fit_to_exact_size(desired));
                    self.draw_preview_guides(ui, response.rect);
                });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label(if self.preview_pending { "Rendering preview..." }
                        else { "Add photos or correct the page/layout settings to preview." });
                });
            }
        });
        self.show_drop_overlay(ctx);
    }
}

#[cfg(test)]
mod preview_guide_tests {
    use super::*;

    #[test]
    fn a4_page_and_grid_area_have_correct_physical_proportions() {
        let job = PrintJob::default(); // A4 with a 0.70 cm margin
        let paper = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(210.0, 297.0));
        let outer = preview_mm_rect(job.page, paper,
            crate::model::RectMm { x: 0.0, y: 0.0, width: 210.0, height: 297.0 }).unwrap();
        assert_eq!(outer, paper);
        let area = preview_layout_area(job.page, &job.layout).unwrap();
        assert_eq!((area.width, area.height), (196.0, 283.0));
        let drawn = preview_mm_rect(job.page, paper, area).unwrap();
        assert!((drawn.left() - 17.0).abs() < 0.001);
        assert!((drawn.top() - 27.0).abs() < 0.001);
        assert!((drawn.width() - 196.0).abs() < 0.001);
        assert!((drawn.height() - 283.0).abs() < 0.001);
    }

    #[test]
    fn drag_and_drop_uses_only_local_paths_and_preserves_order() {
        let files = vec![
            egui::DroppedFile { path: Some(PathBuf::from("/tmp/a.jpg")), ..Default::default() },
            egui::DroppedFile { path: None, name: "remote.png".into(), ..Default::default() },
            egui::DroppedFile { path: Some(PathBuf::from("/tmp/folder")), ..Default::default() },
        ];
        assert_eq!(local_dropped_paths(&files),
            vec![PathBuf::from("/tmp/a.jpg"), PathBuf::from("/tmp/folder")]);
    }

    #[test]
    fn template_uses_individual_frames_instead_of_a_global_margin() {
        let page = Page::from_name("a4").unwrap();
        let layout = Layout::Template {
            name: "custom".into(), auto_rotate: false,
            slots: vec![crate::model::RectMm { x: 10.0, y: 20.0, width: 35.0, height: 40.0 }],
        };
        assert!(preview_layout_area(page, &layout).is_none());
        let paper = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 594.0));
        let slot = if let Layout::Template { slots, .. } = layout { slots[0] } else { unreachable!() };
        let rect = preview_mm_rect(page, paper, slot).unwrap();
        assert_eq!((rect.left(), rect.top(), rect.width(), rect.height()), (20.0, 40.0, 70.0, 80.0));
    }
}
