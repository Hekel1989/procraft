//! Styles: a checkpoint and how to run it (the prompt it's wrapped in, the negative prompt, the
//! sampler, steps and CFG, its LoRAs and the rarer settings). Every style is a small TOML file a
//! person can read and edit: the built-in ones are compiled in from `styles/*.toml`, the person's
//! own come from their styles folder (the apps read the folder and hand the files' text over), and
//! every other checkpoint of a family the server has becomes a style too, run as that family's
//! first built-in style is.

use toml_edit::{DocumentMut, Item, Value};

use crate::comfy::ServerModels;
use crate::family::{Family, matches};
use crate::loras::LoraNotes;
use crate::requirements::Missing;

/// Where a style comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Comes with the app: read-only, duplicated to be changed.
    BuiltIn,
    /// Made from a checkpoint found on the server, run as its family's first built-in style.
    Found,
    /// The person's own, from a file in their styles folder.
    Yours,
}

/// A LoRA a style uses, with its strength (1 is as trained) and the trigger words kept for it on
/// the server, if any.
#[derive(Clone, Debug, PartialEq)]
pub struct LoraUse {
    /// The file as the server lists it, folders included (`anima/dark_art_style.safetensors`).
    pub file: String,
    pub strength: f32,
    /// The words that make it take effect, as written (comma-separated); empty: none kept.
    pub trigger: String,
}

/// What a style needs that the server hasn't got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Model,
    Lora,
    Vae,
    TextEncoder,
}

impl Part {
    /// The folder under `models/` it's looked for in.
    pub fn folder(self) -> &'static str {
        match self {
            Part::Model => "diffusion_models",
            Part::Lora => "loras",
            Part::Vae => "vae",
            Part::TextEncoder => "text_encoders",
        }
    }
}

/// A style as the apps show it and a run uses it.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub id: String,
    pub name: String,
    pub family: Family,
    /// The checkpoint's file as the server lists it, or as written when the server hasn't got it.
    pub checkpoint: String,
    /// The prompt wrapped around the person's: `{prompt}` is theirs (at the end when it's left out).
    pub template: String,
    pub negative: String,
    pub sampler: String,
    pub scheduler: String,
    pub steps: u32,
    pub cfg: f32,
    pub loras: Vec<LoraUse>,
    /// `RescaleCFG`'s multiplier, when overridden.
    pub rescale_cfg: Option<f32>,
    /// A VAE other than the family's own.
    pub vae: Option<String>,
    /// A text encoder other than the family's own.
    pub text_encoder: Option<String>,
    /// One side, in pixels, of the size the model makes pictures at, when overridden (Krita's
    /// "Preferred Resolution").
    pub best_size: Option<u32>,
    pub kind: Kind,
    /// What the server hasn't got, each with the file as written.
    pub missing: Vec<(Part, String)>,
    /// Why the file couldn't be read, when it couldn't; the style is then only shown.
    pub problem: Option<String>,
}

/// One file of the person's styles folder: its name (`Moonlit.toml`) and its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleFile {
    pub name: String,
    pub text: String,
}

/// The built-in styles, with their ids.
const BUILT_IN: [(&str, &str); 2] =
    [("anima", include_str!("../styles/anima.toml")), ("anima-turbo", include_str!("../styles/anima-turbo.toml"))];

/// The best size's range, as Krita's style editor has it, and its step.
pub const BEST_SIZE_MIN: u32 = 256;
pub const BEST_SIZE_MAX: u32 = 2048;

/// A style as written in its file, before it's matched to a server: what the Style Studio edits.
#[derive(Clone, Debug, PartialEq)]
pub struct StyleSettings {
    pub name: Option<String>,
    pub family: Family,
    /// The checkpoint by file name or pattern, best first.
    pub models: Vec<String>,
    pub template: String,
    pub negative: String,
    pub sampler: Option<String>,
    pub scheduler: Option<String>,
    pub steps: Option<u32>,
    pub cfg: Option<f32>,
    /// Each LoRA's file and strength.
    pub loras: Vec<(String, f32)>,
    pub rescale_cfg: Option<f32>,
    pub vae: Option<String>,
    pub text_encoder: Option<String>,
    pub best_size: Option<u32>,
}

