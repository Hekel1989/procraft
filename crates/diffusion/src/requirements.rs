//! What a server needs for each feature: the add-ons' nodes and, per model family, the model files.
//! The app checks a server against it on connecting; the companion uses the same list to set one
//! up.

use crate::comfy::ServerModels;
use crate::family::{Family, ModelKind};

/// What the app can do with a server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Feature {
    /// A new picture from a prompt.
    Generate,
    /// The picture there is, changed by a prompt.
    Refine,
    /// Generating inside a selection.
    Selections,
    /// Generating guided by control layers.
    ControlLayers,
    /// Making a control layer from a picture, with the server's line extractor.
    ControlFromPicture,
}

impl Feature {
    pub const ALL: [Feature; 5] =
        [Feature::Generate, Feature::Refine, Feature::Selections, Feature::ControlLayers, Feature::ControlFromPicture];

    pub fn label(self) -> &'static str {
        match self {
            Feature::Generate => "Generate",
            Feature::Refine => "Refine",
            Feature::Selections => "Selections",
            Feature::ControlLayers => "Control layers",
            Feature::ControlFromPicture => "Control layers from a picture",
        }
    }

    /// The nodes it uses that stock ComfyUI hasn't got.
    pub fn nodes(self) -> &'static [&'static str] {
        match self {
            Feature::Generate => &["ETN_SendImageWebSocket"],
            Feature::Refine => &["ETN_SendImageWebSocket", "ETN_LoadImageBase64"],
            Feature::Selections => &[
                "ETN_SendImageWebSocket",
                "ETN_LoadImageBase64",
                "ETN_LoadMaskBase64",
                "ETN_control_load",
                "ETN_control_apply",
                "INPAINT_ExpandMask",
                "INPAINT_MaskedFill",
                "INPAINT_ColorMatch",
            ],
            Feature::ControlLayers => {
                &["ETN_SendImageWebSocket", "ETN_LoadImageBase64", "ETN_control_load", "ETN_control_apply"]
            }
            Feature::ControlFromPicture => &[
                "ETN_SendImageWebSocket",
                "ETN_LoadImageBase64",
                "AnimeLineArtPreprocessor",
                "PiDiNetPreprocessor",
                "ScribblePreprocessor",
            ],
        }
    }

    /// The kinds of model file it needs, beyond the family's checkpoint, text encoder and VAE.
    pub(crate) fn models(self) -> &'static [ModelKind] {
        match self {
            Feature::Generate | Feature::Refine | Feature::ControlFromPicture => &[],
            Feature::Selections => &[ModelKind::InpaintControl],
            Feature::ControlLayers => &[ModelKind::LineControl],
        }
    }
}

/// Every node any feature uses: what to ask a server about.
pub fn all_nodes() -> Vec<&'static str> {
    let mut nodes: Vec<&str> = Feature::ALL.iter().flat_map(|f| f.nodes().iter().copied()).collect();
    nodes.sort_unstable();
    nodes.dedup();
    nodes
}

/// The add-on a node comes from, by its name's prefix.
pub fn add_on(node: &str) -> &'static str {
    if node.starts_with("INPAINT_") {
        "comfyui-inpaint-nodes"
    } else if matches!(node, "AnimeLineArtPreprocessor" | "PiDiNetPreprocessor" | "ScribblePreprocessor") {
        "comfyui_controlnet_aux"
    } else {
        "comfyui-tooling-nodes"
    }
}

/// Something a server lacks.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Missing {
    /// An add-on (its name).
    AddOn(&'static str),
    /// A model file: what it is (a file name, or a description) and the folder under `models/`.
    Model { what: String, folder: &'static str },
}

impl std::fmt::Display for Missing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Missing::AddOn(name) => write!(f, "the {name} add-on"),
            Missing::Model { what, folder } => write!(f, "{what} in models/{folder}"),
        }
    }
}

/// Which features a server is ready for with a model family, and what the others lack.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Readiness {
    lacking: Vec<(Feature, Vec<Missing>)>,
}

