//! Compatible reader for digiKam Print Creator <templates><paper><template><photo> XML.
use std::{fs, path::Path};
use anyhow::{bail, Context, Result};
use crate::model::{Layout, Page, RectMm};

#[derive(Clone, Debug)]
pub struct TemplateDef {
    pub name: String,
    pub label: String,
    pub paper: Page,
    pub slots: Vec<RectMm>,
    pub auto_rotate: bool,
    pub dpi: Option<u32>,
}

impl TemplateDef {
    pub fn apply(&self, job: &mut crate::model::PrintJob) {
        job.page = self.paper;
        job.layout = Layout::Template {
            name: self.name.clone(), slots: self.slots.clone(), auto_rotate: self.auto_rotate,
        };
        if let Some(dpi) = self.dpi { job.dpi = dpi; }
    }
}

fn attr_f32(node: roxmltree::Node<'_, '_>, key: &str) -> Result<f32> {
    node.attribute(key).with_context(|| format!("missing attribute '{key}'"))?
        .parse().with_context(|| format!("invalid number in '{key}'"))
}

pub fn parse_xml(xml: &str) -> Result<Vec<TemplateDef>> {
    let doc = roxmltree::Document::parse(xml).context("invalid template XML")?;
    let mut out = Vec::new();
    for paper in doc.descendants().filter(|n| n.has_tag_name("paper")) {
        let unit = paper.attribute("unit").unwrap_or("mm").to_ascii_lowercase();
        let factor = match unit.as_str() {
            "mm" => 1., "cm" => 10., "in" | "inch" | "inches" => 25.4,
            _ => bail!("unsupported digiKam template unit: {unit}"),
        };
        let sheet = Page {
            width_mm: attr_f32(paper, "width")? * factor,
            height_mm: attr_f32(paper, "height")? * factor,
        };
        sheet.validate()?;
        for tmpl in paper.children().filter(|n| n.has_tag_name("template")) {
            let name = tmpl.attribute("name").unwrap_or("unnamed").to_string();
            let auto_rotate = tmpl.attribute("autorotate")
                .map(|s| matches!(s.to_ascii_lowercase().as_str(), "true" | "yes" | "1"))
                .unwrap_or(true);
            let dpi = tmpl.attribute("dpi").and_then(|s| s.parse::<u32>().ok()).filter(|d| *d > 0);
            let mut slots = Vec::new();
            for photo in tmpl.children().filter(|n| n.has_tag_name("photo")) {
                let rect = RectMm {
                    x: attr_f32(photo, "x")? * factor,
                    y: attr_f32(photo, "y")? * factor,
                    width: attr_f32(photo, "width")? * factor,
                    height: attr_f32(photo, "height")? * factor,
                };
                if rect.x < 0. || rect.y < 0. || rect.width <= 0. || rect.height <= 0.
                    || rect.x + rect.width > sheet.width_mm + 0.01
                    || rect.y + rect.height > sheet.height_mm + 0.01
                {
                    bail!("template '{}' has photo outside paper bounds", name);
                }
                slots.push(rect);
            }
            if !slots.is_empty() {
                out.push(TemplateDef {
                    label: name.clone(), name, paper: sheet, slots, auto_rotate, dpi,
                });
            }
        }
    }
    Ok(out)
}

pub fn load_xml(path: &Path) -> Result<Vec<TemplateDef>> {
    let xml = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut items = parse_xml(&xml)?;
    let parent = path.parent().unwrap_or(Path::new("."));
    let locale = std::env::var("LC_ALL").or_else(|_| std::env::var("LANG"))
        .unwrap_or_default().split('.').next().unwrap_or("").to_owned();
    for item in &mut items {
        let desktop = parent.join(format!("{}.desktop", item.name));
        if let Ok(text) = fs::read_to_string(desktop) {
            item.label = desktop_label(&text, &locale).unwrap_or_else(|| item.name.clone());
        }
    }
    Ok(items)
}

