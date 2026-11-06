//! The prompt's suggestion strip: tag chips while a tag is being typed, and recent prompts when
//! the field is empty and focused. The tags come from the core (which keeps their offsets in
//! UTF-16); this module converts them to the prompt's byte offsets and inserts what was picked.

use egui::{Align2, Color32, Id, Rect, Sense, Ui, UiBuilder, pos2, vec2};
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
    let suggestions = suggestions(ui, generation, text, *caret);
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

/// The strip's place under the prompt field at `at`, `width` points wide.
pub fn strip_rect(at: egui::Pos2, width: f32) -> Rect {
    Rect::from_min_size(at, vec2(width, STRIP_HEIGHT))
}

/// One chip of the strip: its dot, its label and, for a tag, its count.
struct Chip {
    dot: Color32,
    label: String,
    count: String,
}

/// Draws the strip at `at`, `width` points wide: suggestions, or recent prompts when the field is
/// empty, in a row that scrolls sideways (the mouse wheel too) when they don't all fit. A click
/// completes or fills the prompt.
pub fn strip(
    ui: &mut Ui,
    generation: &Generation,
    prompt: &mut String,
    caret: &mut usize,
    field: Id,
    focused: bool,
    at: egui::Pos2,
    width: f32,
) {
    if !focused {
        // searched afresh next time (the NSFW switch may have changed meanwhile)
        ui.ctx().data_mut(|d| d.remove_temp::<(String, usize, Vec<TagSuggestion>)>(kept_id()));
        return;
    }
    if prompt.trim().is_empty() {
        let recent = generation.recent_prompts();
        let chips: Vec<Chip> = recent
            .iter()
            .map(|p| Chip {
                dot: argb(crate::look::pc::ACCENT),
                label: shortened(&p.replace(['\n', '\r'], " "), MAX_PROMPT_CHARS),
                count: String::new(),
            })
            .collect();
        if let Some(i) = chips_row(ui, at, width, "recent prompts", &chips) {
            if let Some(recent_prompt) = recent.get(i) {
                *prompt = recent_prompt.clone();
                *caret = prompt.len();
                store_caret(ui.ctx(), field, prompt, *caret);
            }
            ui.ctx().memory_mut(|memory| memory.request_focus(field));
        }
        return;
    }
    let found = suggestions(ui, generation, prompt, *caret);
    let chips: Vec<Chip> = found
        .iter()
        .map(|suggestion| Chip {
            dot: kind_colour(suggestion.kind),
            // through an alias, it says which: "housecat → domestic cat"
            label: match &suggestion.alias {
                Some(alias) => shortened(&format!("{alias} → {}", suggestion.tag), MAX_TAG_CHARS + 12),
                None => shortened(&suggestion.tag, MAX_TAG_CHARS),
            },
            count: suggestion.count.clone(),
        })
        .collect();
    // a new word starts the row at its beginning
    let word = found.first().map_or(0, |s| s.from);
    if let Some(i) = chips_row(ui, at, width, ("tag suggestions", word, chips.first().map(|c| c.label.clone())), &chips)
    {
        insert(ui, generation, prompt, caret, field, i);
    }
}

/// The chips in a row at `at` that scrolls sideways; the one clicked, if any.
fn chips_row(ui: &mut Ui, at: egui::Pos2, width: f32, salt: impl egui::AsIdSalt, chips: &[Chip]) -> Option<usize> {
    let row = Rect::from_min_size(pos2(at.x, at.y + 4.0), vec2(width, 26.0));
    let mut row_ui =
        ui.new_child(UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)));
    row_ui.style_mut().always_scroll_the_only_direction = true;
    let mut clicked = None;
    egui::ScrollArea::horizontal()
        .id_salt(salt)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(&mut row_ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
            for (i, c) in chips.iter().enumerate() {
                let label_width = text_width(ui.painter(), &c.label, 14.0, Weight::Medium);
                let chip_width = if c.count.is_empty() {
                    label_width + 34.0
                } else {
                    21.0 + label_width + 8.0 + text_width(ui.painter(), &c.count, 11.0, Weight::Regular) + 14.0
                };
                let (chip, response) = ui.allocate_exact_size(vec2(chip_width, 26.0), Sense::click());
                let painter = ui.painter();
                painter.rect_filled(chip, 13.0, argb(CHIP));
                painter.circle_filled(pos2(chip.left() + 12.0, chip.center().y), 4.0, c.dot);
                let ink = argb(INK);
                text(
                    painter,
                    pos2(chip.left() + 21.0, chip.center().y),
                    Align2::LEFT_CENTER,
                    &c.label,
                    14.0,
                    Weight::Medium,
                    ink,
                );
                if !c.count.is_empty() {
                    let at = pos2(chip.right() - 10.0, chip.center().y);
                    text(painter, at, Align2::RIGHT_CENTER, &c.count, 11.0, Weight::Regular, argb(DIM));
                }
                if response.clicked() {
                    clicked = Some(i);
                }
            }
        });
    clicked
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

/// Where the last suggestions are kept, with the text and caret they were for.
fn kept_id() -> Id {
    Id::unique("tag suggestions kept")
}

/// The suggestions for the prompt at `caret` (a byte offset), kept for the next frame: a search
/// takes a few milliseconds, and the strip and the keys both ask every frame.
fn suggestions(ui: &Ui, generation: &Generation, text: &str, caret: usize) -> Vec<TagSuggestion> {
    type Kept = (String, usize, Vec<TagSuggestion>);
    let id = kept_id();
    let kept: Option<Kept> = ui.ctx().data(|d| d.get_temp(id));
    if let Some((t, c, found)) = kept
        && t == text
        && c == caret
    {
        return found;
    }
    let found = generation.suggest_tags(text.to_owned(), utf16_of_bytes(text, caret));
    ui.ctx().data_mut(|d| d.insert_temp::<Kept>(id, (text.to_owned(), caret, found.clone())));
    found
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
