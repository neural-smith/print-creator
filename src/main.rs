use std::{fs, io::{self, Read}, path::PathBuf};
use anyhow::{bail, Context, Result};
use clap::{Parser,Subcommand};
use print_creator::{collect_images,model::{CaptionType,ConflictRule,ImageFormat,Layout,OutputTarget,Page,Photo,PrintJob},output,template};

#[derive(Parser)]
#[command(name="print-creator",version,author=env!("CARGO_PKG_AUTHORS"),about="Standalone digiKam-inspired photo print layout creator")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the graphical print layout designer.
    Gui,
    /// Render an existing JSON print-job configuration.
    Render {
        /// JSON filename, or '-' to read stdin.
        #[arg(long, allow_hyphen_values=true)] job: PathBuf,
        #[arg(long)] output: Option<PathBuf>,
    },
    /// Generate a PDF, images, GIMP pages or printer job directly from photos.
    Quick {
        #[arg(required=true)] photos: Vec<PathBuf>,
        #[arg(short,long,default_value="print-layout.pdf")] output:PathBuf,
        #[arg(long,default_value="pdf",value_parser=["pdf","images","gimp","printer"]) ] target:String,
        #[arg(long,default_value="a4")] page:String,
        #[arg(long,default_value_t=2)] rows:u32,
        #[arg(long,default_value_t=2)] columns:u32,
        /// Outer page margin in centimeters.
        #[arg(long,default_value_t=0.5,conflicts_with="legacy_margin_mm")] margin_cm:f32,
        /// Spacing between photo frames in centimeters.
        #[arg(long,default_value_t=0.3,conflicts_with="legacy_gap_mm")] gap_cm:f32,
        /// Legacy compatibility with v0.1 print scripts (millimeters).
        #[arg(long="margin-mm",hide=true,conflicts_with="margin_cm")] legacy_margin_mm:Option<f32>,
        /// Legacy compatibility with v0.1 print scripts (millimeters).
        #[arg(long="gap-mm",hide=true,conflicts_with="gap_cm")] legacy_gap_mm:Option<f32>,
        #[arg(long,default_value_t=300)] dpi:u32,
        #[arg(long,default_value_t=1)] copies:u32,
        #[arg(long)] landscape:bool,
        /// Keep every photo visible without cropping (overrides per-photo crop).
        #[arg(long,alias="fit")] no_crop:bool,
        /// Expand imported digiKam template slots to remove white margins and gutters.
        #[arg(long)] borderless_templates:bool,
        /// PDF: place photos at source pixel resolution, without resampling or JPEG recompression.
        #[arg(long)] preserve_quality:bool,
        #[arg(long)] no_auto_rotate:bool,
        #[arg(long,default_value="none")] caption:String,
        #[arg(long)] template_xml:Option<PathBuf>,
        #[arg(long)] template_name:Option<String>,
        #[arg(long,default_value="png",value_parser=["png","jpeg","tiff"])] format:String,
        #[arg(long,default_value_t=95)] quality:u8,
        #[arg(long)] printer:Option<String>,
        #[arg(long)] open_folder:bool,
    },
    /// List bundled or imported digiKam Print Creator XML templates.
    ListTemplates { #[arg(long)] xml:Option<PathBuf> },
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match Cli::parse().command.unwrap_or(Command::Gui) {
        Command::Gui => {
            #[cfg(feature="gui")]
            { print_creator::gui::launch()?; }
            #[cfg(not(feature="gui"))]
            { bail!("GUI is not built; use the 'render' or 'quick' subcommand, or enable the 'gui' feature"); }
        }
        Command::Render { job,output:override_path } => {
            let content=if job.as_os_str()=="-" {
                let mut input=String::new();
                io::stdin().read_to_string(&mut input).context("reading JSON job from stdin")?;
                input
            } else {
                fs::read_to_string(&job).with_context(||format!("opening {}",job.display()))?
            };
            let mut job:PrintJob=serde_json::from_str(&content)?;
            if let Some(path)=override_path {job.output.path=path;}
            export_and_report(&job)?;
        }
        Command::ListTemplates {xml} => {
            let templates=match xml {Some(path)=>template::load_xml(&path)?,None=>template::builtin()};
            for item in templates {
                println!("{}\t{:.2} × {:.2} cm\t{} photo(s)\t{}",item.name,item.paper.width_mm/10.0,item.paper.height_mm/10.0,item.slots.len(),item.label);
            }
        }
        Command::Quick {photos,output:out,target,page,rows,columns,margin_cm,gap_cm,legacy_margin_mm,legacy_gap_mm,dpi,copies,landscape,no_crop,
                        no_auto_rotate,borderless_templates,preserve_quality,caption,template_xml,template_name,format,quality,printer,open_folder} => {
            let mut job=PrintJob::default();
            job.page=Page::from_name(&page).with_context(||format!("unknown page size '{page}'"))?;
            if landscape {std::mem::swap(&mut job.page.width_mm,&mut job.page.height_mm);}
            job.layout=Layout::Grid{
                rows,columns,
                margin_mm:legacy_margin_mm.unwrap_or(margin_cm*10.0),
                gap_mm:legacy_gap_mm.unwrap_or(gap_cm*10.0),
            };
            if let Some(xml)=template_xml {
                let candidates=template::load_xml(&xml)?;
                let choice=candidates.iter().find(|t| template_name.as_ref().is_none_or(|n|t.name==*n))
                    .context("selected template not found in XML")?;
                choice.apply(&mut job);
            } else if template_name.is_some() {bail!("--template-name needs --template-xml");}
            job.dpi=dpi;
            job.auto_rotate=!no_auto_rotate;
            job.disable_crop=no_crop;
            job.borderless_templates=borderless_templates;
            job.output.preserve_original_quality=preserve_quality;
            job.photos=collect_images(&photos).into_iter().map(|path|{
                let mut photo=Photo::from_path(path);
                photo.copies=copies;
                photo
            }).collect();
            job.caption.kind=match caption.to_lowercase().as_str() {
                "none"=>CaptionType::None,
                "filename"=>CaptionType::Filename,
                "date"|"datetime"=>CaptionType::DateTime,
                "comment"=>CaptionType::Comment,
                _ if caption.starts_with("custom:")=>{
                    job.caption.custom=caption[7..].to_string();
                    CaptionType::Custom
                }
                _=>bail!("unsupported caption type '{caption}'"),
            };
            job.output.target=match target.as_str() {
                "pdf"=>OutputTarget::Pdf,"images"=>OutputTarget::Images,
                "gimp"=>OutputTarget::Gimp,"printer"=>OutputTarget::Printer,
                _=>unreachable!(),
            };
            job.output.image_format=match format.as_str(){
                "jpeg"=>ImageFormat::Jpeg,"tiff"=>ImageFormat::Tiff,_=>ImageFormat::Png,
            };
            job.output.path=out;
            job.output.jpeg_quality=quality;
            job.output.open_folder=open_folder;
            job.output.printer_name=printer;
            job.output.conflict=ConflictRule::Overwrite;
            export_and_report(&job)?;
        }
    }
    Ok(())
}

fn export_and_report(job:&PrintJob)->Result<()> {
    let outputs=output::export(job)?;
    if outputs.is_empty() {println!("Finished; pages sent to printer or skipped.");}
    for path in outputs {println!("Created {}",path.display());}
    Ok(())
}
