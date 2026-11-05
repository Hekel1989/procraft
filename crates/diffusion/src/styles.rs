//! Styles: a checkpoint and how to run it (the prompt it's wrapped in, the negative prompt, the
//! sampler, steps and CFG). The built-in ones are in `styles/*.json`; every other checkpoint of a
//! family the server has becomes a style too, run as that family's first built-in style is.

use serde::Deserialize;

use crate::comfy::ServerModels;
use crate::family::{Family, matches};

/// A style ready to run on a server: its checkpoint is a file the server has.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub id: String,
    pub name: String,
    pub family: Family,
    /// The checkpoint's file, as the server lists it.
    pub checkpoint: String,
    /// The prompt wrapped around the user's: `{prompt}` is theirs, `{rating}` the rating tag.
    pub template: String,
    pub negative: String,
    pub sampler: String,
    pub scheduler: String,
    pub steps: u32,
    pub cfg: f32,
    /// Made from a checkpoint found on the server rather than built in.
    pub found: bool,
}

/// A built-in style as written: its checkpoint by file name patterns, best first.
#[derive(Clone, Debug, Deserialize)]
struct Written {
    id: String,
    name: String,
    family: Family,
    checkpoint: Vec<String>,
    positive: String,
    negative: String,
    sampler: String,
    scheduler: String,
    steps: u32,
    cfg: f32,
}

fn written() -> Vec<Written> {
    serde_json::from_str(include_str!("../styles/anima.json")).unwrap_or_default()
}

impl Written {
    fn with(&self, checkpoint: &str) -> Style {
        Style {
            id: self.id.clone(),
            name: self.name.clone(),
            family: self.family,
            checkpoint: checkpoint.to_owned(),
            template: self.positive.clone(),
            negative: self.negative.clone(),
            sampler: self.sampler.clone(),
            scheduler: self.scheduler.clone(),
            steps: self.steps,
            cfg: self.cfg,
            found: false,
        }
    }
}

impl Style {
    /// The built-in style `id`, with its checkpoint on the server; none if it hasn't got one.
    pub fn built_in(id: &str, models: &ServerModels) -> Option<Style> {
        let w = written().into_iter().find(|w| w.id == id)?;
        let file = w.checkpoint.iter().find_map(|p| models.diffusion_models.iter().find(|f| matches(p, f)))?;
        Some(w.with(file))
    }

    /// The prompt to send for `prompt`: the template filled in, the rating tag `safe` unless
    /// `nsfw`, and no empty items left where something was left out.
    pub fn positive(&self, prompt: &str, nsfw: bool) -> String {
        let filled =
            self.template.replace("{rating}", if nsfw { "" } else { "safe" }).replace("{prompt}", prompt.trim());
        filled.split(',').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ")
    }

    /// The negative prompt to send, with `extra` after the style's own: and unless `nsfw`, the
    /// ratings and words that keep pictures safe (the `safe` tag alone isn't enough for Anima,
    /// least of all with an empty or vague prompt).
    pub fn negative_for(&self, extra: &str, nsfw: bool) -> String {
        let safe = if nsfw { "" } else { SAFE_NEGATIVE };
        [self.negative.as_str(), extra, safe]
            .join(",")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Added to the negative prompt while NSFW is off.
const SAFE_NEGATIVE: &str =
    "nsfw, explicit, questionable, sensitive, nude, nudity, sexual, underwear, lingerie, cleavage";

/// The styles a server can run: the built-in ones whose checkpoint it has, then one for each other
/// checkpoint of the same families, named from its file.
pub fn styles_for(models: &ServerModels) -> Vec<Style> {
    let written = written();
    let mut styles: Vec<Style> = written.iter().filter_map(|w| Style::built_in(&w.id, models)).collect();
    for family in Family::ALL {
        let Some(base) = written.iter().find(|w| w.family == family) else { continue };
        for file in models.diffusion_models.iter().filter(|f| family.recognises(f)) {
            if styles.iter().any(|s| &s.checkpoint == file) {
                continue;
            }
            let mut style = base.with(file);
            style.id = format!("file:{file}");
            style.name = name_of(file);
            style.found = true;
            styles.push(style);
        }
    }
    styles
}

/// A checkpoint's file as a name: its folders and extension dropped, `_` as spaces.
fn name_of(file: &str) -> String {
    let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
    let name = name.strip_suffix(".safetensors").or_else(|| name.strip_suffix(".gguf")).unwrap_or(name);
    name.replace('_', " ")
}

#[cfg(test)]
mod tests {
    #[test]
    fn nsfw_off_keeps_the_negative_safe() {
        let style = super::written().into_iter().next().unwrap().with("x.safetensors");
        let off = style.negative_for("object", false);
        assert!(off.starts_with(&style.negative), "{off}");
        assert!(off.contains("object, nsfw"), "{off}");
        assert!(!style.negative_for("", true).contains("nsfw"));
    }

    use super::*;

    fn models(files: &[&str]) -> ServerModels {
        ServerModels { diffusion_models: files.iter().map(|&f| f.to_owned()).collect(), ..ServerModels::default() }
    }

    #[test]
    fn built_in_styles_find_their_checkpoints_and_the_rest_become_styles() {
        let styles = styles_for(&models(&[
            "Anima/anima_baseV10.safetensors",
            "anima_turboV10.safetensors",
            "Anima/divingAnima_v90.safetensors",
            "flux1-dev.safetensors",
        ]));
        let names: Vec<&str> = styles.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Anima", "Anima Turbo", "divingAnima v90"]);
        assert_eq!(styles[0].checkpoint, "Anima/anima_baseV10.safetensors");
        assert!(styles[2].found && styles[2].steps == 30);
    }

    #[test]
    fn no_built_in_style_without_its_checkpoint() {
        assert!(Style::built_in("anima-turbo", &models(&["anima_baseV10.safetensors"])).is_none());
    }

    #[test]
    fn the_prompt_is_wrapped_with_the_rating() {
        let s = Style::built_in("anima", &models(&["anima_baseV10.safetensors"])).unwrap();
        assert_eq!(s.positive(" 1girl, stars ", false), "masterpiece, best quality, safe, 1girl, stars");
        assert_eq!(s.positive("1girl", true), "masterpiece, best quality, 1girl");
        assert_eq!(s.positive("", false), "masterpiece, best quality, safe");
    }
}