/// Reads a style's text; the reason it can't be, with the line, when it can't.
pub fn read(text: &str) -> Result<StyleSettings, String> {
    let doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| plain_error(text, &e))?;
    let string = |key: &str| -> Result<Option<String>, String> {
        match doc.get(key) {
            None => Ok(None),
            Some(item) => {
                item.as_str().map(|s| Some(s.to_owned())).ok_or_else(|| format!("`{key}` must be text in quotes"))
            }
        }
    };
    let number = |item: &Item, key: &str| -> Result<f32, String> {
        item.as_float()
            .map(|f| f as f32)
            .or_else(|| item.as_integer().map(|i| i as f32))
            .filter(|f| f.is_finite())
            .ok_or_else(|| format!("`{key}` must be a number"))
    };
    let whole = |key: &str| -> Result<Option<u32>, String> {
        match doc.get(key) {
            None => Ok(None),
            Some(item) => item
                .as_integer()
                .and_then(|i| u32::try_from(i).ok())
                .map(Some)
                .ok_or_else(|| format!("`{key}` must be a whole number")),
        }
    };
    let family = match string("family")? {
        None => Family::Anima,
        Some(name) => Family::ALL
            .into_iter()
            .find(|f| f.name().eq_ignore_ascii_case(&name))
            .ok_or_else(|| format!("ProCraft Diffusion doesn't know the family \"{name}\" yet"))?,
    };
    let models = match doc.get("model") {
        None => return Err("`model` is missing: the checkpoint's file, as the server lists it".to_owned()),
        Some(item) => match (item.as_str(), item.as_array()) {
            (Some(one), _) => vec![one.to_owned()],
            (None, Some(list)) => list.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
            _ => return Err("`model` must be a file name in quotes".to_owned()),
        },
    };
    if models.iter().all(|m| m.trim().is_empty()) {
        return Err("`model` is empty".to_owned());
    }
    let mut loras = Vec::new();
    if let Some(item) = doc.get("lora") {
        let tables = item.as_array_of_tables().ok_or_else(|| "each LoRA must be a `[[lora]]` section".to_owned())?;
        for table in tables {
            let file = table
                .get("file")
                .and_then(Item::as_str)
                .filter(|f| !f.trim().is_empty())
                .ok_or_else(|| "a `[[lora]]` section needs its `file`".to_owned())?;
            let strength = match table.get("strength") {
                None => 1.0,
                Some(item) => number(item, "strength")?,
            };
            loras.push((file.to_owned(), strength));
        }
    }
    let optional = |key: &str| -> Result<Option<f32>, String> { doc.get(key).map(|i| number(i, key)).transpose() };
    Ok(StyleSettings {
        name: string("name")?.filter(|n| !n.trim().is_empty()),
        family,
        models,
        template: string("prompt")?.unwrap_or_else(|| "{prompt}".to_owned()),
        negative: string("negative")?.unwrap_or_default(),
        sampler: string("sampler")?,
        scheduler: string("scheduler")?,
        steps: whole("steps")?,
        cfg: optional("cfg")?,
        loras,
        rescale_cfg: optional("rescale_cfg")?,
        vae: string("vae")?,
        text_encoder: string("text_encoder")?,
        best_size: whole("best_size")?.map(|s| s.clamp(BEST_SIZE_MIN, BEST_SIZE_MAX)),
    })
}

/// A TOML mistake in one line a person can read: where it is, then what's wrong ("Line 1, column
/// 13: invalid basic string").
pub(crate) fn plain_error(text: &str, e: &toml_edit::TomlError) -> String {
    let message = e.message().trim().replace('\n', " ");
    let Some(at) = e.span().map(|s| s.start.min(text.len())) else { return message };
    let before = text.get(..at).unwrap_or_default();
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or_default().chars().count() + 1;
    format!("Line {line}, column {column}: {message}")
}

/// The built-in styles as written. They're tested, so they always read.
pub(crate) fn built_in_written() -> Vec<(&'static str, StyleSettings)> {
    BUILT_IN.iter().filter_map(|(id, text)| read(text).ok().map(|w| (*id, w))).collect()
}

/// The file on the server for `wanted`: the same path, else the same file in another folder, else
/// the first a pattern (with `*`) matches.
pub fn find(wanted: &str, files: &[String]) -> Option<String> {
    let wanted = wanted.trim();
    let name = |f: &str| f.rsplit(['/', '\\']).next().unwrap_or(f).to_lowercase();
    files
        .iter()
        .find(|f| {
            f.eq_ignore_ascii_case(wanted) || f.replace('\\', "/").eq_ignore_ascii_case(&wanted.replace('\\', "/"))
        })
        .or_else(|| files.iter().find(|f| name(f) == name(wanted)))
        .or_else(|| wanted.contains('*').then(|| files.iter().find(|f| matches(wanted, f))).flatten())
        .cloned()
}

