//! `check`: what a folder or a running server has, read-only. Folder checks use the same
//! requirements the app does, so any file matching a family's patterns counts.

use std::fs;
use std::path::{Path, PathBuf};

use procraft_diffusion::requirements::{Feature, Readiness, add_on, all_nodes};
use procraft_diffusion::{Address, Comfy, Family, ServerModels, styles_for};

use crate::download;
use crate::installed;
use crate::pins;

/// Checks `target` (a folder, or `host:port` / `http://...`); true when everything is ready.
pub fn run(target: &str, verify: bool) -> Result<bool, String> {
    match target_of(target)? {
        Target::Server(address) => check_server(address),
        Target::Folder(dir) => check_folder(&dir, verify),
    }
}

enum Target {
    Server(Address),
    Folder(PathBuf),
}

fn target_of(text: &str) -> Result<Target, String> {
    if text.starts_with("http://") {
        return Address::parse(text).map(Target::Server).ok_or_else(|| format!("not an address: {text}"));
    }
    // a folder that exists wins over a `host:port` reading of the same words; so does anything
    // with a path separator (Windows drive paths have a colon too)
    if Path::new(text).exists() || !text.contains(':') || text.contains(['/', '\\']) {
        return Ok(Target::Folder(PathBuf::from(text)));
    }
    Address::parse(text).map(Target::Server).ok_or_else(|| format!("not a folder or an address: {text}"))
}

fn check_server(address: Address) -> Result<bool, String> {
    let comfy = Comfy::new(address);
    let stats = comfy.system_stats().map_err(|e| e.to_string())?;
    let models = comfy.models(&all_nodes()).map_err(|e| e.to_string())?;
    let gpu = stats.gpu.as_ref().map(|(name, vram)| format!(", {name} ({} GB)", vram / (1 << 30))).unwrap_or_default();
    println!("ComfyUI {}{gpu}", stats.comfyui_version);

    let mut ready = true;
    for family in Family::ALL {
        let readiness = Readiness::check(family, &models);
        println!("{}:", family.name());
        for feature in Feature::ALL {
            ready &= print_feature(&readiness, feature);
        }
    }
    println!("styles:");
    for style in styles_for(&models) {
        println!("  {:<40} {} ({} steps, CFG {})", style.id, style.name, style.steps, style.cfg);
    }
    Ok(ready)
}

fn check_folder(dir: &Path, verify: bool) -> Result<bool, String> {
    println!("Folder: {}", dir.display());
    let mut ready = check_installed(dir);

    for add_on in &pins::ADD_ONS {
        if add_on_present(dir, add_on.folder) {
            println!("✓ add-on {}", add_on.folder);
        } else {
            println!("✕ add-on {}: missing", add_on.folder);
            ready = false;
        }
    }

    let (models, models_ready) = check_models(dir, verify);
    ready &= models_ready;

    for family in Family::ALL {
        let readiness = Readiness::check(family, &models);
        println!("{}:", family.name());
        for feature in Feature::ALL {
            ready &= print_feature(&readiness, feature);
        }
    }
    Ok(ready)
}

fn check_installed(dir: &Path) -> bool {
    if !installed::exists(dir) {
        println!("✕ companion.json: not a folder this companion installed");
        return false;
    }
    match installed::read(dir) {
        Ok(installed) => {
            let mismatches = installed.mismatches();
            if mismatches.is_empty() {
                println!("✓ companion.json matches the current pins");
                true
            } else {
                for mismatch in mismatches {
                    println!("✕ companion.json: {mismatch}");
                }
                false
            }
        }
        Err(e) => {
            println!("✕ companion.json: {e}");
            false
        }
    }
}

/// The models a folder has, as a server would list them, and the nodes its add-ons provide. A file
/// with a pinned name only counts when its size matches the pin; any other file matching a family's
/// patterns counts as it is. `--verify` also drops a pinned file whose hash is wrong.
fn check_models(dir: &Path, verify: bool) -> (ServerModels, bool) {
    let root = dir.join("ComfyUI/models");
    let mut ready = true;
    let mut models = ServerModels {
        diffusion_models: check_model_folder(&root, "diffusion_models", verify, &mut ready),
        text_encoders: check_model_folder(&root, "text_encoders", verify, &mut ready),
        vaes: check_model_folder(&root, "vae", verify, &mut ready),
        controls: check_model_folder(&root, "controlnet", verify, &mut ready),
        nodes: Vec::new(),
    };
    models.nodes =
        all_nodes().into_iter().filter(|node| add_on_present(dir, add_on(node))).map(str::to_owned).collect();
    (models, ready)
}

/// The files in one `models/` folder that can count, checking each pinned name it holds.
fn check_model_folder(root: &Path, folder: &str, verify: bool, ready: &mut bool) -> Vec<String> {
    let mut usable = Vec::new();
    for relative in files_in(&root.join(folder)) {
        let name = relative.rsplit('/').next().unwrap_or(&relative);
        let Some(pin) = pinned(folder, name) else {
            usable.push(relative);
            continue;
        };
        let path = root.join(folder).join(&relative);
        let label = format!("{folder}/{relative}");
        match fs::metadata(&path) {
            Ok(metadata) if metadata.len() == pin.size => {
                if verify {
                    match download::sha256_file(&path) {
                        Ok(hash) if hash == pin.sha256 => {
                            println!("✓ model {label} ({} bytes, SHA-256 ok)", pin.size);
                            usable.push(relative);
                        }
                        Ok(hash) => {
                            println!("✕ model {label}: SHA-256 is {hash}, pinned {}", pin.sha256);
                            *ready = false;
                        }
                        Err(e) => {
                            println!("✕ model {label}: {e}");
                            *ready = false;
                        }
                    }
                } else {
                    println!("✓ model {label} ({} bytes)", pin.size);
                    usable.push(relative);
                }
            }
            Ok(metadata) => {
                println!("✕ model {label}: {} bytes, pinned {} bytes", metadata.len(), pin.size);
                *ready = false;
            }
            Err(e) => {
                println!("✕ model {label}: {e}");
                *ready = false;
            }
        }
    }
    usable
}

/// The pin for a file, by its folder and file name.
fn pinned(folder: &str, name: &str) -> Option<&'static pins::ModelFile> {
    pins::ANIMA.files.iter().find(|file| file.folder == folder && file.name == name)
}

fn print_feature(readiness: &Readiness, feature: Feature) -> bool {
    let missing = readiness.missing(feature);
    if missing.is_empty() {
        println!("  ✓ {}: ready", feature.label());
        true
    } else {
        let list: Vec<String> = missing.iter().map(ToString::to_string).collect();
        println!("  ✕ {}: needs {}", feature.label(), list.join(", "));
        false
    }
}

fn add_on_present(dir: &Path, name: &str) -> bool {
    dir.join("ComfyUI/custom_nodes").join(name).is_dir()
}

/// Every file under `folder`, as paths relative to it with `/` between folders.
fn files_in(folder: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect(folder, folder, &mut files);
    files.sort();
    files
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // don't follow folder symlinks: they can loop, or point outside the folder
            if !path.is_symlink() {
                collect(root, &path, files);
            }
        } else if path.is_file()
            && let Ok(relative) = path.strip_prefix(root)
        {
            files.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}
