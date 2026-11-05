//! Control lines from a picture, made on the server: the visible picture through one of its line
//! extractors, back as white lines on black. Extraction needs no model family. The extractors
//! come from the `comfyui_controlnet_aux` add-on.

use serde_json::{Value, json};

use crate::{ControlKind, Error, Graph, Picture};

/// The smallest resolution an extractor is asked for, and the largest.
const LEAST_RESOLUTION: u32 = 512;
const MOST_RESOLUTION: u32 = 2048;
/// Extractors work at multiples of this.
const STEP: u32 = 64;

/// The resolution to ask `kind`'s extractor for: the picture's shorter side rounded to 64, kept
/// between 512 and 2048.
pub fn resolution(width: u32, height: u32) -> u32 {
    let shorter = f64::from(width.min(height));
    let rounded = (shorter / f64::from(STEP)).round() as u32 * STEP;
    rounded.clamp(LEAST_RESOLUTION, MOST_RESOLUTION)
}

/// The extractor for `kind`: its node and the inputs besides its picture and resolution.
fn extractor(kind: ControlKind) -> (&'static str, &'static [(&'static str, &'static str)]) {
    match kind {
        ControlKind::LineArt => ("AnimeLineArtPreprocessor", &[]),
        ControlKind::Scribble => ("Scribble_PiDiNet_Preprocessor", &[("safe", "enable")]),
    }
}

/// The graph that runs `picture` through `kind`'s extractor and sends the lines back as a PNG.
pub fn graph(kind: ControlKind, picture: &Picture) -> Result<Graph, Error> {
    let png = picture.to_png()?;
    let mut g = Graph::new();
    let image = g.add("ETN_LoadImageBase64", [("image", json!(procraft_engine::base64::encode(&png)))]);
    let (class, extra) = extractor(kind);
    let mut inputs: Vec<(&str, Value)> =
        vec![("image", image.out(0).into()), ("resolution", json!(resolution(picture.width, picture.height)))];
    inputs.extend(extra.iter().map(|&(name, value)| (name, json!(value))));
    let lines = g.add(class, inputs);
    g.add("ETN_SendImageWebSocket", [("images", lines.out(0).into()), ("format", json!("PNG"))]);
    Ok(g)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(width: u32, height: u32) -> Picture {
        Picture::new(width, height, vec![255; width as usize * height as usize * 4]).unwrap()
    }

    #[test]
    fn the_extractor_resolution_is_the_short_side_rounded_between_the_limits() {
        assert_eq!(resolution(700, 1024), 704);
        assert_eq!(resolution(1024, 700), 704);
        assert_eq!(resolution(720, 1080), 704);
        assert_eq!(resolution(760, 1080), 768);
        assert_eq!(resolution(300, 300), 512);
        assert_eq!(resolution(4000, 3000), 2048);
        assert_eq!(resolution(512, 512), 512);
        assert_eq!(resolution(1080, 1920), 1088);
    }

    #[test]
    fn line_art_goes_through_the_line_extractor() {
        let g = graph(ControlKind::LineArt, &picture(700, 1024)).unwrap();
        let json = g.to_json();
        assert_eq!(g.nodes_of("ETN_LoadImageBase64").len(), 1);
        let extractor = g.nodes_of("AnimeLineArtPreprocessor");
        assert_eq!(extractor.len(), 1);
        assert_eq!(json[&extractor[0]]["inputs"]["resolution"], 704);
        assert!(json[&extractor[0]]["inputs"].get("safe").is_none());
        let sent = g.nodes_of("ETN_SendImageWebSocket");
        assert_eq!(json[&sent[0]]["inputs"]["format"], "PNG");
        let load = &g.nodes_of("ETN_LoadImageBase64")[0];
        let png = procraft_engine::base64::decode(json[load]["inputs"]["image"].as_str().unwrap()).unwrap();
        let sent = Picture::decode(&png).unwrap();
        assert_eq!((sent.width, sent.height), (700, 1024), "the picture goes at its own size");
    }

    #[test]
    fn scribble_goes_through_the_pidinet_extractor_with_safety_on() {
        let g = graph(ControlKind::Scribble, &picture(1024, 700)).unwrap();
        let json = g.to_json();
        let extractor = g.nodes_of("Scribble_PiDiNet_Preprocessor");
        assert_eq!(extractor.len(), 1);
        assert_eq!(json[&extractor[0]]["inputs"]["resolution"], 704);
        assert_eq!(json[&extractor[0]]["inputs"]["safe"], "enable");
    }
}
