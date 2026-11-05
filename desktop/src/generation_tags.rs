//! The prompt's suggestion strip: tag chips while a tag is being typed, and recent prompts when
//! the field is empty and focused. The tags come from the core (which keeps their offsets in
//! UTF-16); this module converts them to the prompt's byte offsets and inserts what was picked.

use egui::{Align2, Color32, Id, Rect, Sense, Ui, pos2, vec2};
use procraft_core::{Generation, TagKind, TagSuggestion};

use crate::look::{Themed, Weight, argb, text, text_width};

/// The room the strip takes under the prompt field, and the gap above it.
pub const STRIP_HEIGHT: f32 = 34.0;
pub const STRIP_GAP: f32 = 8.0;

const CHIP: Themed = Themed(0x14FFFFFF, 0x14000000);
const INK: Themed = Themed(0xFFFFFFFF, 0xFF222222);
const DIM: Themed = Themed(0x99FFFFFF, 0x99000000);
/// Danbooru's category colours: general, artist, copyright, character, meta.
const KIND_COLOURS: [u32; 5] = [0xFF8E8E93, 0xFFE85D75, 0xFFF2B440, 0xFF55C98A, 0xFFA57BFF];
const MAX_TAG_CHARS: usize = 28;
const MAX_PROMPT_CHARS: usize = 34;

/// The keys the suggestion strip handles before the prompt field sees them: Tab or Enter inserts
/// the best suggestion. Returns whether Enter was taken (so Generate must not run).
pub fn keys(ui: &Ui, generation: &Generation, text: &mut String, caret: &mut usize, field: Id) -> bool {
    if !ui.memory(|memory| memory.has_focus(field)) {
        return false;
    }
    let suggestions = suggestions(generation, text, *caret);
    if suggestions.is_empty() {
        return false;
    }
    let tab = ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
    let enter = ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    if tab || enter {
        insert(ui, generation, text, caret, field, 0);
    }
    enter
}

/// Draws the strip at `at`, `width` points wide: suggestions, or recent prompts when the field is
/// empty. A click completes or fills the prompt.
pub fn strip(
    ui: &Ui,
    generation: &Generation,
    prompt: &mut String,
    caret: &mut usize,
    field: Id,
    focused: bool,
    at: egui::Pos2,
    width: f32,
) {
    if !focused {
        return;
    }
    if prompt.trim().is_empty() {
        let recent = generation.recent_prompts();
        let mut x = at.x;
        for (i, recent_prompt) in recent.iter().take(4).enumerate() {
            let label = shortened(&recent_prompt.replace(['\n', '\r'], " "), MAX_PROMPT_CHARS);
            let label_width = text_width(ui.painter(), &label, 14.0, Weight::Medium);
            let chip_width = (label_width + 34.0).min(width.max(80.0));
            if x + chip_width > at.x + width {
                break;
            }
            let chip = Rect::from_min_size(pos2(x, at.y + 4.0), vec2(chip_width, 26.0));
            ui.painter().rect_filled(chip, 13.0, argb(CHIP));
            ui.painter().circle_filled(pos2(chip.left() + 12.0, chip.center().y), 3.5, argb(crate::look::pc::ACCENT));
            text(
                ui.painter(),
                pos2(chip.left() + 21.0, chip.center().y),
                Align2::LEFT_CENTER,
                &label,
                14.0,
                Weight::Medium,
                argb(INK),
            );
            if ui.interact(chip, ui.scope_id().with(("recent prompt", i)), Sense::click()).clicked() {
                *prompt = recent_prompt.clone();
                *caret = prompt.len();
                store_caret(ui.ctx(), field, prompt, *caret);
                ui.ctx().memory_mut(|memory| memory.request_focus(field));
            }
            x = chip.right() + 6.0;
        }
        return;
    }
    let found = suggestions(generation, prompt, *caret);
    let mut x = at.x;
    for (i, suggestion) in found.iter().enumerate() {
        // through an alias, it says which: "housecat → domestic cat"
        let label = match &suggestion.alias {
            Some(alias) => shortened(&format!("{alias} → {}", suggestion.tag), MAX_TAG_CHARS + 12),
            None => shortened(&suggestion.tag, MAX_TAG_CHARS),
        };
        let label_width = text_width(ui.painter(), &label, 14.0, Weight::Medium);
        let count_width = text_width(ui.painter(), &suggestion.count, 11.0, Weight::Regular);
        let chip_width = 21.0 + label_width + 8.0 + count_width + 14.0;
        if x + chip_width > at.x + width {
            break;
        }
        let chip = Rect::from_min_size(pos2(x, at.y + 4.0), vec2(chip_width, 26.0));
        ui.painter().rect_filled(chip, 13.0, argb(CHIP));
        ui.painter().circle_filled(pos2(chip.left() + 12.0, chip.center().y), 4.0, kind_colour(suggestion.kind));
        text(
            ui.painter(),
            pos2(chip.left() + 21.0, chip.center().y),
            Align2::LEFT_CENTER,
            &label,
            14.0,
            Weight::Medium,
            argb(INK),
        );
        text(
            ui.painter(),
            pos2(chip.right() - 10.0, chip.center().y),
            Align2::RIGHT_CENTER,
            &suggestion.count,
            11.0,
            Weight::Regular,
            argb(DIM),
        );
        if ui.interact(chip, ui.scope_id().with(("tag suggestion", i)), Sense::click()).clicked() {
            insert(ui, generation, prompt, caret, field, i);
        }
        x = chip.right() + 6.0;
    }
}

