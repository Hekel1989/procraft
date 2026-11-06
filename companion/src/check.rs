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
    check_installed(dir);

    for add_on in &pins::ADD_ONS {
        if add_on_present(dir, add_on.folder) {
            println!("✓ add-on {}", add_on.folder);
        } else {
            println!("ℹ add-on {}: missing", add_on.folder);
        }
    }

    let models = check_models(dir, verify);
    let mut ready = true;
    for family in Family::ALL {
        println!("{}:", family.name());
        print_kinds(&models);
        let readiness = Readiness::check(family, &models);
        for feature in Feature::ALL {
            ready &= print_feature(&readiness, feature);
        }
    }
    Ok(ready)
}

/// Notes what `companion.json` says; versions never decide readiness, only the features do.
fn check_installed(dir: &Path) {
    if !installed::exists(dir) {
        if installed::unfinished(dir) {
            println!("✕ an install started here and didn't finish: run install again");
        } else {
            println!("ℹ not installed by the companion: checked as it is");
        }
        return;
    }
    match installed::read(dir) {
        Ok(installed) => {
            let mismatches = installed.mismatches();
            if mismatches.is_empty() {
                println!("✓ companion.json matches the current pins");
            } else {
                for mismatch in mismatches {
                    println!("ℹ companion.json: {mismatch}");
                }
            }
        }
        Err(e) => println!("ℹ companion.json: {e}"),
    }
}

/// The models a folder has, as a server would list them, and the nodes its add-ons provide. A file
/// with a pinned name only counts when its size matches the pin; any other file matching a family's
/// patterns counts as it is. `--verify` also drops a pinned file whose hash is wrong.
fn check_models(dir: &Path, verify: bool) -> ServerModels {
    let root = dir.join("ComfyUI/models");
    let mut models = ServerModels {
        diffusion_models: check_model_folder(&root, "diffusion_models", verify),
        text_encoders: check_model_folder(&root, "text_encoders", verify),
        vaes: check_model_folder(&root, "vae", verify),
        controls: check_model_folder(&root, "controlnet", verify),
        nodes: Vec::new(),
    };
    models.nodes =
        all_nodes().into_iter().filter(|node| add_on_present(dir, add_on(node))).map(str::to_owned).collect();
    models
}

/// The files in one `models/` folder that can count, checking each pinned name it holds.
fn check_model_folder(root: &Path, folder: &str, verify: bool) -> Vec<String> {
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
                        Ok(hash) => println!("ℹ model {label}: SHA-256 is {hash}, pinned {}", pin.sha256),
                        Err(e) => println!("ℹ model {label}: {e}"),
                    }
                } else {
                    println!("✓ model {label} ({} bytes)", pin.size);
                    usable.push(relative);
                }
            }
            Ok(metadata) => println!("ℹ model {label}: {} bytes, pinned {} bytes", metadata.len(), pin.size),
            Err(e) => println!("ℹ model {label}: {e}"),
        }
    }
    usable
}

/// The model kinds to list for Anima, in the order the app uses them. The patterns mirror
/// `Family`'s in `crates/diffusion`, which the companion can't reach.
struct Kind {
    label: &'static str,
    folder: &'static str,
    patterns: &'static [&'static str],
}

const ANIMA_KINDS: [Kind; 5] = [
    Kind { label: "checkpoint", folder: "diffusion_models", patterns: &["*anima*"] },
    Kind { label: "text encoder", folder: "text_encoders", patterns: &["qwen_3_06b*"] },
    Kind { label: "VAE", folder: "vae", patterns: &["qwen_image_vae*"] },
    Kind {
        label: "inpaint control",
        folder: "controlnet",
        patterns: &["anima*lllite*inpaint*", "anima*inpaint*lllite*"],
    },
    Kind { label: "line control", folder: "controlnet", patterns: &["anima*lllite*any*"] },
];

impl Kind {
    /// The files that satisfy this kind, as `folder/name`.
    fn files(&self, models: &ServerModels) -> Vec<String> {
        let choices: &[String] = match self.folder {
            "diffusion_models" => &models.diffusion_models,
            "text_encoders" => &models.text_encoders,
            "vae" => &models.vaes,
            "controlnet" => &models.controls,
            _ => return Vec::new(),
        };
        choices
            .iter()
            .filter(|file| self.patterns.iter().any(|pattern| glob(pattern, file)))
            .map(|file| format!("{}/{}", self.folder, file))
            .collect()
    }
}

