//! Model families: which files on a server belong to one, the size it works best at, and how its
//! graphs are built. Anima is the only one so far (`family/anima.rs`).

mod anima;

use crate::comfy::ServerModels;
use crate::plan::Plan;
use crate::{ControlStep, Error, Graph, Request};

/// A family of diffusion models that share a text encoder, a VAE and their graphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    /// CircleStone Labs' Anima (a Cosmos-Predict2 derivative): anime and illustration.
    Anima,
}

/// The kinds of model file a family uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ModelKind {
    Checkpoint,
    TextEncoder,
    Vae,
    /// The control model that fills selections.
    InpaintControl,
    /// The control model that follows line art and scribbles.
    LineControl,
}

impl ModelKind {
    /// Its folder under `models/`.
    pub(crate) fn folder(self) -> &'static str {
        match self {
            ModelKind::Checkpoint => "diffusion_models",
            ModelKind::TextEncoder => "text_encoders",
            ModelKind::Vae => "vae",
            ModelKind::InpaintControl | ModelKind::LineControl => "controlnet",
        }
    }

    fn choices(self, models: &ServerModels) -> &[String] {
        match self {
            ModelKind::Checkpoint => &models.diffusion_models,
            ModelKind::TextEncoder => &models.text_encoders,
            ModelKind::Vae => &models.vaes,
            ModelKind::InpaintControl | ModelKind::LineControl => &models.controls,
        }
    }
}

impl Family {
    pub const ALL: [Family; 1] = [Family::Anima];

    pub fn name(self) -> &'static str {
        match self {
            Family::Anima => "Anima",
        }
    }

    /// The model's licence, as the app and the companion show it.
    pub fn licence(self) -> &'static str {
        match self {
            Family::Anima => "CircleStone Labs Non-Commercial License. Built on NVIDIA Cosmos.",
        }
    }

    /// Whether `file` (in `models/diffusion_models`) is one of this family's checkpoints.
    pub fn recognises(self, file: &str) -> bool {
        match self {
            Family::Anima => anima::recognises(file),
        }
    }

    /// The file on the server for `kind`: the first that matches its patterns, in their order.
    pub(crate) fn pick(self, kind: ModelKind, models: &ServerModels) -> Option<String> {
        let choices = kind.choices(models);
        if kind == ModelKind::Checkpoint {
            return choices.iter().find(|f| self.recognises(f)).cloned();
        }
        self.patterns(kind).iter().find_map(|p| choices.iter().find(|f| matches(p, f))).cloned()
    }

    /// What `kind` is called when it's missing: its usual file name.
    pub(crate) fn describe(self, kind: ModelKind) -> String {
        match self {
            Family::Anima => anima::describe(kind).to_owned(),
        }
    }

    fn patterns(self, kind: ModelKind) -> &'static [&'static str] {
        match self {
            Family::Anima => anima::patterns(kind),
        }
    }

    /// The most pixels it makes a picture at, in megapixels, whatever the GPU could take.
    pub fn most_megapixels(self) -> f32 {
        match self {
            Family::Anima => anima::MOST_MEGAPIXELS,
        }
    }

    /// The pixel count it generates best at.
    pub fn native_pixels(self) -> u32 {
        match self {
            Family::Anima => anima::NATIVE_PIXELS,
        }
    }

    /// The denoise a second pass uses, for weighting its progress.
    pub(crate) fn second_pass_denoise(self) -> f64 {
        match self {
            Family::Anima => anima::SECOND_PASS_DENOISE,
        }
    }

    /// How closely a control layer is followed at `step`: its strength and how far through
    /// sampling that applies.
    pub fn control_settings(self, step: ControlStep) -> (f32, f32) {
        match self {
            Family::Anima => anima::control_settings(step),
        }
    }

    /// The graph that makes one picture for `request`, at `plan`'s size, with seed `seed`.
    pub fn graph(self, request: &Request, plan: Plan, seed: u64, models: &ServerModels) -> Result<Graph, Error> {
        match self {
            Family::Anima => anima::graph(request, plan, seed, models),
        }
    }
}

/// Whether `file` matches `pattern`: `*` stands for anything, case doesn't matter, and the pattern
/// is matched against the file's name without its folders.
pub(crate) fn matches(pattern: &str, file: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_match_like_a_shell() {
        assert!(matches("anima*lllite*inpaint*", "anima-lllite-inpainting-v2.safetensors"));
        assert!(matches("qwen_3_06b*", "sub/folder/Qwen_3_06B_base.safetensors"));
        assert!(matches("*anima*", "Anima/divingAnima_v90.safetensors"));
        assert!(!matches("anima*lllite*any*", "anima-lllite-depth-1.safetensors"));
        assert!(matches("exact.safetensors", "exact.safetensors"));
        assert!(!matches("exact.safetensors", "exact.safetensors.bak"));
    }
}
