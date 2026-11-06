//! `companion.json`: what an install recorded. The companion refuses to write into a folder without
//! it, so it never changes a ComfyUI it didn't make, and `check` compares it with the current pins.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::pins;

/// The file's name in a companion folder.
pub const FILE: &str = "companion.json";

/// What an install wrote down.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installed {
    /// The ComfyUI commit.
    pub comfyui_commit: String,
    /// Each add-on folder and the commit installed there.
    pub add_ons: BTreeMap<String, String>,
    /// The PyTorch index used.
    pub torch_index: String,
    /// The uv version.
    pub uv_version: String,
}

impl Installed {
    /// How this differs from the current pins; empty when it matches.
    pub fn mismatches(&self) -> Vec<String> {
        let mut found = Vec::new();
        if self.comfyui_commit != pins::COMFYUI_COMMIT {
            found.push(format!("ComfyUI is {}, pinned {}", self.comfyui_commit, pins::COMFYUI_COMMIT));
        }
        for add_on in &pins::ADD_ONS {
            match self.add_ons.get(add_on.folder) {
                Some(commit) if commit == add_on.commit => {}
                Some(commit) => found.push(format!("{} is {}, pinned {}", add_on.folder, commit, add_on.commit)),
                None => found.push(format!("{} is not recorded, pinned {}", add_on.folder, add_on.commit)),
            }
        }
        for folder in self.add_ons.keys() {
            if !pins::ADD_ONS.iter().any(|add_on| add_on.folder == folder) {
                found.push(format!("{folder} is recorded but not pinned"));
            }
        }
        if !pins::TORCH_INDEXES.contains(&self.torch_index.as_str()) {
            found.push(format!("the torch index is {}, not one of the pinned indexes", self.torch_index));
        }
        if self.uv_version != pins::UV_VERSION {
            found.push(format!("uv is {}, pinned {}", self.uv_version, pins::UV_VERSION));
        }
        found
    }
}

/// Whether `dir` holds a companion.json.
pub fn exists(dir: &Path) -> bool {
    dir.join(FILE).is_file()
}

/// Reads it.
pub fn read(dir: &Path) -> Result<Installed, String> {
    let path = dir.join(FILE);
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Writes it. Only `install` does this; the other commands only read.
pub fn write(dir: &Path, installed: &Installed) -> Result<(), String> {
    let path = dir.join(FILE);
    let text = serde_json::to_string_pretty(installed).map_err(|e| format!("{}: {e}", path.display()))?;
    fs::write(&path, format!("{text}\n")).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn current() -> Installed {
        Installed {
            comfyui_commit: pins::COMFYUI_COMMIT.to_owned(),
            add_ons: pins::ADD_ONS.iter().map(|add_on| (add_on.folder.to_owned(), add_on.commit.to_owned())).collect(),
            torch_index: pins::TORCH_INDEX_CPU.to_owned(),
            uv_version: pins::UV_VERSION.to_owned(),
        }
    }

    #[test]
    fn a_current_install_matches_the_pins() {
        assert!(current().mismatches().is_empty(), "{:?}", current().mismatches());
    }

    #[test]
    fn a_changed_pin_is_reported() {
        let mut installed = current();
        installed.comfyui_commit = "an old commit".to_owned();
        installed.uv_version = "0.1.0".to_owned();
        let mismatches = installed.mismatches();
        assert!(mismatches.iter().any(|m| m.contains("ComfyUI")), "{mismatches:?}");
        assert!(mismatches.iter().any(|m| m.contains("uv")), "{mismatches:?}");
    }
}
