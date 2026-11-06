//! Browser entry point. Same PrintJob, XML templates, image rendering and PDF
//! writers as desktop; only input/output/UI event handling is browser-specific.
use std::{cell::RefCell, path::PathBuf};
use eframe::egui;
use wasm_bindgen::{prelude::*, JsCast};
use crate::{layout, model::{CaptionType, CropMode, Layout, Page, Photo, PrintJob},
    render, template::{self, TemplateDef}, theme::{self, Appearance, Flavor}, web_assets, web_output};

#[derive(Debug)]
enum WebEvent { Image(PathBuf), Project(Vec<u8>), Templates(String) }

thread_local! {
    static EVENTS: RefCell<Vec<WebEvent>> = const { RefCell::new(Vec::new()) };
    static CTX: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
    static RUNNER: RefCell<Option<eframe::WebRunner>> = const { RefCell::new(None) };
}

fn repaint() {
    CTX.with(|ctx| { if let Some(ctx)=ctx.borrow().as_ref() {ctx.request_repaint();} });
}

#[wasm_bindgen]
pub fn import_image(name: String, data: Vec<u8>) -> Result<(), JsValue> {
    let path=web_assets::insert(&name,data).map_err(|e|JsValue::from_str(&format!("{e:#}")))?;
    EVENTS.with(|events| events.borrow_mut().push(WebEvent::Image(path)));
    repaint();
    Ok(())
}

#[wasm_bindgen]
pub fn import_project(data: Vec<u8>) {
    EVENTS.with(|events| events.borrow_mut().push(WebEvent::Project(data)));
    repaint();
}

#[wasm_bindgen]
pub fn import_template_xml(xml: String) {
    EVENTS.with(|events| events.borrow_mut().push(WebEvent::Templates(xml)));
    repaint();
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace=window, js_name=pcOpenPhotos)]
    fn open_photos();
    #[wasm_bindgen(js_namespace=window, js_name=pcOpenProject)]
    fn open_project();
    #[wasm_bindgen(js_namespace=window, js_name=pcOpenTemplates)]
    fn open_templates();
    #[wasm_bindgen(js_namespace=window, js_name=pcDownload)]
    fn download_file(name:&str, mime:&str, bytes:&[u8]);
}

#[wasm_bindgen(start)]
pub fn boot() {
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::spawn_local(async {
        let Some(canvas)=web_sys::window().and_then(|w|w.document())
            .and_then(|doc|doc.get_element_by_id("print-creator-canvas"))
            .and_then(|node|node.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        else {return;};
        let runner=eframe::WebRunner::new();
        RUNNER.with(|stored|*stored.borrow_mut()=Some(runner.clone()));
        let result=runner.start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(|creation| {
                theme::install(&creation.egui_ctx);
                CTX.with(|slot| *slot.borrow_mut()=Some(creation.egui_ctx.clone()));
                Ok(Box::new(WebApp::default()))
            }),
        ).await;
        if let Err(error)=result {
            web_sys::window().and_then(|w|w.document())
                .and_then(|doc|doc.get_element_by_id("startup-status"))
                .map(|node|node.set_text_content(Some(&format!("Unable to start Print Creator: {error:?}"))));
        } else {
            web_sys::window().and_then(|w|w.document())
                .and_then(|doc|doc.get_element_by_id("startup-status"))
                .map(|node|node.remove());
        }
    });
}

struct WebApp {
    job: PrintJob,
    templates: Vec<TemplateDef>,
    template_name: String,
    chosen_photo: usize,
    preview_page: usize,
    preview: Option<egui::TextureHandle>,
    preview_key: String,
    status: String,
    appearance: Appearance,
    page_choice: String,
}

impl Default for WebApp {
    fn default() -> Self {
        let mut job=PrintJob::default();
        job.output.path=PathBuf::from("print-layout.pdf");
        job.dpi=150; // Browser raster exports remain adjustable up to 1200 DPI.
        job.output.preserve_original_quality=true;
        Self {job, templates:template::builtin(), template_name:String::new(),
            chosen_photo:0, preview_page:0, preview:None, preview_key:String::new(),
            status:"Import photographs or drag them onto the page.".into(),
            appearance:Appearance::System, page_choice:"A4".into()}
    }
}

