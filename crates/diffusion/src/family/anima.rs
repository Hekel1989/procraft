//! Anima: a 2B Cosmos-Predict2 derivative for anime and illustration, prompted with booru tags or
//! plain words. Its text encoder is Qwen3 0.6B (loaded as ComfyUI's `omnigen2` type) and its VAE
//! Qwen-Image's. It works best near 1 MP; its control and inpaint models are LLLite, applied with
//! the tooling add-on's control nodes.

use serde_json::json;

use super::ModelKind;
use crate::comfy::ServerModels;
use crate::plan::Plan;
use crate::{Error, Family, Graph, Out, Request};

pub(super) const NATIVE_PIXELS: u32 = 1024 * 1024;

/// Its checkpoints: anything with "anima" in the name in `diffusion_models`.
pub(super) fn recognises(file: &str) -> bool {
    super::matches("*anima*", file)
}

/// The files for each kind, best first.
pub(super) fn patterns(kind: ModelKind) -> &'static [&'static str] {
    match kind {
        ModelKind::Checkpoint => &["*anima*"],
        ModelKind::TextEncoder => &["qwen_3_06b*"],
        ModelKind::Vae => &["qwen_image_vae*"],
        ModelKind::InpaintControl => &["anima*lllite*inpaint*", "anima*inpaint*lllite*"],
        ModelKind::LineControl => &["anima*lllite*any*"],
    }
}

pub(super) fn describe(kind: ModelKind) -> &'static str {
    match kind {
        ModelKind::Checkpoint => "an Anima model (anima-base-v1.0.safetensors)",
        ModelKind::TextEncoder => "qwen_3_06b_base.safetensors",
        ModelKind::Vae => "qwen_image_vae.safetensors",
        ModelKind::InpaintControl => "anima-lllite-inpainting-v2.safetensors",
        ModelKind::LineControl => "anima-lllite-any-test-like-v2.safetensors",
    }
}

/// One picture: from an empty latent at full strength, or from the picture sent (scaled to the
/// plan) below it.
pub(super) fn graph(request: &Request, plan: Plan, seed: u64, models: &ServerModels) -> Result<Graph, Error> {
    let family = Family::Anima;
    let file = |kind| family.pick(kind, models).ok_or_else(|| missing(kind));
    let style = &request.style;
    let mut g = Graph::new();
    let model = g.add("UNETLoader", [("unet_name", json!(style.checkpoint)), ("weight_dtype", json!("default"))]);
    let clip = g.add("CLIPLoader", [("clip_name", json!(file(ModelKind::TextEncoder)?)), ("type", json!("omnigen2"))]);
    let vae = g.add("VAELoader", [("vae_name", json!(file(ModelKind::Vae)?))]);
    let text =
        |g: &mut Graph, text: String| g.add("CLIPTextEncode", [("clip", clip.out(0).into()), ("text", json!(text))]);
    let positive = text(&mut g, style.positive(&request.prompt, request.nsfw));
    let negative = text(&mut g, style.negative.clone());

    let refine = request.strength < 1.0;
    let latent: Out = if refine {
        let Some(picture) = &request.picture else {
            return Err(Error::Request { reason: "refining needs the picture there is".to_owned() });
        };
        let png = picture.flattened([255, 255, 255]).resized(plan.width, plan.height)?.to_png()?;
        let image = g.add("ETN_LoadImageBase64", [("image", json!(procraft_engine::base64::encode(&png)))]);
        g.add("VAEEncode", [("pixels", image.out(0).into()), ("vae", vae.out(0).into())]).into()
    } else {
        g.add(
            "EmptyLatentImage",
            [("width", json!(plan.width)), ("height", json!(plan.height)), ("batch_size", json!(1))],
        )
        .into()
    };
    let sampled = g.add(
        "KSampler",
        [
            ("model", model.out(0).into()),
            ("seed", json!(seed)),
            ("steps", json!(style.steps)),
            ("cfg", json!(style.cfg)),
            ("sampler_name", json!(style.sampler)),
            ("scheduler", json!(style.scheduler)),
            ("positive", positive.out(0).into()),
            ("negative", negative.out(0).into()),
            ("latent_image", latent.into()),
            ("denoise", json!(if refine { request.strength.clamp(0.01, 1.0) } else { 1.0 })),
        ],
    );
    let image = g.add("VAEDecode", [("samples", sampled.out(0).into()), ("vae", vae.out(0).into())]);
    g.add("ETN_SendImageWebSocket", [("images", image.out(0).into()), ("format", json!("PNG"))]);
    Ok(g)
}

fn missing(kind: ModelKind) -> Error {
    Error::Missing(vec![crate::Missing::Model { what: describe(kind).to_owned(), folder: kind.folder() }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Picture, Style};

    fn models() -> ServerModels {
        ServerModels {
            diffusion_models: vec!["anima_baseV10.safetensors".into()],
            text_encoders: vec!["qwen_3_06b_base.safetensors".into()],
            vaes: vec!["qwen_image_vae.safetensors".into()],
            ..ServerModels::default()
        }
    }

    fn request(strength: f32) -> Request {
        let style = Style::built_in("anima", &models()).unwrap();
        Request {
            prompt: "1girl, night sky".into(),
            style,
            strength,
            seed: 7,
            count: 1,
            picture: Some(Picture::new(2, 2, vec![200; 16]).unwrap()),
            size: (2, 2),
            nsfw: false,
        }
    }

    #[test]
    fn a_new_picture_starts_from_an_empty_latent() {
        let plan = Plan { width: 1024, height: 768 };
        let g = graph(&request(1.0), plan, 42, &models()).unwrap().to_json();
        assert_eq!(g["1"]["inputs"]["unet_name"], "anima_baseV10.safetensors");
        assert_eq!(g["2"]["inputs"]["type"], "omnigen2");
        assert_eq!(g["4"]["inputs"]["text"], "masterpiece, best quality, safe, 1girl, night sky");
        assert_eq!(g["6"]["class_type"], "EmptyLatentImage");
        assert_eq!((g["6"]["inputs"]["width"].clone(), g["6"]["inputs"]["height"].clone()), (json!(1024), json!(768)));
        assert_eq!(g["7"]["inputs"]["seed"], 42);
        assert_eq!(g["7"]["inputs"]["denoise"], 1.0);
        assert_eq!(g["9"]["class_type"], "ETN_SendImageWebSocket");
    }

    #[test]
    fn refining_sends_the_picture_at_the_plans_size() {
        let g = graph(&request(0.5), Plan { width: 64, height: 32 }, 1, &models()).unwrap();
        let json = g.to_json();
        assert_eq!(g.nodes_of("ETN_LoadImageBase64").len(), 1);
        assert_eq!(json["8"]["inputs"]["denoise"], 0.5);
        let png = procraft_engine::base64::decode(json["6"]["inputs"]["image"].as_str().unwrap()).unwrap();
        let sent = Picture::decode(&png).unwrap();
        assert_eq!((sent.width, sent.height), (64, 32));
    }

    #[test]
    fn a_missing_vae_is_named() {
        let mut m = models();
        m.vaes.clear();
        let err = graph(&request(1.0), Plan { width: 64, height: 64 }, 1, &m).unwrap_err();
        assert_eq!(err.to_string(), "the server is missing qwen_image_vae.safetensors in models/vae");
    }
}
