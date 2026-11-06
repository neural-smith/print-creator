//! Reuses the desktop PDF writers with in-memory Cursor output.
use std::{io::Cursor, path::PathBuf};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use crate::{model::PrintJob, web_assets, render, native_pdf::NativePdfWriter, pdf::PdfWriter};

pub fn pdf_bytes(job: &PrintJob) -> Result<Vec<u8>> {
    job.validate()?;
    if job.page_count() > 100 { bail!("browser export is limited to 100 pages"); }
    let pages=job.page_count();
    let cursor=Cursor::new(Vec::new());
    if job.output.preserve_original_quality {
        let mut pdf=NativePdfWriter::new(cursor,pages,job.page.width_mm,job.page.height_mm)?;
        for n in 0..pages {
            pdf.add_page(job,n).with_context(||format!("generating page {}",n+1))?;
        }
        Ok(pdf.finish()?.into_inner())
    } else {
        let mut pdf=PdfWriter::new(cursor,pages,job.page.width_mm,job.page.height_mm)?;
        for n in 0..pages {
            let bitmap=render::render_page(job,n,job.dpi)
                .with_context(||format!("rendering page {}",n+1))?;
            pdf.add_page(&bitmap,job.output.jpeg_quality)?;
        }
        Ok(pdf.finish()?.into_inner())
    }
}

#[derive(Serialize, Deserialize)]
struct EmbeddedImage {
    filename: String,
    old_path: PathBuf,
    base64: String,
}

#[derive(Serialize, Deserialize)]
struct ProjectBundle {
    format: String,
    job: PrintJob,
    images: Vec<EmbeddedImage>,
}

pub fn save_project(job: &PrintJob) -> Result<Vec<u8>> {
    let mut images=Vec::new();
    for photo in &job.photos {
        if images.iter().any(|asset: &EmbeddedImage| asset.old_path==photo.path) { continue; }
        let data=web_assets::get(&photo.path)
            .ok_or_else(||anyhow::anyhow!("missing photo: {}",photo.path.display()))?;
        images.push(EmbeddedImage {
            filename: photo.path.file_name().unwrap_or_default().to_string_lossy().to_string(),
            old_path: photo.path.clone(),
            base64: STANDARD.encode(data),
        });
    }
    Ok(serde_json::to_vec(&ProjectBundle {format:"print-creator-web-v1".into(),job:job.clone(),images})?)
}

pub fn load_project(data: &[u8]) -> Result<PrintJob> {
    if data.len() > 500*1024*1024 { bail!("project file is too large"); }
    let bundle:ProjectBundle=serde_json::from_slice(data).context("invalid Print Creator project")?;
    if bundle.format != "print-creator-web-v1" { bail!("unsupported project format"); }
    if bundle.images.len()>10000 { bail!("project contains too many photos"); }
    let mut mapping=std::collections::BTreeMap::new();
    let mut newly_added=Vec::new();
    let result=(|| -> Result<PrintJob> {
        for image in bundle.images {
            let data=STANDARD.decode(image.base64).context("invalid embedded photo")?;
            let path=web_assets::insert(&image.filename,data)?;
            newly_added.push(path.clone());
            mapping.insert(image.old_path,path);
        }
        let mut job=bundle.job;
        for photo in &mut job.photos {
            photo.path=mapping.get(&photo.path).cloned()
                .ok_or_else(||anyhow::anyhow!("project is missing a referenced photo"))?;
        }
        job.validate()?;
        Ok(job)
    })();
    if result.is_err() {
        for path in newly_added {web_assets::remove(&path);}
    }
    result
}
