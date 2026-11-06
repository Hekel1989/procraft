//! Editing a style: its settings found by id, a copy of them, the file's name, and the file
//! written back. A file is edited in place with `toml_edit`, so what a person wrote by hand (its
//! comments, its order, keys this version doesn't know) is kept after an edit in the Style
//! Studio; a new style gets a short heading comment.

use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, Value};

use crate::family::Family;
use crate::styles::{StyleFile, StyleSettings, built_in_written, read};

/// The heading of a new style's file.
const HEADING: &str = "# A ProCraft Diffusion style: open it in any text editor to read or change it.\n";

/// The characters that can't be in a file's name on some system; each becomes `-`.
const NOT_IN_NAMES: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// The settings of style `id`: a built-in one, one found on the server (`file:<checkpoint>`), or one
/// of the person's (`user:<file name>`, from `files`), with every gap filled from its family's
/// first built-in style, so the Studio shows real values. None if there's no such style, or its
/// file can't be read.
pub fn settings_for(id: &str, files: &[StyleFile]) -> Option<StyleSettings> {
    let written = built_in_written();
    let base_of = |family: Family| written.iter().find(|(_, w)| w.family == family).map(|(_, w)| w.clone());
    let settings = if let Some(name) = id.strip_prefix("user:") {
        read(&files.iter().find(|f| f.name == name)?.text).ok()?
    } else if let Some(checkpoint) = id.strip_prefix("file:") {
        let mut found = base_of(Family::Anima)?;
        found.name = Some(crate::styles::name_of(checkpoint));
        found.models = vec![checkpoint.to_owned()];
        found
    } else {
        written.iter().find(|(i, _)| *i == id).map(|(_, w)| w.clone())?
    };
    let base = base_of(settings.family);
    Some(filled(settings, base.as_ref()))
}

/// `settings` with the sampler, scheduler, steps and CFG it leaves out taken from `base`.
fn filled(mut s: StyleSettings, base: Option<&StyleSettings>) -> StyleSettings {
    if let Some(b) = base {
        s.sampler = s.sampler.or_else(|| b.sampler.clone());
        s.scheduler = s.scheduler.or_else(|| b.scheduler.clone());
        s.steps = s.steps.or(b.steps);
        s.cfg = s.cfg.or(b.cfg);
    }
    s
}

/// A new style: "New style", with the settings of the first built-in one (a style needs a model
/// and a sampler to run), to change from there.
pub fn fresh() -> Option<StyleSettings> {
    let mut settings = settings_for("anima", &[])?;
    settings.name = Some("New style".to_owned());
    Some(settings)
}

/// A copy of `settings` to start a new style from: "<name> copy".
pub fn duplicate(settings: &StyleSettings) -> StyleSettings {
    let mut copy = settings.clone();
    copy.name = Some(format!("{} copy", settings.name.as_deref().unwrap_or("Style")));
    copy
}

/// The file for a style named `name` (`Moonlit.toml`): characters no file name can have become
/// `-`, and " 2", " 3"… is added when `taken` (the folder's other files) already has that name.
pub fn file_name(name: &str, taken: &[String]) -> String {
    let clean: String =
        name.trim().chars().map(|c| if NOT_IN_NAMES.contains(&c) || c.is_control() { '-' } else { c }).collect();
    let clean = clean.trim_matches(['.', ' ']).to_owned();
    let stem = if clean.is_empty() { "Style".to_owned() } else { clean };
    let free = |file: &str| !taken.iter().any(|t| t.eq_ignore_ascii_case(file));
    let first = format!("{stem}.toml");
    if free(&first) {
        return first;
    }
    (2..).map(|n| format!("{stem} {n}.toml")).find(|f| free(f)).unwrap_or(first)
}

