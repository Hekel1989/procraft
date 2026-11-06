//! What's kept about each LoRA on the server: its trigger words and the strength it's given when
//! added to a style. One TOML file for every LoRA, in ComfyUI's own user folder
//! (`user/default/procraft-diffusion/loras.toml`), read and written through its `userdata` API,
//! so every device using the server shares it:
//!
//! ```toml
//! ["anima/dark_art_style_(zura_janai).safetensors"]
//! trigger = "dark_art_style"
//! strength = 0.8
//! ```

use toml_edit::{DocumentMut, Item, Table, value};

/// The file's path under ComfyUI's user folder, as `userdata` takes it.
pub const NOTES_PATH: &str = "procraft-diffusion/loras.toml";

/// What's kept about one LoRA.
#[derive(Clone, Debug, PartialEq)]
pub struct LoraNote {
    /// Its trigger words, comma-separated; empty: none.
    pub trigger: String,
    /// The strength it starts at in a style; none: 1.
    pub strength: Option<f32>,
}

/// The server's notes on its LoRAs, by file as the server lists it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoraNotes {
    notes: Vec<(String, LoraNote)>,
    /// Why the file couldn't be read, if it couldn't: it's then left alone rather than written over.
    pub problem: Option<String>,
}

impl LoraNotes {
    /// The notes in `text`; none, with the problem, if it can't be read. Entries that aren't
    /// tables are passed over.
    pub fn read(text: &str) -> LoraNotes {
        let doc: DocumentMut = match text.parse() {
            Ok(doc) => doc,
            Err(e) => return LoraNotes { notes: Vec::new(), problem: Some(crate::styles::plain_error(text, &e)) },
        };
        let notes = doc
            .iter()
            .filter_map(|(file, item)| {
                let table = item.as_table_like()?;
                let trigger = table.get("trigger").and_then(Item::as_str).unwrap_or_default().to_owned();
                let strength = table
                    .get("strength")
                    .and_then(|s| s.as_float().or_else(|| s.as_integer().map(|i| i as f64)))
                    .map(|s| s as f32)
                    .filter(|s| s.is_finite());
                Some((file.to_owned(), LoraNote { trigger, strength }))
            })
            .collect();
        LoraNotes { notes, problem: None }
    }

    /// The note on `file`, matched as the server lists it, or by its name in another folder.
    pub fn get(&self, file: &str) -> Option<&LoraNote> {
        let name = |f: &str| f.rsplit(['/', '\\']).next().unwrap_or(f).to_lowercase();
        self.notes
            .iter()
            .find(|(f, _)| f == file)
            .or_else(|| self.notes.iter().find(|(f, _)| name(f) == name(file)))
            .map(|(_, n)| n)
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }
}

/// The heading of a new notes file.
const HEADING: &str = "# Trigger words and default strengths for LoRAs, kept by ProCraft Diffusion.\n\
# One entry per LoRA file, as ComfyUI names it. Edit freely.\n\n";

/// The notes file's text with each `(file, trigger words)` of `changes` set: `old` (the file as it
/// is) edited in place, so other entries and comments stay; a new file when there's none. An entry
/// whose words are emptied keeps its other keys (its strength) and loses `trigger`. None if `old`
/// can't be read: a file someone wrote by hand isn't written over.
pub fn write_notes(old: Option<&str>, changes: &[(String, String)]) -> Option<String> {
    let mut doc: DocumentMut = match old {
        Some(text) if !text.trim().is_empty() => text.parse().ok()?,
        _ => {
            let mut doc = DocumentMut::new();
            doc.decor_mut().set_prefix(HEADING);
            doc
        }
    };
    for (file, trigger) in changes {
        let trigger = trigger.trim();
        if !doc.contains_key(file) {
            if trigger.is_empty() {
                continue;
            }
            doc.insert(file, Item::Table(Table::new()));
        }
        let Some(table) = doc.get_mut(file).and_then(Item::as_table_mut) else { continue };
        if trigger.is_empty() {
            table.remove("trigger");
        } else {
            table.insert("trigger", value(trigger));
        }
    }
    Some(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_are_read_by_file() {
        let notes = LoraNotes::read(
            "# kept by ProCraft Diffusion\n\n[\"anima/dark_art.safetensors\"]\ntrigger = \"dark_art_style\"\nstrength = 0.8\n\n\
             [\"anima/dragon.safetensors\"]\ntrigger = \"@dragonball\"\nstrength = 1\n\nstray = 3\n",
        );
        assert_eq!(notes.problem, None);
        let dark = notes.get("anima/dark_art.safetensors").unwrap();
        assert_eq!((dark.trigger.as_str(), dark.strength), ("dark_art_style", Some(0.8)));
        assert_eq!(notes.get("moved/dragon.safetensors").unwrap().strength, Some(1.0));
        assert!(notes.get("stray").is_none());
    }

    #[test]
    fn notes_are_written_in_place() {
        let old = "# mine\n[\"a.safetensors\"]\ntrigger = \"old\"\nstrength = 0.8 # keep\n\n[\"b.safetensors\"]\ntrigger = \"bee\"\n";
        let changes = [
            ("a.safetensors".to_owned(), "new, words".to_owned()),
            ("b.safetensors".to_owned(), String::new()),
            ("anima/c.safetensors".to_owned(), "sea".to_owned()),
        ];
        let text = write_notes(Some(old), &changes).unwrap();
        assert!(text.starts_with("# mine"), "{text}");
        assert!(text.contains("strength = 0.8 # keep"), "{text}");
        let notes = LoraNotes::read(&text);
        assert_eq!(notes.get("a.safetensors").unwrap().trigger, "new, words");
        assert_eq!(notes.get("b.safetensors").unwrap().trigger, "");
        assert_eq!(notes.get("anima/c.safetensors").unwrap().trigger, "sea");
        let fresh = write_notes(None, &changes[2..]).unwrap();
        assert!(fresh.starts_with("# Trigger words"), "{fresh}");
        assert!(write_notes(Some("[\"a\"\n"), &changes).is_none(), "a broken file is left alone");
    }

    #[test]
    fn a_file_that_cant_be_read_is_noted() {
        let notes = LoraNotes::read("[\"a\"\ntrigger = ");
        assert!(notes.is_empty() && notes.problem.is_some());
    }
}
