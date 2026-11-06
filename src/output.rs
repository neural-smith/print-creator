use std::{fs::{self,File}, io::BufWriter, path::{Path,PathBuf}, process::Command};
use anyhow::{bail, Context, Result};
use image::{codecs::jpeg::JpegEncoder, DynamicImage, ImageFormat as NativeFormat, RgbaImage};
use crate::{model::{ConflictRule,ImageFormat,OutputTarget,PrintJob},pdf::PdfWriter,native_pdf::NativePdfWriter,render};

fn with_conflict(path: &Path, rule: ConflictRule) -> Result<Option<PathBuf>> {
    if !path.exists() { return Ok(Some(path.to_owned())); }
    match rule {
        ConflictRule::Overwrite => Ok(Some(path.to_owned())),
        ConflictRule::Skip => Ok(None),
        ConflictRule::Error => bail!("file already exists: {}",path.display()),
        ConflictRule::Rename => {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            let extension = path.extension().unwrap_or_default().to_string_lossy();
            for i in 1..100_000 {
                let filename = if extension.is_empty() {format!("{stem}-{i}")} else {format!("{stem}-{i}.{extension}")};
                let new_path = path.with_file_name(filename);
                if !new_path.exists() { return Ok(Some(new_path)); }
            }
            bail!("unable to find a free output file name")
        }
    }
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) { fs::create_dir_all(dir)?; }
    Ok(())
}

fn write_image(img: &RgbaImage, path: &Path, format: ImageFormat, quality: u8) -> Result<()> {
    let file = File::create(path)?;
    match format {
        ImageFormat::Jpeg => {
            let mut out = BufWriter::new(file);
            JpegEncoder::new_with_quality(&mut out,quality).encode_image(&DynamicImage::ImageRgba8(img.clone()).to_rgb8())?;
        }
        ImageFormat::Png => DynamicImage::ImageRgba8(img.clone()).write_to(&mut std::io::BufWriter::new(file),NativeFormat::Png)?,
        ImageFormat::Tiff => DynamicImage::ImageRgba8(img.clone()).write_to(&mut std::io::BufWriter::new(file),NativeFormat::Tiff)?,
    }
    Ok(())
}

fn export_images(job: &PrintJob, folder: &Path) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(folder).with_context(||format!("creating {}", folder.display()))?;
    let mut paths = Vec::new();
    let ext = match job.output.image_format {
        ImageFormat::Jpeg => "jpg", ImageFormat::Png => "png", ImageFormat::Tiff => "tiff",
    };
    for page in 0..job.page_count() {
        let name = folder.join(format!("page-{:04}.{ext}",page+1));
        let Some(dest) = with_conflict(&name, job.output.conflict)? else { continue };
        let bitmap = render::render_page(job,page,job.dpi).with_context(||format!("rendering page {}",page+1))?;
        let temp = dest.with_extension(format!("{ext}.partial"));
        if let Err(e) = write_image(&bitmap,&temp,job.output.image_format,job.output.jpeg_quality) {
            let _ = fs::remove_file(&temp);
            return Err(e);
        }
        fs::rename(&temp,&dest)?;
        paths.push(dest);
    }
    Ok(paths)
}

fn export_pdf(job: &PrintJob, path: &Path) -> Result<Vec<PathBuf>> {
    let Some(dest) = with_conflict(path,job.output.conflict)? else { return Ok(Vec::new()) };
    ensure_parent(&dest)?;
    let temp = dest.with_extension("pdf.partial");
    let result = (|| -> Result<()> {
        let file = BufWriter::new(File::create(&temp)?);
        if job.output.preserve_original_quality {
            let mut pdf=NativePdfWriter::new(file,job.page_count(),job.page.width_mm,job.page.height_mm)?;
            for page in 0..job.page_count() {
                pdf.add_page(job,page).with_context(||format!("composing lossless PDF page {}",page+1))?;
            }
            pdf.finish()?;
        } else {
            let mut pdf=PdfWriter::new(file,job.page_count(),job.page.width_mm,job.page.height_mm)?;
            for page in 0..job.page_count() {
                let bitmap = render::render_page(job,page,job.dpi).with_context(||format!("rendering page {}",page+1))?;
                pdf.add_page(&bitmap,job.output.jpeg_quality)?;
            }
            pdf.finish()?;
        }
        Ok(())
    })();
    if let Err(e) = result { let _=fs::remove_file(&temp); return Err(e); }
    fs::rename(&temp,&dest)?;
    Ok(vec![dest])
}

fn command_success(program: &str, args: &[&str], paths: &[PathBuf]) -> Result<()> {
    let result = Command::new(program).args(args).args(paths).status()
        .with_context(|| format!("cannot execute '{}'; install it or choose another output target",program))?;
    if !result.success() { bail!("'{program}' exited with status {result}"); }
    Ok(())
}

pub fn export(job: &PrintJob) -> Result<Vec<PathBuf>> {
    job.validate()?;
    let outputs=match job.output.target {
        OutputTarget::Pdf => export_pdf(job,&job.output.path)?,
        OutputTarget::Images => export_images(job,&job.output.path)?,
        OutputTarget::Gimp => {
            let paths=export_images(job,&job.output.path)?;
            if !paths.is_empty() {
                Command::new("gimp").args(&paths).spawn()
                    .context("GIMP not found; pages were exported, but could not be opened")?;
            }
            paths
        }
        OutputTarget::Printer => {
            let pdf_path=std::env::temp_dir().join(format!("print-creator-{}.pdf",std::process::id()));
            let mut print_job=job.clone();
            print_job.output.target=OutputTarget::Pdf;
            print_job.output.path=pdf_path;
            print_job.output.conflict=ConflictRule::Overwrite;
            let paths=export_pdf(&print_job,&print_job.output.path)?;
            let result = if let Some(name) = job.output.printer_name.as_deref().filter(|s| !s.is_empty()) {
                command_success("lp", &["-d",name],&paths)
            } else { command_success("lp",&[],&paths) };
            for file in &paths { let _=fs::remove_file(file); }
            result?;
            Vec::new()
        }
    };
    if job.output.open_folder && !outputs.is_empty() {
        let folder = if outputs[0].is_dir() {outputs[0].clone()} else {outputs[0].parent().unwrap_or(Path::new(".")).to_path_buf()};
        #[cfg(target_os="linux")]
        { let _=Command::new("xdg-open").arg(folder).spawn(); }
        #[cfg(target_os="macos")]
        { let _=Command::new("open").arg(folder).spawn(); }
        #[cfg(target_os="windows")]
        { let _=Command::new("explorer").arg(folder).spawn(); }
    }
    Ok(outputs)
}