/// The file's text for `settings`: `old` (the file as it is) edited in place when there is one and
/// it reads, else a new file. Numbers are rounded to what a person would write (0.7, not
/// 0.699999988).
pub fn write(settings: &StyleSettings, old: Option<&str>) -> String {
    let mut doc: DocumentMut = old.and_then(|t| t.parse().ok()).unwrap_or_else(|| {
        let mut doc = DocumentMut::new();
        doc.decor_mut().set_prefix(HEADING);
        doc
    });
    let table = doc.as_table_mut();
    set(table, "name", settings.name.as_deref().map(Value::from));
    set(table, "family", Some(Value::from(settings.family.name().to_lowercase())));
    let model = match settings.models.as_slice() {
        [one] => Value::from(one.as_str()),
        many => Value::Array(many.iter().map(String::as_str).collect()),
    };
    set(table, "model", Some(model));
    set(table, "prompt", Some(Value::from(settings.template.as_str())));
    set(table, "negative", Some(Value::from(settings.negative.as_str())));
    set(table, "sampler", settings.sampler.as_deref().map(Value::from));
    set(table, "scheduler", settings.scheduler.as_deref().map(Value::from));
    set(table, "steps", settings.steps.map(|s| Value::from(i64::from(s))));
    set(table, "cfg", settings.cfg.map(|c| Value::from(round(c))));
    set(table, "rescale_cfg", settings.rescale_cfg.map(|c| Value::from(round(c))));
    set(table, "vae", settings.vae.as_deref().map(Value::from));
    set(table, "text_encoder", settings.text_encoder.as_deref().map(Value::from));
    set(table, "best_size", settings.best_size.map(|s| Value::from(i64::from(s))));
    // the LoRAs: each kept section updated (its comments stay), new ones added, the rest gone
    let old_loras: Vec<Table> =
        table.get("lora").and_then(Item::as_array_of_tables).map(|a| a.iter().cloned().collect()).unwrap_or_default();
    let mut loras = ArrayOfTables::new();
    for (file, strength) in &settings.loras {
        let mut section = old_loras
            .iter()
            .find(|t| t.get("file").and_then(Item::as_str) == Some(file.as_str()))
            .cloned()
            .unwrap_or_default();
        set(&mut section, "file", Some(Value::from(file.as_str())));
        set(&mut section, "strength", Some(Value::from(round(*strength))));
        loras.push(section);
    }
    if loras.is_empty() {
        table.remove("lora");
    } else {
        table.insert("lora", Item::ArrayOfTables(loras));
    }
    // the LoRAs' sections after every plain key, as TOML wants
    doc.to_string()
}

/// Sets `key` to `value`, keeping the comments around it; none removes it.
fn set(table: &mut Table, key: &str, value: Option<Value>) {
    let Some(mut value) = value else {
        table.remove(key);
        return;
    };
    match table.get_mut(key).and_then(Item::as_value_mut) {
        Some(old) => {
            *value.decor_mut() = old.decor().clone();
            *old = value;
        }
        None => {
            table.insert(key, Item::Value(value));
        }
    }
}

/// A number as a person would write it: three decimals at most.
fn round(x: f32) -> f64 {
    (f64::from(x) * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, text: &str) -> StyleFile {
        StyleFile { name: name.to_owned(), text: text.to_owned() }
    }

    #[test]
    fn a_built_in_style_duplicates_into_a_new_file_that_reads_back() {
        let anima = settings_for("anima", &[]).unwrap();
        let mut copy = duplicate(&anima);
        assert_eq!(copy.name.as_deref(), Some("Anima copy"));
        copy.loras.push(("anima/dark_art.safetensors".into(), 0.7));
        copy.best_size = Some(1024);
        let text = write(&copy, None);
        assert!(text.starts_with("# A ProCraft Diffusion style"), "{text}");
        assert!(text.contains("model = [\"anima_base*\", \"anima-base*\"]"), "{text}");
        assert!(text.contains("strength = 0.7\n"), "{text}");
        assert_eq!(read(&text).unwrap(), copy);
    }

    #[test]
    fn an_edit_keeps_what_was_written_by_hand() {
        let old = "# my night style\nname = \"Moonlit\" # the name\nmodel = \"a.safetensors\"\n\n\
                   # keep this low\nsteps = 20\nmood = \"blue\"\n\n[[lora]]\n# the dark one\nfile = \"x.safetensors\"\nstrength = 0.5\n";
        let mut s = settings_for("user:Moonlit.toml", &[file("Moonlit.toml", old)]).unwrap();
        assert_eq!(s.sampler.as_deref(), Some("euler_ancestral"), "filled from the Anima style");
        s.steps = Some(28);
        s.loras[0].1 = 0.8;
        s.loras.push(("y.safetensors".into(), 1.0));
        s.rescale_cfg = None;
        let text = write(&s, Some(old));
        for kept in ["# my night style", "# the name", "# keep this low", "mood = \"blue\"", "# the dark one"] {
            assert!(text.contains(kept), "{kept} kept in:\n{text}");
        }
        assert!(text.contains("steps = 28"), "{text}");
        assert!(text.contains("strength = 0.8"), "{text}");
        assert!(!text.contains("rescale_cfg"));
        let back = read(&text).unwrap();
        assert_eq!(back.loras, [("x.safetensors".to_owned(), 0.8), ("y.safetensors".to_owned(), 1.0)]);
    }

    #[test]
    fn a_found_checkpoint_is_run_as_anima() {
        let s = settings_for("file:Anima/divingAnima_v90.safetensors", &[]).unwrap();
        assert_eq!(
            (s.name.as_deref(), s.models.as_slice(), s.steps),
            (Some("divingAnima v90"), &["Anima/divingAnima_v90.safetensors".to_owned()][..], Some(30))
        );
    }

    #[test]
    fn file_names_are_safe_and_unique() {
        let taken = vec!["Moonlit.toml".to_owned(), "moonlit 2.toml".to_owned()];
        assert_eq!(file_name("Moonlit", &taken), "Moonlit 3.toml");
        assert_eq!(file_name("Night: blue/grey?", &[]), "Night- blue-grey-.toml");
        assert_eq!(file_name("  ..  ", &[]), "Style.toml");
    }
}
