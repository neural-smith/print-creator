//! Shell-safe exact GUI job reproduction. No temporary JSON files are needed.
use std::{env, path::{Path,PathBuf}};
use anyhow::{Context,Result};
use crate::model::PrintJob;

fn absolute(p:&Path,cwd:&Path)->PathBuf { if p.is_absolute() {p.into()} else {cwd.join(p)} }

pub fn shell_command(job:&PrintJob)->Result<String> {
    let cwd=env::current_dir().context("resolving current directory")?;
    let mut job=job.clone();
    for photo in &mut job.photos {photo.path=absolute(&photo.path,&cwd);}
    if let Some(font)=&mut job.caption.font_file {*font=absolute(font,&cwd);}
    job.output.path=absolute(&job.output.path,&cwd);
    let json=serde_json::to_string_pretty(&job)?;
    let mut delim="PRINT_CREATOR_JOB".to_owned();
    while json.lines().any(|line|line==delim) {delim.push_str("_END");}
    Ok(format!("print-creator render --job - <<'{delim}'\n{json}\n{delim}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_serialization_is_safe_for_shell_metacharacters() {
        let mut job=PrintJob::default();
        job.caption.custom="weird $HOME $(malicious) ' quotes".into();
        let text=shell_command(&job).unwrap();
        assert!(text.starts_with("print-creator render --job - <<'"));
        let (_,body)=text.split_once('\n').unwrap();
        let delim=text.lines().next().unwrap().split("<<'").nth(1).unwrap().trim_end_matches('\'');
        let json=body.rsplit_once(&format!("\n{delim}\n")).unwrap().0;
        let result:PrintJob=serde_json::from_str(json).unwrap();
        assert_eq!(result.caption.custom,job.caption.custom);
        assert!(result.output.path.is_absolute());
    }
}
