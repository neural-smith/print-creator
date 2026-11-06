use print_creator::{model::{ConflictRule, ImageFormat, Layout, OutputTarget, Page, Photo, PrintJob}, output, template};

fn sample_job(folder: &std::path::Path) -> PrintJob {
    let photo_path=folder.join("photo.png");
    let mut image=image::RgbImage::new(64,48);
    for (x,y,p) in image.enumerate_pixels_mut() {
        *p=if x<32 {image::Rgb([255,0,0])} else {image::Rgb([0,y as u8,255])};
    }
    image.save(&photo_path).unwrap();
    let mut job=PrintJob::default();
    job.page=Page{width_mm:60.,height_mm:40.};
    job.layout=Layout::Grid{rows:1,columns:1,margin_mm:0.,gap_mm:0.};
    job.photos=vec![Photo::from_path(photo_path)];
    job.dpi=72;
    job.output.path=folder.join("export.pdf");
    job
}

#[test]
fn generates_pdf_and_png() {
    let tmp=tempfile::tempdir().unwrap();
    let mut job=sample_job(tmp.path());
    let results=output::export(&job).unwrap();
    let pdf=std::fs::read(&results[0]).unwrap();
    assert!(pdf.starts_with(b"%PDF-1.4"));
    assert!(pdf.windows(15).any(|b| b==b"/Subtype /Image"));

    job.output.path=tmp.path().join("pages");
    job.output.target=OutputTarget::Images;
    job.output.image_format=ImageFormat::Png;
    let results=output::export(&job).unwrap();
    assert_eq!(results.len(),1);
    let (w,h)=image::image_dimensions(&results[0]).unwrap();
    assert_eq!((w,h),job.page.pixels(72).unwrap());

    job.output.conflict=ConflictRule::Rename;
    let renamed=output::export(&job).unwrap();
    assert_ne!(results[0],renamed[0]);
}

#[test]
fn custom_template_round_trip() {
    let tmp=tempfile::tempdir().unwrap();
    let job=sample_job(tmp.path());
    let xml=tmp.path().join("custom.xml");
    template::write_compatible_template(&job,"A layout & name",&xml).unwrap();
    assert!(std::fs::read_to_string(&xml).unwrap().contains("unit=\"cm\""));
    let templates=template::load_xml(&xml).unwrap();
    assert_eq!(templates.len(),1);
    assert_eq!(templates[0].slots.len(),1);
    assert_eq!(templates[0].paper.width_mm,60.);
}

#[test]
fn disabling_crop_shows_entire_photo_in_pdf_rendering_pipeline() {
    use print_creator::{model::CropMode, render};
    let tmp = tempfile::tempdir().unwrap();
    let mut job = sample_job(tmp.path());
    // 2:1 red photo inside a 1:1 square frame.
    let red = image::RgbImage::from_pixel(160, 80, image::Rgb([255, 0, 0]));
    red.save(&job.photos[0].path).unwrap();
    job.page = Page { width_mm: 40., height_mm: 40. };
    job.photos[0].crop.mode = CropMode::Fill;
    job.auto_rotate = false;
    let cropped = render::render_page(&job, 0, 72).unwrap();
    assert_eq!(cropped.get_pixel(10, 10).0, [255, 0, 0, 255]);
    job.disable_crop = true;
    let fitted = render::render_page(&job, 0, 72).unwrap();
    // Top of the frame remains white; image is letterboxed, not cropped.
    assert_eq!(fitted.get_pixel(10, 10).0, [255, 255, 255, 255]);
    assert_eq!(fitted.get_pixel(10, fitted.height()/2).0, [255, 0, 0, 255]);
}

#[test]
fn template_borderless_removes_baked_in_white_margins() {
    use print_creator::{model::{CropMode, RectMm}, render};
    let tmp=tempfile::tempdir().unwrap();
    let mut job=sample_job(tmp.path());
    job.layout=Layout::Template {name:"inset".into(),
        slots:vec![RectMm{x:4.,y:5.,width:52.,height:30.}],auto_rotate:false};
    job.auto_rotate=false;
    job.photos[0].crop.mode=CropMode::Fill;
    let old=render::render_page(&job,0,72).unwrap();
    assert_eq!(old.get_pixel(0,0).0,[255,255,255,255]);
    job.borderless_templates=true;
    let new=render::render_page(&job,0,72).unwrap();
    assert_ne!(new.get_pixel(0,0).0,[255,255,255,255]);
    assert_ne!(new.get_pixel(new.width()-1,new.height()-1).0,[255,255,255,255]);
}

#[test]
fn lossless_pdf_uses_native_photo_dimensions_without_jpeg() {
    let tmp=tempfile::tempdir().unwrap();
    let mut job=sample_job(tmp.path());
    job.output.preserve_original_quality=true;
    job.dpi=600; // Changing print resolution must not resample the source.
    let paths=output::export(&job).unwrap();
    let pdf=std::fs::read(&paths[0]).unwrap();
    assert!(pdf.windows(b"/Width 64 /Height 48".len())
        .any(|window|window==b"/Width 64 /Height 48"));
    assert!(pdf.windows(b"/Filter /FlateDecode".len())
        .any(|window|window==b"/Filter /FlateDecode"));
    assert!(!pdf.windows(b"/DCTDecode".len())
        .any(|window|window==b"/DCTDecode"));
}

#[test]
fn new_options_are_backwards_compatible() {
    let job=PrintJob::default();
    let mut value=serde_json::to_value(job).unwrap();
    value.as_object_mut().unwrap().remove("borderless_templates");
    value.get_mut("output").unwrap().as_object_mut().unwrap().remove("preserve_original_quality");
    let restored: PrintJob=serde_json::from_value(value).unwrap();
    assert!(!restored.borderless_templates);
    assert!(!restored.output.preserve_original_quality);
}

#[test]
fn render_cli_accepts_full_job_from_stdin() {
    use std::{io::Write, process::{Command,Stdio}};
    let tmp=tempfile::tempdir().unwrap();
    let job=sample_job(tmp.path());
    let json=serde_json::to_vec(&job).unwrap();
    let mut child=Command::new(env!("CARGO_BIN_EXE_print-creator"))
        .args(["render", "--job", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn().unwrap();
    child.stdin.take().unwrap().write_all(&json).unwrap();
    let result=child.wait_with_output().unwrap();
    assert!(result.status.success(), "{}",String::from_utf8_lossy(&result.stderr));
    assert!(std::fs::read(job.output.path).unwrap().starts_with(b"%PDF-1.4"));
}