impl StyleSettings {
    /// The style it makes on a server with `models`, with the gaps of `base` (its family's first
    /// built-in style) filled in and its LoRAs' trigger words from `notes`.
    pub fn style(
        &self,
        id: String,
        name: String,
        kind: Kind,
        base: Option<&StyleSettings>,
        models: &ServerModels,
        notes: &LoraNotes,
    ) -> Style {
        let mut missing = Vec::new();
        let checkpoint = match self.models.iter().find_map(|m| find(m, &models.diffusion_models)) {
            Some(file) => file,
            None => {
                let written = self.models.first().cloned().unwrap_or_default();
                missing.push((Part::Model, written.clone()));
                written
            }
        };
        let mut look_up = |file: &Option<String>, part: Part, choices: &[String]| -> Option<String> {
            let file = file.as_ref()?;
            Some(find(file, choices).unwrap_or_else(|| {
                missing.push((part, file.clone()));
                file.clone()
            }))
        };
        let vae = look_up(&self.vae, Part::Vae, &models.vaes);
        let text_encoder = look_up(&self.text_encoder, Part::TextEncoder, &models.text_encoders);
        let loras = self
            .loras
            .iter()
            .map(|(file, strength)| {
                let found = find(file, &models.loras);
                if found.is_none() {
                    missing.push((Part::Lora, file.clone()));
                }
                let file = found.unwrap_or_else(|| file.clone());
                let trigger = notes.get(&file).map(|n| n.trigger.clone()).unwrap_or_default();
                LoraUse { file, strength: *strength, trigger }
            })
            .collect();
        let or_base = |own: &Option<String>, of: fn(&StyleSettings) -> &Option<String>, fallback: &str| {
            own.clone().or_else(|| base.and_then(|b| of(b).clone())).unwrap_or_else(|| fallback.to_owned())
        };
        Style {
            id,
            name,
            family: self.family,
            checkpoint,
            template: self.template.clone(),
            negative: self.negative.clone(),
            sampler: or_base(&self.sampler, |b| &b.sampler, "euler_ancestral"),
            scheduler: or_base(&self.scheduler, |b| &b.scheduler, "normal"),
            steps: self.steps.or_else(|| base.and_then(|b| b.steps)).unwrap_or(30).clamp(1, 200),
            cfg: self.cfg.or_else(|| base.and_then(|b| b.cfg)).unwrap_or(4.0),
            loras,
            rescale_cfg: self.rescale_cfg,
            vae,
            text_encoder,
            best_size: self.best_size,
            kind,
            missing,
            problem: None,
        }
    }
}

impl Style {
    /// The built-in style `id`, with its checkpoint on the server; none if it hasn't got one.
    pub fn built_in(id: &str, models: &ServerModels) -> Option<Style> {
        let (id, w) = built_in_written().into_iter().find(|(i, _)| *i == id)?;
        let style = w.style(
            id.to_owned(),
            w.name.clone().unwrap_or_default(),
            Kind::BuiltIn,
            None,
            models,
            &LoraNotes::default(),
        );
        style.missing.is_empty().then_some(style)
    }

    /// Whether it was made from a checkpoint found on the server rather than written.
    pub fn found(&self) -> bool {
        self.kind == Kind::Found
    }

    /// The prompt to send for `prompt`: the template filled in, then the trigger words of its
    /// LoRAs that the prompt hasn't got yet, with no empty items left where something was left
    /// out. What is generated is up to the person: the NSFW switch only chooses which tags are
    /// suggested, so no rating tag is added here.
    pub fn positive(&self, prompt: &str) -> String {
        let key = |tag: &str| tag.trim().to_lowercase().replace('_', " ");
        let mut theirs: Vec<String> =
            prompt.split(',').map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned).collect();
        for lora in &self.loras {
            for word in lora.trigger.split(',').map(str::trim).filter(|w| !w.is_empty()) {
                if !theirs.iter().any(|t| key(t) == key(word)) {
                    theirs.push(word.to_owned());
                }
            }
        }
        let prompt = theirs.join(", ");
        let filled = if self.template.contains("{prompt}") {
            self.template.replace("{prompt}", &prompt)
        } else {
            format!("{}, {prompt}", self.template)
        };
        filled.split(',').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ")
    }

    /// The model's own size for this style: its best size squared when overridden, else its
    /// family's.
    pub fn model_size(&self) -> crate::plan::ModelSize {
        match self.best_size {
            Some(side) => crate::plan::ModelSize::best(side),
            None => crate::plan::ModelSize::family(self.family.native_pixels()),
        }
    }

    /// What the server hasn't got, as the run's error lists it.
    pub fn missing_files(&self) -> Vec<Missing> {
        self.missing.iter().map(|(part, file)| Missing::Model { what: file.clone(), folder: part.folder() }).collect()
    }
}