fn desktop_label(text: &str, locale: &str) -> Option<String> {
    let mut values = std::collections::HashMap::new();
    let mut section = "";
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len()-1];
        } else if section == "Desktop Entry" {
            if let Some((k, v)) = line.split_once('=') { values.insert(k.trim(), v.trim()); }
        }
    }
    let language = locale.split('_').next().unwrap_or(locale);
    for key in [format!("Name[{locale}]"), format!("Name[{language}]"), "Name".to_string()] {
        if let Some(value) = values.get(key.as_str()) { return Some((*value).to_owned()); }
    }
    None
}

pub fn builtin() -> Vec<TemplateDef> {
    let mut templates = parse_xml(include_str!("../assets/templates.xml")).unwrap_or_default();
    // Keep stable digiKam identifiers while presenting readable centimeter labels.
    for item in &mut templates {
        let name = item.name.as_str();
        if let Some((count, dimensions)) = name.split_once("_photos_")
            .or_else(|| name.split_once("_photo_"))
        {
            if let Ok(n) = count.parse::<usize>() {
                let dim = if name == "1_photo_8x10" {
                    "20.32 × 25.40".to_string()
                } else {
                    dimensions.trim_end_matches("cm").replace('x', " × ")
                };
                item.label = format!("{dim} cm ({n} {})", if n == 1 { "photo" } else { "photos" });
            }
        }
    }
    templates
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_passport() {
        let xml = r#"<templates><paper name="passport" width="90" height="130" unit="mm"><template name="six" dpi="0" autorotate="true"><photo x="10" y="3" width="35" height="40"/></template></paper></templates>"#;
        let items = parse_xml(xml).unwrap();
        assert_eq!(items[0].slots[0].width, 35.);
        assert_eq!(items[0].paper.height_mm, 130.);
    }
    #[test]
    fn builtins_valid() {
        let templates = builtin();
        assert!(templates.len() >= 35, "Expected a broad range of print layouts");
        assert!(templates.iter().any(|t|t.name == "1_photo_9x13cm"));
        assert!(templates.iter().any(|t|t.name == "6_photos_3.5x4cm"));
        assert!(templates.iter().any(|t|t.slots.len() >= 12));
    }
}

/// Export an editable layout to digiKam's XML syntax (centimeter units).
pub fn write_compatible_template(job: &crate::model::PrintJob, name: &str, path: &Path) -> Result<()> {
    let frames = match &job.layout {
        Layout::Grid {rows,columns,margin_mm,gap_mm} =>
            crate::layout::grid(job.page,*rows,*columns,*margin_mm,*gap_mm)?,
        Layout::Template {slots,..} => slots.clone(),
        Layout::Adaptive {..} => {
            let ids=job.expanded_photo_indices();
            let slice=ids.iter().take(job.capacity()?).map(|&i|&job.photos[i]).collect::<Vec<_>>();
            crate::layout::slots(&job.layout,job.page,&slice)?
        }
    };
    let escape = |s: &str| s.replace('&',"&amp;").replace('"',"&quot;").replace('<',"&lt;").replace('>',"&gt;");
    let mut xml = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<templates>\n  <paper name=\"custom\" width=\"{}\" height=\"{}\" unit=\"cm\">\n    <template name=\"{}\" dpi=\"0\" autorotate=\"{}\">\n",
        job.page.width_mm/10.0,job.page.height_mm/10.0,escape(name),job.auto_rotate);
    for f in frames {
        xml.push_str(&format!("      <photo x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" />\n",f.x/10.0,f.y/10.0,f.width/10.0,f.height/10.0));
    }
    xml.push_str("    </template>\n  </paper>\n</templates>\n");
    let _ = parse_xml(&xml)?; // Validate before writing to disk.
    fs::write(path,xml).with_context(||format!("saving {}",path.display()))?;
    Ok(())
}