fn centimeters(ui:&mut egui::Ui, name:&str, mm:&mut f32, max:f32) -> bool {
    ui.horizontal(|ui| {
        ui.label(name);
        let mut cm=*mm/10.0;
        let changed=ui.add(egui::DragValue::new(&mut cm).speed(0.05).range(0.0..=max).suffix(" cm")).changed();
        if changed { *mm=cm*10.0; }
        changed
    }).inner
}

impl WebApp {
    fn read_events(&mut self) {
        for event in EVENTS.with(|events|std::mem::take(&mut *events.borrow_mut())) {
            match event {
                WebEvent::Image(path) => {
                    self.job.photos.push(Photo::from_path(path));
                    self.chosen_photo=self.job.photos.len()-1;
                    self.status=format!("{} photograph(s) loaded locally",self.job.photos.len());
                }
                WebEvent::Templates(xml) => match template::parse_xml(&xml) {
                    Ok(mut imports) => {
                        let count=imports.len();
                        self.templates.append(&mut imports);
                        self.status=format!("Imported {count} digiKam template(s)");
                    }
                    Err(error) => self.status=format!("XML template import: {error:#}"),
                },
                WebEvent::Project(data) => match web_output::load_project(&data) {
                    Ok(job) => {
                        for path in web_assets::paths() {
                            if !job.photos.iter().any(|photo|photo.path==path) {
                                web_assets::remove(&path);
                            }
                        }
                        self.job=job;
                        self.chosen_photo=0;
                        self.preview_page=0;
                        self.template_name=String::new();
                        self.page_choice="Custom".into();
                        self.status="Project restored with its embedded photos".into();
                    }
                    Err(error) => self.status=format!("Project import: {error:#}"),
                },
            }
        }
    }

    fn save_pdf(&mut self) {
        self.status="Generating PDF in this browser tab...".into();
        match web_output::pdf_bytes(&self.job) {
            Ok(bytes) => {
                let name=self.job.output.path.file_name().and_then(|s|s.to_str())
                    .filter(|s| !s.is_empty()).unwrap_or("print-layout.pdf").to_owned();
                let name=if name.to_ascii_lowercase().ends_with(".pdf") {name} else {format!("{name}.pdf")};
                download_file(&name,"application/pdf",&bytes);
                self.status=format!("PDF created: {} pages, {:.1} MiB",self.job.page_count(),bytes.len() as f64/1048576.0);
            },
            Err(error) => self.status=format!("Export failed: {error:#}"),
        }
    }

    fn save_project(&mut self) {
        match web_output::save_project(&self.job) {
            Ok(bytes) => {
                download_file("print-creator-project.json","application/json",&bytes);
                self.status="Self-contained project downloaded".into();
            },
            Err(error) => self.status=format!("Cannot save project: {error:#}"),
        }
    }

    fn redraw_preview(&mut self,ctx:&egui::Context) {
        let mut no_output=self.job.clone();
        no_output.output.path=PathBuf::new();
        let key=serde_json::to_string(&(no_output.page,&no_output.layout,&no_output.photos,
            no_output.auto_rotate,no_output.disable_crop,no_output.borderless_templates,
            &no_output.caption,&no_output.background,self.preview_page)).unwrap_or_default();
        if self.preview_key==key {return;}
        self.preview_key=key;
        self.preview=None;
        if self.job.photos.is_empty(){return;}
        self.preview_page=self.preview_page.min(self.job.page_count().saturating_sub(1));
        match render::render_page(&self.job,self.preview_page,55) {
            Ok(bitmap) => {
                let (w,h)=bitmap.dimensions();
                let image=egui::ColorImage::from_rgba_unmultiplied([w as usize,h as usize],bitmap.as_raw());
                self.preview=Some(ctx.load_texture("browser-preview",image,egui::TextureOptions::LINEAR));
            }
            Err(error) => self.status=format!("Preview: {error:#}"),
        }
    }