/// The styles a server can run, in the order the apps show them: the person's own (by name), then
/// the built-in ones whose checkpoint it has, then one for each other checkpoint of the same
/// families, named from its file. A file of theirs that can't be read is kept, with its problem;
/// one whose model or LoRAs the server hasn't got is kept, with what's missing.
pub fn styles_for(models: &ServerModels, files: &[StyleFile], notes: &LoraNotes) -> Vec<Style> {
    let written = built_in_written();
    let base_of = |family: Family| written.iter().find(|(_, w)| w.family == family).map(|(_, w)| w);
    let mut yours: Vec<Style> = files
        .iter()
        .map(|file| {
            let id = format!("user:{}", file.name);
            let from_name = name_of(&file.name);
            match read(&file.text) {
                Ok(w) => {
                    let name = w.name.clone().unwrap_or(from_name);
                    w.style(id, name, Kind::Yours, base_of(w.family), models, notes)
                }
                Err(problem) => unreadable(id, from_name, problem),
            }
        })
        .collect();
    yours.sort_by_key(|s| s.name.to_lowercase());
    let mut styles = yours;
    for (id, w) in &written {
        let style = w.style((*id).to_owned(), w.name.clone().unwrap_or_default(), Kind::BuiltIn, None, models, notes);
        if style.missing.is_empty() {
            styles.push(style);
        }
    }
    for family in Family::ALL {
        let Some(base) = base_of(family) else { continue };
        for file in models.diffusion_models.iter().filter(|f| family.recognises(f)) {
            if styles.iter().any(|s| s.kind != Kind::Yours && &s.checkpoint == file) {
                continue;
            }
            let mut found = base.clone();
            found.models = vec![file.clone()];
            styles.push(found.style(format!("file:{file}"), name_of(file), Kind::Found, Some(base), models, notes));
        }
    }
    styles
}

/// A file of the person's that can't be read: shown with its problem, never run.
fn unreadable(id: String, name: String, problem: String) -> Style {
    Style {
        id,
        name,
        family: Family::Anima,
        checkpoint: String::new(),
        template: "{prompt}".to_owned(),
        negative: String::new(),
        sampler: String::new(),
        scheduler: String::new(),
        steps: 0,
        cfg: 0.0,
        loras: Vec::new(),
        rescale_cfg: None,
        vae: None,
        text_encoder: None,
        best_size: None,
        kind: Kind::Yours,
        missing: Vec::new(),
        problem: Some(problem),
    }
}

