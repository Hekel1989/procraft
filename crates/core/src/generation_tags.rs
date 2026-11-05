//! Prompt autocompletion and recent prompts, exported beside [`Generation`](crate::Generation).
//!
//! The tag search itself lives in `procraft-diffusion::tags`; here it takes the current NSFW
//! setting, caps the list for the popover, and speaks the units the apps use. Offsets in
//! [`TagSuggestion`] and [`TagCompletion`] are **UTF-16 code units**, not bytes, so Kotlin's
//! `String` indices work unchanged; desktop converts them to its UTF-8 byte offsets.
//!
//! Recent prompts are app-wide, newest first, at most twenty. The apps load what they kept at
//! start with [`Generation::set_recent_prompts`], and every run of [`Generation::generate`] adds
//! its prompt through [`remember`].

use std::sync::Mutex;

use procraft_diffusion::tags::{self, TagKind as DiffusionKind};

use crate::generation::Generation;

/// How many suggestions the popover is offered.
const MOST_SUGGESTIONS: usize = 12;
/// How many recent prompts are kept.
const MOST_RECENT: usize = 20;

/// The tag list's categories, as the apps show them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum TagKind {
    General,
    Artist,
    Copyright,
    Character,
    Meta,
}

impl From<DiffusionKind> for TagKind {
    fn from(kind: DiffusionKind) -> TagKind {
        match kind {
            DiffusionKind::General => TagKind::General,
            DiffusionKind::Artist => TagKind::Artist,
            DiffusionKind::Copyright => TagKind::Copyright,
            DiffusionKind::Character => TagKind::Character,
            DiffusionKind::Meta => TagKind::Meta,
        }
    }
}

/// A tag the prompt can be completed with.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TagSuggestion {
    /// The tag as inserted, with spaces for underscores.
    pub tag: String,
    pub kind: TagKind,
    /// A short count for the chip ("12k", "1.2M", "850").
    pub count: String,
    /// The UTF-16 range of the prompt this replaces.
    pub from: u32,
    pub to: u32,
}

/// A prompt after a suggestion was inserted.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct TagCompletion {
    pub text: String,
    /// The caret's UTF-16 offset in `text`.
    pub caret: u32,
}

/// The recent prompts, newest first, shared by the app's one [`Generation`].
static RECENT: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[uniffi::export]
impl Generation {
    /// The tags matching the word being typed at `caret` (a UTF-16 offset), best first, at most
    /// twelve. NSFW tags follow the setting.
    pub fn suggest_tags(&self, text: String, caret: u32) -> Vec<TagSuggestion> {
        let nsfw = self.settings.lock().nsfw;
        let byte = utf16_to_byte(&text, caret as usize);
        tags::suggest(&text, byte, nsfw, MOST_SUGGESTIONS)
            .items
            .into_iter()
            .map(|suggestion| TagSuggestion {
                tag: suggestion.tag,
                kind: suggestion.kind.into(),
                count: suggestion.count,
                from: byte_to_utf16(&text, suggestion.from),
                to: byte_to_utf16(&text, suggestion.to),
            })
            .collect()
    }

    /// Inserts suggestion `tag` (an index into [`suggest_tags`](Self::suggest_tags) for the same
    /// text and caret) over the word being typed; `caret` and the returned one are UTF-16 offsets.
    pub fn complete_tag(&self, text: String, caret: u32, tag: u32) -> TagCompletion {
        let byte = utf16_to_byte(&text, caret as usize);
        let nsfw = self.settings.lock().nsfw;
        let found = tags::suggest(&text, byte, nsfw, MOST_SUGGESTIONS);
        let Some(suggestion) = found.items.get(tag as usize) else {
            return TagCompletion { text, caret };
        };
        let (text, caret) = tags::complete(&text, byte, suggestion);
        TagCompletion { caret: byte_to_utf16(&text, caret), text }
    }

    /// Replaces the recent prompts with `prompts`, taken as newest first; blanks and duplicates
    /// are dropped and at most twenty are kept.
    pub fn set_recent_prompts(&self, prompts: Vec<String>) {
        let kept = kept_recent(prompts.iter().map(String::as_str));
        if let Ok(mut recent) = RECENT.lock() {
            *recent = kept;
        }
    }

    /// The recent prompts, newest first.
    pub fn recent_prompts(&self) -> Vec<String> {
        RECENT.lock().map(|recent| recent.clone()).unwrap_or_default()
    }

    /// Adds `prompt` to the recent prompts, newest first.
    pub fn remember_prompt(&self, prompt: String) {
        remember(&prompt);
    }
}

/// Adds `prompt` to the recent prompts, newest first; blanks are ignored and duplicates move to
/// the front.
pub(crate) fn remember(prompt: &str) {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return;
    }
    if let Ok(mut recent) = RECENT.lock() {
        recent.retain(|kept| kept != prompt);
        recent.insert(0, prompt.to_owned());
        recent.truncate(MOST_RECENT);
    }
}

/// Keeps the first of each prompt, without blanks, up to the cap.
fn kept_recent<'a>(prompts: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for prompt in prompts {
        let prompt = prompt.trim();
        if prompt.is_empty() || kept.iter().any(|old| old == prompt) {
            continue;
        }
        kept.push(prompt.to_owned());
        if kept.len() == MOST_RECENT {
            break;
        }
    }
    kept
}

/// The byte offset of UTF-16 offset `units` in `text` (the end if it is past the end).
fn utf16_to_byte(text: &str, units: usize) -> usize {
    let mut seen = 0;
    for (byte, c) in text.char_indices() {
        if seen >= units {
            return byte;
        }
        seen = seen.saturating_add(c.len_utf16());
    }
    text.len()
}

/// The UTF-16 offset of byte offset `byte` in `text`.
fn byte_to_utf16(text: &str, byte: usize) -> u32 {
    let byte = boundary_at_or_before(text, byte.min(text.len()));
    let units = text.get(..byte).map_or(0, |head| head.encode_utf16().count());
    u32::try_from(units).unwrap_or(u32::MAX)
}

/// `at` moved back to a UTF-8 boundary.
fn boundary_at_or_before(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::GenerationSettings;

    #[test]
    fn recent_prompts_are_newest_first_without_blanks_or_duplicates() {
        let generation =
            Generation::new(GenerationSettings { address: String::new(), nsfw: false, largest_megapixels: None });
        generation.set_recent_prompts(vec![
            "a landscape".to_owned(),
            "  ".to_owned(),
            "a still life".to_owned(),
            "a landscape".to_owned(),
        ]);
        assert_eq!(generation.recent_prompts(), ["a landscape", "a still life"]);
        generation.remember_prompt("  a portrait  ".to_owned());
        generation.remember_prompt("a still life".to_owned());
        assert_eq!(generation.recent_prompts(), ["a still life", "a portrait", "a landscape"]);
    }

    #[test]
    fn suggestions_use_utf16_offsets() {
        let generation =
            Generation::new(GenerationSettings { address: String::new(), nsfw: false, largest_megapixels: None });
        let text = "🌙, long_h".to_owned();
        let caret = text.encode_utf16().count() as u32;
        let found = generation.suggest_tags(text.clone(), caret);
        let long_hair = found.iter().find(|s| s.tag == "long hair").expect("long hair is in the lists");
        assert_eq!(long_hair.from, 4);
        assert_eq!(long_hair.to, 10);
        let done = generation.complete_tag(text, caret, 0);
        assert_eq!(done.text, "🌙, long hair, ");
        assert_eq!(done.caret, done.text.encode_utf16().count() as u32);
    }
}
