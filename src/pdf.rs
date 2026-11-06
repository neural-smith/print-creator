//! Minimal, standards-based PDF 1.4 writer: one high-resolution JPEG XObject/page.
//! Entire pages are raster images, matching the print-layout preview pixel-for-pixel.
use std::io::{Seek, SeekFrom, Write};
use anyhow::{bail, Result};
use image::{codecs::jpeg::JpegEncoder, RgbaImage};

pub struct PdfWriter<W: Write + Seek> {
    output: W,
    offsets: Vec<u64>,
    pages: usize,
    width_pt: f64,
    height_pt: f64,
    current: usize,
}

impl<W: Write + Seek> PdfWriter<W> {
    pub fn new(mut output: W, pages: usize, width_mm: f32, height_mm: f32) -> Result<Self> {
        if pages == 0 { bail!("cannot write an empty PDF"); }
        output.write_all(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n")?;
        let mut this = Self {
            output, offsets: vec![0; 3+pages*3], pages,
            width_pt: width_mm as f64 * 72. / 25.4,
            height_pt: height_mm as f64 * 72. / 25.4,
            current: 0,
        };
        this.start_obj(1)?;
        this.output.write_all(b"<< /Type /Catalog /Pages 2 0 R >>\nendobj\n")?;
        this.start_obj(2)?;
        write!(this.output, "<< /Type /Pages /Count {} /Kids [", pages)?;
        for i in 0..pages { write!(this.output," {} 0 R",3+i*3)?; }
        this.output.write_all(b" ] >>\nendobj\n")?;
        Ok(this)
    }

    fn start_obj(&mut self, n: usize) -> Result<()> {
        self.offsets[n] = self.output.stream_position()?;
        write!(self.output, "{n} 0 obj\n")?;
        Ok(())
    }

    pub fn add_page(&mut self, bitmap: &RgbaImage, quality: u8) -> Result<()> {
        if self.current >= self.pages { bail!("too many PDF pages"); }
        let i = self.current;
        let page_id = 3+i*3;
        let content_id = page_id+1;
        let image_id = page_id+2;
        self.start_obj(page_id)?;
        writeln!(self.output,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.6} {:.6}] /Resources << /XObject << /Im0 {} 0 R >> >> /Contents {} 0 R >>\nendobj",
            self.width_pt,self.height_pt,image_id,content_id)?;
        let content = format!("q\n{:.6} 0 0 {:.6} 0 0 cm\n/Im0 Do\nQ\n",self.width_pt,self.height_pt);
        self.start_obj(content_id)?;
        writeln!(self.output,"<< /Length {} >>\nstream",content.len())?;
        self.output.write_all(content.as_bytes())?;
        self.output.write_all(b"endstream\nendobj\n")?;

        // PDF DCTDecode accepts direct JPEG: avoids an extra PDF compression pass.
        let rgb = image::DynamicImage::ImageRgba8(bitmap.clone()).to_rgb8();
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg,quality).encode_image(&rgb)?;
        self.start_obj(image_id)?;
        writeln!(self.output,
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream",
            bitmap.width(),bitmap.height(),jpeg.len())?;
        self.output.write_all(&jpeg)?;
        self.output.write_all(b"\nendstream\nendobj\n")?;
        self.current += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Result<W> {
        if self.current != self.pages { bail!("only {} of {} PDF pages generated",self.current,self.pages); }
        let xref = self.output.stream_position()?;
        writeln!(self.output,"xref\n0 {}",self.offsets.len())?;
        self.output.write_all(b"0000000000 65535 f \n")?;
        for &offset in self.offsets.iter().skip(1) {
            if offset > 9_999_999_999 { bail!("PDF larger than the classic xref limit"); }
            writeln!(self.output,"{offset:010} 00000 n ")?;
        }
        writeln!(self.output,"trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
            self.offsets.len(),xref)?;
        self.output.flush()?;
        self.output.seek(SeekFrom::End(0))?;
        Ok(self.output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pdf_signature_and_xref() {
        let mut doc = PdfWriter::new(std::io::Cursor::new(Vec::<u8>::new()),1,210.,297.).unwrap();
        doc.add_page(&RgbaImage::new(20,30),90).unwrap();
        let buf=doc.finish().unwrap().into_inner();
        assert!(buf.starts_with(b"%PDF-1.4"));
        assert!(buf.windows(5).any(|x| x == b"xref\n"));
        assert!(buf.ends_with(b"%%EOF\n"));
    }
}