/// A file as a name: its folders and extension dropped, `_` as spaces.
pub fn name_of(file: &str) -> String {
    let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
    let name = [".safetensors", ".gguf", ".toml"].iter().find_map(|e| name.strip_suffix(e)).unwrap_or(name);
    name.replace('_', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn models(files: &[&str]) -> ServerModels {
        ServerModels { diffusion_models: files.iter().map(|&f| f.to_owned()).collect(), ..ServerModels::default() }
    }

    fn with_loras(mut m: ServerModels, loras: &[&str]) -> ServerModels {
        m.loras = loras.iter().map(|&f| f.to_owned()).collect();
        m
    }

    fn file(name: &str, text: &str) -> StyleFile {
        StyleFile { name: name.to_owned(), text: text.to_owned() }
    }

    #[test]
    fn the_built_in_styles_read() {
        let written = built_in_written();
        assert_eq!(written.len(), BUILT_IN.len());
        assert_eq!(written[0].1.name.as_deref(), Some("Anima"));
        assert_eq!(written[1].1.steps, Some(8));
    }

    #[test]
    fn built_in_styles_find_their_checkpoints_and_the_rest_become_styles() {
        let styles = styles_for(
            &models(&[
                "Anima/anima_baseV10.safetensors",
                "anima_turboV10.safetensors",
                "Anima/divingAnima_v90.safetensors",
                "flux1-dev.safetensors",
            ]),
            &[],
            &LoraNotes::default(),
        );
        let names: Vec<&str> = styles.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Anima", "Anima Turbo", "divingAnima v90"]);
        assert_eq!(styles[0].checkpoint, "Anima/anima_baseV10.safetensors");
        assert!(styles[2].found() && styles[2].steps == 30);
    }

    #[test]
    fn the_companions_file_names_are_found_too() {
        let styles = styles_for(&models(&["anima-base-v1.0.safetensors"]), &[], &LoraNotes::default());
        assert_eq!(styles[0].id, "anima");
    }

    #[test]
    fn no_built_in_style_without_its_checkpoint() {
        assert!(Style::built_in("anima-turbo", &models(&["anima_baseV10.safetensors"])).is_none());
    }

    #[test]
    fn the_prompt_is_wrapped_in_the_template() {
        let s = Style::built_in("anima", &models(&["anima_baseV10.safetensors"])).unwrap();
        assert_eq!(s.positive(" 1girl, stars "), "masterpiece, best quality, 1girl, stars");
        assert_eq!(s.positive(""), "masterpiece, best quality");
    }

    #[test]
    fn your_styles_come_first_with_their_loras_and_what_is_missing() {
        let server = with_loras(models(&["anima_baseV10.safetensors"]), &["anima/dark_art.safetensors"]);
        let moonlit = file(
            "Moonlit.toml",
            "# a comment\nname = \"Moonlit\"\nmodel = \"anima_baseV10.safetensors\"\nprompt = \"{prompt}, night\"\n\
             steps = 24\ncfg = 4\nbest_size = 1024\n\n[[lora]]\nfile = \"dark_art.safetensors\"\nstrength = 0.7\n\n\
             [[lora]]\nfile = \"anima/gone.safetensors\"\n",
        );
        let other = file("aurora.toml", "model = \"elsewhere.safetensors\"\n");
        let styles = styles_for(&server, &[moonlit, other], &LoraNotes::default());
        let names: Vec<&str> = styles.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["aurora", "Moonlit", "Anima"]);
        let moonlit = &styles[1];
        assert_eq!((moonlit.id.as_str(), moonlit.kind), ("user:Moonlit.toml", Kind::Yours));
        assert_eq!((moonlit.steps, moonlit.cfg, moonlit.best_size), (24, 4.0, Some(1024)));
        // what it leaves out is the Anima style's
        assert_eq!(moonlit.sampler, "euler_ancestral");
        assert_eq!(moonlit.loras[0].file, "anima/dark_art.safetensors");
        assert_eq!(moonlit.loras[1].strength, 1.0);
        assert_eq!(moonlit.missing, [(Part::Lora, "anima/gone.safetensors".to_owned())]);
        assert_eq!(styles[0].missing, [(Part::Model, "elsewhere.safetensors".to_owned())]);
    }

    #[test]
    fn a_file_with_a_mistake_is_kept_with_its_problem() {
        let styles = styles_for(
            &models(&["anima_baseV10.safetensors"]),
            &[
                file("Broken.toml", "name = \"Broken\nmodel = 3"),
                file("No model.toml", "name = \"x\""),
                file("Odd.toml", "model = \"a\"\nfamily = \"flux\""),
                file("Steps.toml", "model = \"a\"\nsteps = \"many\""),
            ],
            &LoraNotes::default(),
        );
        let problems: Vec<&str> = styles.iter().filter_map(|s| s.problem.as_deref()).collect();
        assert_eq!(problems.len(), 4);
        assert!(problems[0].starts_with("Line 1, column 15: invalid basic string"), "{}", problems[0]);
        assert!(problems[1].contains("`model` is missing"));
        assert!(problems[2].contains("\"flux\""));
        assert!(problems[3].contains("`steps`"));
    }

    #[test]
    fn trigger_words_go_after_the_prompt_once() {
        let server = with_loras(models(&["anima_baseV10.safetensors"]), &["anima/dark_art.safetensors"]);
        let notes = LoraNotes::read("[\"anima/dark_art.safetensors\"]\ntrigger = \"dark_art_style, glowing\"\n");
        let moonlit = file(
            "M.toml",
            "model = \"anima_baseV10.safetensors\"\nprompt = \"best, {prompt}\"\n[[lora]]\nfile = \"anima/dark_art.safetensors\"",
        );
        let style = styles_for(&server, &[moonlit], &notes).remove(0);
        assert_eq!(style.positive("a castle, dark art style"), "best, a castle, dark art style, glowing");
        assert_eq!(style.positive(""), "best, dark_art_style, glowing");
    }

    #[test]
    fn a_template_without_the_prompt_takes_it_at_the_end() {
        let style = styles_for(
            &models(&["a.safetensors"]),
            &[file("x.toml", "model = \"a.safetensors\"\nprompt = \"oil painting\"")],
            &LoraNotes::default(),
        )
        .remove(0);
        assert_eq!(style.positive("a boat"), "oil painting, a boat");
    }
}