impl Readiness {
    /// Checks `models` (what a server has) for each feature with `family`.
    pub fn check(family: Family, models: &ServerModels) -> Readiness {
        let base = [ModelKind::Checkpoint, ModelKind::TextEncoder, ModelKind::Vae];
        let lacking = Feature::ALL
            .iter()
            .map(|&feature| {
                let mut missing: Vec<Missing> = Vec::new();
                for &node in feature.nodes() {
                    let add_on = Missing::AddOn(add_on(node));
                    if !models.has_node(node) && !missing.contains(&add_on) {
                        missing.push(add_on);
                    }
                }
                for &kind in base.iter().chain(feature.models()) {
                    if family.pick(kind, models).is_none() {
                        missing.push(Missing::Model { what: family.describe(kind), folder: kind.folder() });
                    }
                }
                (feature, missing)
            })
            .collect();
        Readiness { lacking }
    }

    /// What `feature` lacks; empty when it's ready.
    pub fn missing(&self, feature: Feature) -> &[Missing] {
        self.lacking.iter().find(|(f, _)| *f == feature).map(|(_, m)| m.as_slice()).unwrap_or_default()
    }

    pub fn ready(&self, feature: Feature) -> bool {
        self.missing(feature).is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(nodes: &[&str], controls: &[&str]) -> ServerModels {
        ServerModels {
            diffusion_models: vec!["Anima/anima_baseV10.safetensors".into()],
            text_encoders: vec!["qwen_3_06b_base.safetensors".into()],
            vaes: vec!["qwen_image_vae.safetensors".into()],
            controls: controls.iter().map(|&c| c.to_owned()).collect(),
            nodes: nodes.iter().map(|&n| n.to_owned()).collect(),
        }
    }

    #[test]
    fn a_full_server_is_ready_for_everything() {
        let models = server(
            &all_nodes(),
            &["anima-lllite-inpainting-v2.safetensors", "anima-lllite-any-test-like-v2.safetensors"],
        );
        let r = Readiness::check(Family::Anima, &models);
        assert!(Feature::ALL.iter().all(|&f| r.ready(f)), "{r:?}");
    }

    #[test]
    fn what_is_missing_is_named_per_feature() {
        let models =
            server(&["ETN_SendImageWebSocket", "ETN_LoadImageBase64", "ETN_control_load", "ETN_control_apply"], &[]);
        let r = Readiness::check(Family::Anima, &models);
        assert!(r.ready(Feature::Generate) && r.ready(Feature::Refine));
        let selections: Vec<String> = r.missing(Feature::Selections).iter().map(ToString::to_string).collect();
        assert!(selections.contains(&"the comfyui-inpaint-nodes add-on".to_owned()), "{selections:?}");
        assert!(selections.iter().any(|m| m.ends_with("in models/controlnet")), "{selections:?}");
        assert_eq!(r.missing(Feature::ControlLayers).len(), 1);
    }

    #[test]
    fn nothing_without_the_tooling_add_on() {
        let r = Readiness::check(Family::Anima, &server(&[], &[]));
        assert_eq!(r.missing(Feature::Generate), &[Missing::AddOn("comfyui-tooling-nodes")]);
    }

    #[test]
    fn the_extractors_come_from_the_controlnet_aux_add_on() {
        assert_eq!(add_on("AnimeLineArtPreprocessor"), "comfyui_controlnet_aux");
        assert_eq!(add_on("PiDiNetPreprocessor"), "comfyui_controlnet_aux");
        assert_eq!(add_on("ScribblePreprocessor"), "comfyui_controlnet_aux");
        // the tooling nodes are there, the extractors aren't
        let models = server(
            &["ETN_SendImageWebSocket", "ETN_LoadImageBase64", "ETN_control_load", "ETN_control_apply"],
            &["anima-lllite-inpainting-v2.safetensors", "anima-lllite-any-test-like-v2.safetensors"],
        );
        let r = Readiness::check(Family::Anima, &models);
        assert!(!r.ready(Feature::ControlFromPicture));
        assert_eq!(r.missing(Feature::ControlFromPicture), &[Missing::AddOn("comfyui_controlnet_aux")]);
        // with them it is
        let models = server(&all_nodes(), &[]);
        assert!(Readiness::check(Family::Anima, &models).ready(Feature::ControlFromPicture));
    }
}
