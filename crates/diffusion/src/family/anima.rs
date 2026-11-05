//! Anima: a 2B Cosmos-Predict2 derivative for anime and illustration, prompted with booru tags or
//! plain words. Its text encoder is Qwen3 0.6B (loaded as ComfyUI's `omnigen2` type) and its VAE
//! Qwen-Image's. It works best near 1 MP; its control and inpaint models are LLLite, applied with
//! the tooling add-on's control nodes.

use serde_json::json;

use super::ModelKind;
use crate::comfy::ServerModels;
use crate::plan::Plan;
use crate::{Error, Family, Graph, Out, Picture, Request, SelectionMode};

pub(super) const NATIVE_PIXELS: u32 = 1024 * 1024;
/// How much the second pass of a two-pass picture changes it: enough to add detail, too little to
/// invent a new composition.
pub(super) const SECOND_PASS_DENOISE: f64 = 0.3;
/// The most pixels a pass is made at, in megapixels: past about 3 MP Anima's pictures go soft and
/// show a faint grid (tried at 5.6 MP on 2800 × 2000), so bigger targets are scaled up the rest.
pub(super) const MOST_MEGAPIXELS: f32 = 3.0;
/// The inpaint control's strength: the LLLite layer follows the mask whole.
pub(super) const INPAINT_CONTROL_STRENGTH: f64 = 1.0;
/// How much the last colour match pulls a selection's result towards the original: enough to blend
/// it in, not enough to undo the generated colours.
pub(super) const COLOR_MATCH: f64 = 0.5;
/// A selection's mask grows by this share of the context box's longer side, and blurs by this
/// share of the growth.
const INPAINT_GROW: f32 = 0.02;
const INPAINT_GROW_MIN: u32 = 8;
const INPAINT_BLUR_SHARE: u32 = 2;

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
/// plan) below it. A two-pass plan composes at the first size, scales that up and samples it again
/// at the second. A selection goes through the inpaint structure instead: one pass at the plan's
/// size, prefilled from its surroundings, with the LLLite inpaint control.
pub(super) fn graph(request: &Request, plan: Plan, seed: u64, models: &ServerModels) -> Result<Graph, Error> {
    let family = Family::Anima;
    let file = |kind| family.pick(kind, models).ok_or_else(|| missing(kind));
    let style = &request.style;
    let mut g = Graph::new();
    let checkpoint = g.add("UNETLoader", [("unet_name", json!(style.checkpoint)), ("weight_dtype", json!("default"))]);
    let clip = g.add("CLIPLoader", [("clip_name", json!(file(ModelKind::TextEncoder)?)), ("type", json!("omnigen2"))]);
    let vae = g.add("VAELoader", [("vae_name", json!(file(ModelKind::Vae)?))]);
    let text =
        |g: &mut Graph, text: String| g.add("CLIPTextEncode", [("clip", clip.out(0).into()), ("text", json!(text))]);
    let selection = request.selection.as_ref();
    let removing = selection.is_some_and(|s| s.mode == SelectionMode::Remove);
    let prompt = if removing { "" } else { request.prompt.as_str() };
    let positive = text(&mut g, style.positive(prompt));
    let negative = if removing {
        text(&mut g, format!("{}, object, person, text", style.negative))
    } else {
        text(&mut g, style.negative.clone())
    };
    let mut model: Out = checkpoint.out(0);
    let sampler = |g: &mut Graph, model: Out, latent: Out, denoise: f64| {
        g.add(
            "KSampler",
            [
                ("model", model.into()),
                ("seed", json!(seed)),
                ("steps", json!(style.steps)),
                ("cfg", json!(style.cfg)),
                ("sampler_name", json!(style.sampler)),
                ("scheduler", json!(style.scheduler)),
                ("positive", positive.out(0).into()),
                ("negative", negative.out(0).into()),
                ("latent_image", latent.into()),
                ("denoise", json!(denoise)),
            ],
        )
    };

    if let Some(selection) = selection {
        let Some(picture) = &request.picture else {
            return Err(Error::Request { reason: "a selection needs the picture there is".to_owned() });
        };
        let expected = u64::from(request.size.0) * u64::from(request.size.1);
        if selection.mask.len() as u64 != expected {
            return Err(Error::Request {
                reason: format!("{} bytes aren't the selection's {expected} pixels", selection.mask.len()),
            });
        }
        let (width, height) = (plan.first.width, plan.first.height);
        let original = picture.flattened([255, 255, 255]).resized(width, height)?.to_png()?;
        let original = g.add("ETN_LoadImageBase64", [("image", json!(procraft_engine::base64::encode(&original)))]);
        let mask = Picture::resized_mask(request.size.0, request.size.1, &selection.mask, width, height)?;
        let mask = Picture::mask_png(width, height, &mask)?;
        let mask = g.add("ETN_LoadMaskBase64", [("mask", json!(procraft_engine::base64::encode(&mask)))]);
        let longer = width.max(height);
        let grow = ((longer as f32 * INPAINT_GROW).round() as u32).max(INPAINT_GROW_MIN);
        let blur = (grow / INPAINT_BLUR_SHARE).max(1);
        let grown = g.add(
            "INPAINT_ExpandMask",
            [
                ("mask", mask.out(0).into()),
                ("grow", json!(grow)),
                ("blur", json!(blur)),
                ("blur_type", json!("gaussian")),
            ],
        );
        let fill = match selection.mode {
            SelectionMode::Fill | SelectionMode::Add => "neutral",
            SelectionMode::Remove => "telea",
            SelectionMode::Expand => "navier-stokes",
        };
        let filled = g.add(
            "INPAINT_MaskedFill",
            [
                ("image", original.out(0).into()),
                ("mask", grown.out(0).into()),
                ("fill", json!(fill)),
                ("falloff", json!(0)),
            ],
        );
        let control = g.add(
            "ETN_control_load",
            [("model", checkpoint.out(0).into()), ("weights", json!(file(ModelKind::InpaintControl)?))],
        );
        let controlled = g.add(
            "ETN_control_apply",
            [
                ("model", control.out(0).into()),
                ("control_net", control.out(1).into()),
                ("image", filled.out(0).into()),
                ("mask", grown.out(0).into()),
                ("strength", json!(INPAINT_CONTROL_STRENGTH)),
                ("start_percent", json!(0.0)),
                ("end_percent", json!(1.0)),
            ],
        );
        model = controlled.out(0);
        let encoded = g.add("VAEEncode", [("pixels", filled.out(0).into()), ("vae", vae.out(0).into())]);
        let latent = g.add("SetLatentNoiseMask", [("samples", encoded.out(0).into()), ("mask", grown.out(0).into())]);
        let sampled = sampler(&mut g, model, latent.into(), f64::from(request.strength.clamp(0.01, 1.0)));
        let decoded = g.add("VAEDecode", [("samples", sampled.out(0).into()), ("vae", vae.out(0).into())]);
        let composited = g.add(
            "ImageCompositeMasked",
            [
                ("destination", original.out(0).into()),
                ("source", decoded.out(0).into()),
                ("x", json!(0)),
                ("y", json!(0)),
                ("resize_source", json!(false)),
                ("mask", grown.out(0).into()),
            ],
        );
        let matched = g.add(
            "INPAINT_ColorMatch",
            [
                ("target", composited.out(0).into()),
                ("reference", original.out(0).into()),
                ("exclude_mask", grown.out(0).into()),
                ("strength", json!(COLOR_MATCH)),
            ],
        );
        g.add("ETN_SendImageWebSocket", [("images", matched.out(0).into()), ("format", json!("PNG"))]);
        return Ok(g);
    }

    let refine = request.strength < 1.0;
    let latent: Out = if refine {
        let Some(picture) = &request.picture else {
            return Err(Error::Request { reason: "refining needs the picture there is".to_owned() });
        };
        let png = picture.flattened([255, 255, 255]).resized(plan.first.width, plan.first.height)?.to_png()?;
        let image = g.add("ETN_LoadImageBase64", [("image", json!(procraft_engine::base64::encode(&png)))]);
        g.add("VAEEncode", [("pixels", image.out(0).into()), ("vae", vae.out(0).into())]).into()
    } else {
        g.add(
            "EmptyLatentImage",
            [("width", json!(plan.first.width)), ("height", json!(plan.first.height)), ("batch_size", json!(1))],
        )
        .into()
    };
    let sampled =
        sampler(&mut g, model, latent, if refine { f64::from(request.strength.clamp(0.01, 1.0)) } else { 1.0 });
    let mut image = g.add("VAEDecode", [("samples", sampled.out(0).into()), ("vae", vae.out(0).into())]);
    if let Some(second) = plan.second.filter(|_| !refine) {
        let scaled = g.add(
            "ImageScale",
            [
                ("image", image.out(0).into()),
                ("upscale_method", json!("lanczos")),
                ("width", json!(second.width)),
                ("height", json!(second.height)),
                ("crop", json!("disabled")),
            ],
        );
        let encoded = g.add("VAEEncode", [("pixels", scaled.out(0).into()), ("vae", vae.out(0).into())]);
        let refined = sampler(&mut g, model, encoded.into(), SECOND_PASS_DENOISE);
        image = g.add("VAEDecode", [("samples", refined.out(0).into()), ("vae", vae.out(0).into())]);
    }
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
            controls: vec!["anima-lllite-inpainting-v2.safetensors".into()],
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
            selection: None,
            size: (2, 2),
            resolution: crate::Resolution::Quality,
            largest: None,
            nsfw: false,
        }
    }

    fn selection(mode: SelectionMode) -> Request {
        let mut r = request(0.75);
        r.size = (64, 64);
        r.picture = Some(Picture::new(64, 64, vec![200; 64 * 64 * 4]).unwrap());
        r.selection = Some(crate::Selection { mask: vec![255; 64 * 64], mode });
        r
    }

    fn one_pass(width: u32, height: u32) -> Plan {
        let size = crate::Size::new(width, height);
        Plan { first: size, second: None, size, target: size, capped: false }
    }

    fn two_pass(first: (u32, u32), second: (u32, u32)) -> Plan {
        let first = crate::Size::new(first.0, first.1);
        let second = crate::Size::new(second.0, second.1);
        Plan { first, second: Some(second), size: second, target: second, capped: false }
    }

    #[test]
    fn a_new_picture_starts_from_an_empty_latent() {
        let g = graph(&request(1.0), one_pass(1024, 768), 42, &models()).unwrap().to_json();
        assert_eq!(g["1"]["inputs"]["unet_name"], "anima_baseV10.safetensors");
        assert_eq!(g["2"]["inputs"]["type"], "omnigen2");
        assert_eq!(g["4"]["inputs"]["text"], "masterpiece, best quality, 1girl, night sky");
        assert_eq!(g["6"]["class_type"], "EmptyLatentImage");
        assert_eq!((g["6"]["inputs"]["width"].clone(), g["6"]["inputs"]["height"].clone()), (json!(1024), json!(768)));
        assert_eq!(g["7"]["inputs"]["seed"], 42);
        assert_eq!(g["7"]["inputs"]["denoise"], 1.0);
        assert_eq!(g["9"]["class_type"], "ETN_SendImageWebSocket");
    }

    #[test]
    fn a_two_pass_picture_scales_up_then_samples_again() {
        let g = graph(&request(1.0), two_pass((1024, 768), (2048, 1536)), 42, &models()).unwrap();
        let json = g.to_json();
        assert_eq!(g.nodes_of("KSampler").len(), 2);
        assert_eq!(g.nodes_of("ImageScale").len(), 1);
        let empty = &g.nodes_of("EmptyLatentImage")[0];
        assert_eq!(
            (json[empty]["inputs"]["width"].clone(), json[empty]["inputs"]["height"].clone()),
            (json!(1024), json!(768))
        );
        let scale = &g.nodes_of("ImageScale")[0];
        assert_eq!(json[scale]["inputs"]["upscale_method"], "lanczos");
        assert_eq!(json[scale]["inputs"]["crop"], "disabled");
        assert_eq!(
            (json[scale]["inputs"]["width"].clone(), json[scale]["inputs"]["height"].clone()),
            (json!(2048), json!(1536))
        );
        let samplers = g.nodes_of("KSampler");
        assert_eq!(json[&samplers[0]]["inputs"]["denoise"], 1.0);
        assert_eq!(json[&samplers[1]]["inputs"]["denoise"], SECOND_PASS_DENOISE);
        assert_eq!(json[&samplers[1]]["inputs"]["seed"], 42);
    }

    #[test]
    fn refining_sends_the_picture_at_the_plans_size() {
        let g = graph(&request(0.5), one_pass(64, 32), 1, &models()).unwrap();
        let json = g.to_json();
        assert_eq!(g.nodes_of("KSampler").len(), 1, "refining is one pass");
        assert_eq!(g.nodes_of("ImageScale").len(), 0);
        assert_eq!(g.nodes_of("ETN_LoadImageBase64").len(), 1);
        assert_eq!(json["8"]["inputs"]["denoise"], 0.5);
        let png = procraft_engine::base64::decode(json["6"]["inputs"]["image"].as_str().unwrap()).unwrap();
        let sent = Picture::decode(&png).unwrap();
        assert_eq!((sent.width, sent.height), (64, 32));
    }

    #[test]
    fn each_selection_mode_prefills_as_designed() {
        for (mode, fill) in [
            (SelectionMode::Fill, "neutral"),
            (SelectionMode::Add, "neutral"),
            (SelectionMode::Remove, "telea"),
            (SelectionMode::Expand, "navier-stokes"),
        ] {
            let built = graph(&selection(mode), one_pass(64, 64), 42, &models()).unwrap();
            let g = built.to_json();
            let fills = built.nodes_of("INPAINT_MaskedFill");
            let node = &g[&fills[0]]["inputs"];
            assert_eq!(node["fill"], fill, "{mode:?}");
            assert_eq!(node["falloff"], 0);
        }
    }

    #[test]
    fn a_selection_goes_through_the_inpaint_control_and_back_through_the_mask() {
        let built = graph(&selection(SelectionMode::Fill), one_pass(64, 64), 42, &models()).unwrap();
        let g = built.to_json();
        assert_eq!(built.nodes_of("EmptyLatentImage").len(), 0);
        let samplers = built.nodes_of("KSampler");
        assert_eq!(samplers.len(), 1);
        assert_eq!(g[&samplers[0]]["inputs"]["denoise"], 0.75);
        let expand = built.nodes_of("INPAINT_ExpandMask");
        assert_eq!(expand.len(), 1);
        assert_eq!(g[&expand[0]]["inputs"]["grow"], 8, "2% of 64, at least 8");
        assert_eq!(g[&expand[0]]["inputs"]["blur"], 4, "half the growth");
        assert_eq!(g[&expand[0]]["inputs"]["blur_type"], "gaussian");
        let filled = built.nodes_of("INPAINT_MaskedFill");
        assert_eq!(filled.len(), 1);
        assert_eq!(g[&filled[0]]["inputs"]["mask"], json!([&expand[0], 0]));
        let control = built.nodes_of("ETN_control_load");
        assert_eq!(control.len(), 1);
        assert_eq!(g[&control[0]]["inputs"]["weights"], "anima-lllite-inpainting-v2.safetensors");
        let apply = built.nodes_of("ETN_control_apply");
        assert_eq!(apply.len(), 1);
        assert_eq!(g[&apply[0]]["inputs"]["strength"], INPAINT_CONTROL_STRENGTH);
        assert_eq!(g[&apply[0]]["inputs"]["mask"], json!([&expand[0], 0]));
        let noise = built.nodes_of("SetLatentNoiseMask");
        assert_eq!(noise.len(), 1);
        assert_eq!(g[&noise[0]]["inputs"]["mask"], json!([&expand[0], 0]));
        let composite = built.nodes_of("ImageCompositeMasked");
        assert_eq!(composite.len(), 1);
        assert_eq!(g[&composite[0]]["inputs"]["mask"], json!([&expand[0], 0]));
        let match_colours = built.nodes_of("INPAINT_ColorMatch");
        assert_eq!(match_colours.len(), 1);
        assert_eq!(g[&match_colours[0]]["inputs"]["strength"], COLOR_MATCH);
        assert_eq!(g[&match_colours[0]]["inputs"]["exclude_mask"], json!([&expand[0], 0]));
        let sent = built.nodes_of("ETN_SendImageWebSocket");
        assert_eq!(g[&sent[0]]["inputs"]["images"], json!([&match_colours[0], 0]));
    }

    #[test]
    fn removing_drops_the_prompt_and_keeps_objects_out() {
        let built = graph(&selection(SelectionMode::Remove), one_pass(64, 64), 42, &models()).unwrap();
        let g = built.to_json();
        let texts = built.nodes_of("CLIPTextEncode");
        assert_eq!(g[&texts[0]]["inputs"]["text"], "masterpiece, best quality");
        assert_eq!(
            g[&texts[1]]["inputs"]["text"],
            "worst quality, low quality, blurry, jpeg artifacts, watermark, signature, object, person, text"
        );
    }

    #[test]
    fn a_missing_vae_is_named() {
        let mut m = models();
        m.vaes.clear();
        let err = graph(&request(1.0), one_pass(64, 64), 1, &m).unwrap_err();
        assert_eq!(err.to_string(), "the server is missing qwen_image_vae.safetensors in models/vae");
    }
}