fn print_kinds(models: &ServerModels) {
    for kind in &ANIMA_KINDS {
        let files = kind.files(models);
        if files.is_empty() {
            println!("  ✕ {}: none", kind.label);
        } else {
            println!("  ✓ {} {}", kind.label, files.join(", "));
        }
    }
}

/// Whether `file` matches `pattern`: `*` stands for anything, case doesn't matter, and the pattern
/// is matched against the file's name without its folders.
fn glob(pattern: &str, file: &str) -> bool {
    let name = file.rsplit(['/', '\\']).next().unwrap_or(file).to_lowercase();
    let pattern = pattern.to_lowercase();
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = name.strip_prefix(first) else { return false };
    let mut parts: Vec<&str> = parts.collect();
    let last = if pattern.contains('*') { parts.pop() } else { None };
    for part in parts {
        match rest.find(part) {
            Some(at) => rest = rest.get(at + part.len()..).unwrap_or_default(),
            None => return false,
        }
    }
    match last {
        Some(last) => rest.ends_with(last),
        None => rest.is_empty(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_dir() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "procraft-companion-check-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn patterns_match_like_the_family_ones() {
        assert!(glob("*anima*", "anima_baseV10.safetensors"));
        assert!(glob("*anima*", "Anima/divingAnima_v90.safetensors"));
        assert!(glob("qwen_3_06b*", "sub/folder/Qwen_3_06B_base.safetensors"));
        assert!(glob("anima*lllite*inpaint*", "anima-lllite-inpainting-v2.safetensors"));
        assert!(!glob("anima*lllite*any*", "anima-lllite-depth-1.safetensors"));
    }

    #[test]
    fn each_kind_lists_the_files_that_satisfy_it() {
        let models = ServerModels {
            diffusion_models: vec![
                "anima_baseV10.safetensors".to_owned(),
                "anima_turboV10.safetensors".to_owned(),
                "something_else.safetensors".to_owned(),
            ],
            text_encoders: vec!["qwen_3_06b_base.safetensors".to_owned()],
            vaes: vec!["qwen_image_vae.safetensors".to_owned()],
            controls: vec![
                "anima-lllite-inpainting-v2.safetensors".to_owned(),
                "anima-lllite-any-test-like-v2.safetensors".to_owned(),
            ],
            nodes: Vec::new(),
        };
        let checkpoints = ANIMA_KINDS.first().map(|kind| kind.files(&models)).unwrap_or_default();
        assert_eq!(
            checkpoints,
            vec![
                "diffusion_models/anima_baseV10.safetensors".to_owned(),
                "diffusion_models/anima_turboV10.safetensors".to_owned()
            ]
        );
        assert_eq!(
            ANIMA_KINDS.get(3).map(|kind| kind.files(&models)).unwrap_or_default(),
            vec!["controlnet/anima-lllite-inpainting-v2.safetensors".to_owned()]
        );
        assert_eq!(
            ANIMA_KINDS.get(4).map(|kind| kind.files(&models)).unwrap_or_default(),
            vec!["controlnet/anima-lllite-any-test-like-v2.safetensors".to_owned()]
        );
    }

    #[test]
    fn a_folder_with_every_feature_is_ready_and_needs_no_companion_json() {
        let dir = temp_dir();
        for add_on in &pins::ADD_ONS {
            fs::create_dir_all(dir.join("ComfyUI/custom_nodes").join(add_on.folder)).unwrap();
        }
        let files = [
            ("diffusion_models", "anima_baseV10.safetensors"),
            ("text_encoders", "qwen_3_06b_custom.safetensors"),
            ("vae", "qwen_image_vae_custom.safetensors"),
            ("controlnet", "anima-lllite-inpainting-custom.safetensors"),
            ("controlnet", "anima-lllite-any-custom.safetensors"),
        ];
        for (folder, name) in files {
            let folder = dir.join("ComfyUI/models").join(folder);
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join(name), b"a small stand-in").unwrap();
        }

        assert!(check_folder(&dir, false).unwrap());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_empty_folder_is_not_ready() {
        let dir = temp_dir();
        assert!(!check_folder(&dir, false).unwrap());
        let _ = fs::remove_dir_all(dir);
    }
}