    fn settings(&mut self, ui:&mut egui::Ui) {
        ui.heading("Paper & layout");
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("paper").selected_text(&self.page_choice).show_ui(ui,|ui| {
                for name in ["A3","A4","A5","A6","Letter","Legal","10x15","13x18","Custom"] {
                    ui.selectable_value(&mut self.page_choice,name.to_string(),name);
                }
            });
            if self.page_choice!="Custom" {
                if let Some(size)=Page::from_name(&self.page_choice) {
                    let current_landscape=self.job.page.width_mm>self.job.page.height_mm;
                    let mut candidate=size;
                    if current_landscape {std::mem::swap(&mut candidate.width_mm,&mut candidate.height_mm);}
                    if !matches!(&self.job.layout,Layout::Template{..}) {self.job.page=candidate;}
                }
            }
            let mut landscape=self.job.page.width_mm>self.job.page.height_mm;
            if ui.checkbox(&mut landscape,"Landscape").changed() {
                std::mem::swap(&mut self.job.page.width_mm,&mut self.job.page.height_mm);
                self.page_choice="Custom".into();
            }
        });
        if centimeters(ui,"Width",&mut self.job.page.width_mm,200.0) {self.page_choice="Custom".into();}
        if centimeters(ui,"Height",&mut self.job.page.height_mm,200.0) {self.page_choice="Custom".into();}
        ui.separator();
        let mut kind=match &self.job.layout {Layout::Grid{..}=>0,Layout::Adaptive{..}=>1,Layout::Template{..}=>2};
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut kind,0,"Grid");
            ui.selectable_value(&mut kind,1,"Adaptive");
            ui.selectable_value(&mut kind,2,"digiKam presets");
        });
        if kind==0 && !matches!(&self.job.layout,Layout::Grid{..}) {
            self.job.layout=Layout::Grid{rows:2,columns:2,margin_mm:5.0,gap_mm:3.0};
        } else if kind==1 && !matches!(&self.job.layout,Layout::Adaptive{..}) {
            self.job.layout=Layout::Adaptive{photos_per_page:4,margin_mm:5.0,gap_mm:3.0};
        } else if kind==2 && !matches!(&self.job.layout,Layout::Template{..}) {
            if let Some(t)=self.templates.first() {t.apply(&mut self.job);self.template_name=t.name.clone();}
        }
        let mut apply_template = None;
        match &mut self.job.layout {
            Layout::Grid{rows,columns,margin_mm,gap_mm} => {
                ui.horizontal(|ui| {
                    ui.label("Rows");ui.add(egui::DragValue::new(rows).range(1..=20));
                    ui.label("Columns");ui.add(egui::DragValue::new(columns).range(1..=20));
                });
                centimeters(ui,"Margin",margin_mm,20.0);
                centimeters(ui,"Gap",gap_mm,20.0);
            },
            Layout::Adaptive{photos_per_page,margin_mm,gap_mm} => {
                ui.horizontal(|ui| {ui.label("Photos per page");ui.add(egui::DragValue::new(photos_per_page).range(1..=100));});
                centimeters(ui,"Margin",margin_mm,20.0);
                centimeters(ui,"Gap",gap_mm,20.0);
            },
            Layout::Template{name,..} => {
                if ui.button("Import custom digiKam XML templates").clicked() {open_templates();}
                egui::ComboBox::from_id_salt("web-templates")
                    .selected_text(self.templates.iter().find(|t|t.name==self.template_name)
                        .map(|t|t.label.as_str()).unwrap_or("Choose a template"))
                    .width(ui.available_width().max(80.0))
                    .show_ui(ui,|ui| {
                        egui::ScrollArea::vertical().max_height(270.0).show(ui,|ui| {
                            for tmpl in &self.templates {
                                ui.selectable_value(&mut self.template_name,tmpl.name.clone(),&tmpl.label);
                            }
                        });
                    });
                if let Some(t)=self.templates.iter().find(|t|t.name==self.template_name) {
                    if name != &t.name { apply_template=Some(t.clone()); }
                }
                ui.checkbox(&mut self.job.borderless_templates,"Remove preset borders and gaps");
            }
        }
        if let Some(t)=apply_template { t.apply(&mut self.job); }
        ui.separator();
        ui.heading("Photos");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Add photos").clicked() {open_photos();}
            if ui.button("Clear all").clicked() {
                self.job.photos.clear();self.chosen_photo=0;web_assets::clear();
            }
        });
        ui.label(format!("{} imported · {} pages",self.job.photos.len(),self.job.page_count()));
        egui::ScrollArea::vertical().max_height(140.0).show(ui,|ui| {
            for (n,photo) in self.job.photos.iter().enumerate() {
                let label=photo.path.file_name().unwrap_or_default().to_string_lossy();
                ui.selectable_value(&mut self.chosen_photo,n,format!("{} · {}",n+1,label));
            }
        });
        if !self.job.photos.is_empty() {
            let selected=self.chosen_photo.min(self.job.photos.len()-1);
            let mut delete=false;
            if let Some(photo)=self.job.photos.get_mut(selected) {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Copies");ui.add(egui::DragValue::new(&mut photo.copies).range(1..=100));
                    if ui.button("Remove").clicked(){delete=true;}
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Rotation");
                    egui::ComboBox::from_id_salt("rotation").selected_text(format!("{}°",photo.rotation)).show_ui(ui,|ui| {
                        for deg in [0,90,180,270] {ui.selectable_value(&mut photo.rotation,deg,format!("{deg}°"));}
                    });
                    ui.checkbox(&mut photo.flip_h,"Flip H");
                    ui.checkbox(&mut photo.flip_v,"Flip V");
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label("Image placement");
                    ui.selectable_value(&mut photo.crop.mode,CropMode::Fill,"Fill / crop");
                    ui.selectable_value(&mut photo.crop.mode,CropMode::Fit,"Fit / no crop");
                });
                if photo.crop.mode==CropMode::Fill {
                    ui.add(egui::Slider::new(&mut photo.crop.zoom,1.0..=10.0).text("Zoom"));
                    ui.add(egui::Slider::new(&mut photo.crop.focal_x,0.0..=1.0).text("Horizontal focus"));
                    ui.add(egui::Slider::new(&mut photo.crop.focal_y,0.0..=1.0).text("Vertical focus"));
                }
                let caption=photo.caption_override.get_or_insert_with(String::new);
                ui.horizontal(|ui|{ui.label("Photo caption");ui.text_edit_singleline(caption);});
                if caption.is_empty(){photo.caption_override=None;}
            }
            if delete {
                let path=self.job.photos.remove(selected).path;
                if !self.job.photos.iter().any(|photo|photo.path==path){web_assets::remove(&path);}
                self.chosen_photo=self.chosen_photo.min(self.job.photos.len().saturating_sub(1));
            }
        }
        ui.separator();
        ui.heading("Print options");
        ui.checkbox(&mut self.job.auto_rotate,"Match longer image edge automatically");
        ui.checkbox(&mut self.job.disable_crop,"Do not crop any photo");
        ui.horizontal_wrapped(|ui| {
            ui.label("Caption");
            egui::ComboBox::from_id_salt("captions").selected_text(format!("{:?}",self.job.caption.kind)).show_ui(ui,|ui| {
                for (value,label) in [(CaptionType::None,"None"),(CaptionType::Filename,"Filename"),
                    (CaptionType::DateTime,"EXIF date"),(CaptionType::Comment,"EXIF comment"),(CaptionType::Custom,"Custom")] {
                    ui.selectable_value(&mut self.job.caption.kind,value,label);
                }
            });
        });
        if self.job.caption.kind==CaptionType::Custom {ui.text_edit_singleline(&mut self.job.caption.custom);}
        if self.job.caption.kind!=CaptionType::None {
            ui.add(egui::Slider::new(&mut self.job.caption.size_pt,6.0..=40.0).text("Caption size (pt)"));
            ui.small("Web caption text uses a compact bitmap font; desktop supports system fonts.");
        }
        ui.separator();
        ui.heading("PDF export");
        ui.checkbox(&mut self.job.output.preserve_original_quality,"Preserve source pixels (lossless PDF)");
        if !self.job.output.preserve_original_quality {
            ui.add(egui::Slider::new(&mut self.job.dpi,72..=600).text("DPI"));
            ui.add(egui::Slider::new(&mut self.job.output.jpeg_quality,50..=100).text("JPEG quality"));
        }
        ui.horizontal(|ui| {
            ui.label("Filename");
            let mut path=self.job.output.path.to_string_lossy().into_owned();
            if ui.text_edit_singleline(&mut path).changed() {
                self.job.output.path=PathBuf::from(path.replace('/', "_").replace('\\', "_"));
            }
        });
        ui.small("Photos and PDFs stay in your browser; no server upload is used.");
    }

    fn preview(&self,ui:&mut egui::Ui,colors:theme::Palette) {
        ui.horizontal_wrapped(|ui|{
            ui.heading("Print preview");
            ui.label(format!("{:.2} × {:.2} cm",self.job.page.width_mm/10.0,self.job.page.height_mm/10.0));
        });
        ui.separator();
        if let Some(texture)=&self.preview {
            let available=(ui.available_size()-egui::vec2(14.0,28.0)).max(egui::vec2(40.0,40.0));
            let source=texture.size_vec2();
            let scale=(available.x/source.x).min(available.y/source.y).min(1.0);
            let size=source*scale;
            ui.vertical_centered(|ui|{
                let resp=ui.add(egui::Image::new(texture).fit_to_exact_size(size));
                let paper=resp.rect;
                ui.painter().rect_stroke(paper,0.0,egui::Stroke::new(2.0_f32,colors.sapphire),egui::StrokeKind::Inside);
                let ids=self.job.expanded_photo_indices();
                let capacity=self.job.capacity().unwrap_or(0);
                let from=self.preview_page.saturating_mul(capacity);
                let chosen=ids.iter().skip(from).take(capacity)
                    .filter_map(|i|self.job.photos.get(*i)).collect::<Vec<_>>();
                if let Ok(frames)=layout::job_slots(&self.job,&chosen) {
                    for frame in frames {
                        let rect=egui::Rect::from_min_size(
                            paper.min+egui::vec2(frame.x/self.job.page.width_mm*paper.width(),
                                frame.y/self.job.page.height_mm*paper.height()),
                            egui::vec2(frame.width/self.job.page.width_mm*paper.width(),
                                frame.height/self.job.page.height_mm*paper.height()));
                        ui.painter().rect_stroke(rect,0.0,egui::Stroke::new(0.9_f32,colors.peach),egui::StrokeKind::Inside);
                    }
                }
            });
        } else {
            ui.centered_and_justified(|ui|ui.label("Import photos to preview the printable page."));
        }
    }
}