/// Completes suggestion `index` and puts the caret after its separator.
fn insert(ui: &Ui, generation: &Generation, text: &mut String, caret: &mut usize, field: Id, index: usize) {
    let at = utf16_of_bytes(text, *caret);
    let done = generation.complete_tag(text.clone(), at, index as u32);
    *text = done.text;
    *caret = byte_of_utf16(text, done.caret);
    store_caret(ui.ctx(), field, text, *caret);
    ui.ctx().memory_mut(|memory| memory.request_focus(field));
}

/// The suggestions for the prompt at `caret` (a byte offset).
fn suggestions(generation: &Generation, text: &str, caret: usize) -> Vec<TagSuggestion> {
    generation.suggest_tags(text.to_owned(), utf16_of_bytes(text, caret))
}

/// A prompt's byte offset as the UTF-16 offset the core uses.
fn utf16_of_bytes(text: &str, byte: usize) -> u32 {
    let byte = byte.min(text.len());
    let units = text.get(..byte).map_or(0, |head| head.encode_utf16().count());
    u32::try_from(units).unwrap_or(u32::MAX)
}

/// A UTF-16 offset in `text` as a byte offset.
fn byte_of_utf16(text: &str, units: u32) -> usize {
    let mut seen = 0;
    for (byte, c) in text.char_indices() {
        if seen >= units as usize {
            return byte;
        }
        seen += c.len_utf16();
    }
    text.len()
}

/// A text cursor's character index as a byte offset.
pub fn char_to_byte(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(byte, _)| byte)
}

/// Puts the text cursor at the byte offset `byte`.
fn store_caret(ctx: &egui::Context, field: Id, text: &str, byte: usize) {
    let chars = text.get(..byte).map_or(0, |head| head.chars().count());
    if let Some(mut state) = egui::text_edit::TextEditState::load(ctx, field) {
        state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(chars))));
        state.store(ctx, field);
    }
}

/// A chip's dot colour.
fn kind_colour(kind: TagKind) -> Color32 {
    let at = match kind {
        TagKind::General => 0,
        TagKind::Artist => 1,
        TagKind::Copyright => 2,
        TagKind::Character => 3,
        TagKind::Meta => 4,
    };
    KIND_COLOURS.get(at).map_or(Color32::GRAY, |colour| {
        Color32::from_rgba_unmultiplied(
            (*colour >> 16) as u8,
            (*colour >> 8) as u8,
            *colour as u8,
            (*colour >> 24) as u8,
        )
    })
}

/// `text` cut to `most` characters, with an ellipsis when it was longer.
fn shortened(text: &str, most: usize) -> String {
    let mut chars = text.chars();
    let mut short: String = chars.by_ref().take(most).collect();
    if chars.next().is_some() {
        short.push('…');
    }
    short
}
