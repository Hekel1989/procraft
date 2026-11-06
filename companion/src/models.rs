//! `models`: the model files a family needs, downloaded into a folder the companion installed.

use std::fs;
use std::io::Write;
use std::path::Path;

use crate::download;
use crate::installed;
use crate::pins;

/// Downloads the Anima set into `dir`, after the licence is accepted.
pub fn anima(dir: &Path, accept_licence: bool) -> Result<(), String> {
    if installed::read(dir).is_err() {
        return Err(format!("{}: not a folder this companion installed", dir.display()));
    }
    let set = &pins::ANIMA;
    println!("{}", set.licence);
    println!("{}", set.link);
    if !accept_licence && !ask()? {
        return Err("the licence was not accepted".to_owned());
    }
    for file in set.files {
        let folder = dir.join("ComfyUI/models").join(file.folder);
        fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        download::download(file.url, &folder.join(file.name), file.size, file.sha256)?;
    }
    Ok(())
}

fn ask() -> Result<bool, String> {
    print!("Accept? [y/N] ");
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).map_err(|e| e.to_string())?;
    Ok(matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}