impl eframe::App for WebApp {
    fn update(&mut self,ctx:&egui::Context,_frame:&mut eframe::Frame) {
        self.read_events();
        let scheme=match self.appearance {
            Appearance::System=>egui::ThemePreference::System,
            Appearance::Dark=>egui::ThemePreference::Dark,
            Appearance::Light=>egui::ThemePreference::Light,
        };
        ctx.set_theme(scheme);
        let flavor=self.appearance.resolve(None,theme::native_flavor(ctx));
        let colors=theme::palette(flavor);
        egui::TopBottomPanel::top("web-toolbar").frame(egui::Frame::default().fill(colors.mantle))
            .show(ctx,|ui|{
                ui.horizontal_wrapped(|ui|{
                    ui.strong(egui::RichText::new("Print Creator").color(colors.sapphire));
                    ui.separator();
                    if ui.button("Add photos").clicked(){open_photos();}
                    if ui.button("Open project").clicked(){open_project();}
                    if ui.button("Save project").clicked(){self.save_project();}
                    if ui.add_enabled(!self.job.photos.is_empty(),egui::Button::new("Download PDF")).clicked(){self.save_pdf();}
                    ui.separator();
                    egui::ComboBox::from_id_salt("web-appearance").selected_text(self.appearance.label()).show_ui(ui,|ui|{
                        for mode in [Appearance::System,Appearance::Dark,Appearance::Light] {
                            ui.selectable_value(&mut self.appearance,mode,mode.label());
                        }
                    });
                });
            });
        egui::TopBottomPanel::bottom("web-status").show(ctx,|ui|{
            ui.horizontal_wrapped(|ui|{
                ui.label(&self.status);
                ui.separator();ui.label("Private · local browser processing");
            });
        });
        let width=ctx.available_rect().width();
        let max_sidebar=(width-280.0).max(240.0).min(460.0);
        egui::SidePanel::left("web-settings").resizable(true)
            .default_width(350.0).width_range(240.0_f32.min(max_sidebar)..=max_sidebar)
            .frame(egui::Frame::default().fill(colors.mantle)).show(ctx,|ui|{
                egui::ScrollArea::vertical().auto_shrink([false,false]).show(ui,|ui|{
                    ui.set_min_width(0.0);
                    self.settings(ui);
                });
            });
        self.redraw_preview(ctx);
        egui::CentralPanel::default().frame(egui::Frame::default().fill(colors.base)).show(ctx,|ui|{
            ui.horizontal_wrapped(|ui|{
                let count=self.job.page_count();
                if count>0 {
                    if ui.add_enabled(self.preview_page>0,egui::Button::new("◀ Previous")).clicked(){self.preview_page-=1;}
                    ui.label(format!("Page {} / {}",self.preview_page+1,count));
                    if ui.add_enabled(self.preview_page+1<count,egui::Button::new("Next ▶")).clicked(){self.preview_page+=1;}
                }
            });
            self.preview(ui,colors);
        });
    }
}
