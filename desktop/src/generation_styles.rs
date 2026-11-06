//! The person's styles on the desktop: their folder (beside `brushes/` in the app's data folder),
//! read again every little while the Generate popover is open, so a style written or edited in a
//! text editor shows without a restart; and the popover's strip of style cards, each with its
//! family's badge and, when it can't run, why in short.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Align2, Rect, Sense, Ui, UiBuilder, pos2, vec2};
use procraft_core::{StyleFileText, StyleInfo};

use crate::look::{Themed, Weight, argb, pc, text, text_width};

/// A file bigger than this isn't a style.
const MOST_BYTES: u64 = 256 * 1024;
/// How often the folder is read again while the popover is open.
const READ_EVERY: Duration = Duration::from_secs(2);
pub const CARD_WIDTH: f32 = 116.0;
pub const CARD_HEIGHT: f32 = 76.0;

const CARD: Themed = Themed(0x2E5B5B5B, 0xFFD7D7D7);
const INK: Themed = Themed(0xFFFFFFFF, 0xFF222222);
const DIM: Themed = Themed(0x99FFFFFF, 0x99000000);
const WARN: u32 = 0xFFF2B440;

/// The styles folder: `styles/` in the app's data folder.
pub fn styles_dir() -> PathBuf {
    crate::files::app_dir().join("styles")
}

/// The folder's style files, by name; the folder is made if it isn't there, so there's a place to
/// put them.
pub fn read_style_files() -> Vec<StyleFileText> {
    let dir = styles_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("the styles folder {} couldn't be made: {e}", dir.display());
    }
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut files: Vec<StyleFileText> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("toml")))
        .filter(|e| e.metadata().is_ok_and(|m| m.is_file() && m.len() <= MOST_BYTES))
        .filter_map(|e| {
            let text = std::fs::read_to_string(e.path()).ok()?;
            Some(StyleFileText { name: e.file_name().to_string_lossy().into_owned(), text })
        })
        .collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
}

/// When the folder was last read, so the popover reads it again only every little while.
#[derive(Default)]
pub struct StylesFolder {
    read_at: Option<Instant>,
}

impl StylesFolder {
    /// Reads the folder into `generation` when it hasn't been read for a while.
    pub fn refresh(&mut self, generation: &procraft_core::Generation) {
        if self.read_at.is_some_and(|at| at.elapsed() < READ_EVERY) {
            return;
        }
        self.read_at = Some(Instant::now());
        generation.set_style_files(read_style_files());
    }
}

/// A family's badge colours: its ink and its pill's fill.
fn badge_colours(family: &str) -> (Themed, Themed) {
    if family == "Anima" {
        // a soft violet
        (Themed(0xFFC3B2FF, 0xFF5B3FC4), Themed(0x338E6CEF, 0x268E6CEF))
    } else {
        (Themed(0xFFCFCFCF, 0xFF4A4A4A), Themed(0x33FFFFFF, 0x1F000000))
    }
}

/// The strip of style cards at `at`, `width` across, scrolling sideways when they don't fit; the
/// chosen one ringed. Returns the style clicked, if one was.
pub fn strip(ui: &mut Ui, at: egui::Pos2, width: f32, styles: &[StyleInfo], chosen: &str) -> Option<String> {
    let row = Rect::from_min_size(at, vec2(width, CARD_HEIGHT));
    let mut row_ui =
        ui.new_child(UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)));
    row_ui.style_mut().always_scroll_the_only_direction = true;
    let mut clicked = None;
    egui::ScrollArea::horizontal()
        .id_salt("style cards")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(&mut row_ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
            for s in styles {
                let (card, response) = ui.allocate_exact_size(vec2(CARD_WIDTH, CARD_HEIGHT), Sense::click());
                card_at(ui, card, s, s.id == chosen);
                if response.clicked() {
                    clicked = Some(s.id.clone());
                }
            }
        });
    clicked
}

/// One card: the name (two lines at most) beside the family's badge, and the settings line, or
/// in amber why it can't run.
fn card_at(ui: &Ui, card: Rect, s: &StyleInfo, on: bool) {
    let painter = ui.painter();
    painter.rect_filled(card, 12.0, argb(CARD));
    if on {
        painter.rect_stroke(card, 12.0, egui::Stroke::new(2.0, argb(pc::ACCENT)), egui::StrokeKind::Inside);
    }
    let (ink, fill) = badge_colours(&s.family);
    let badge_width = text_width(painter, &s.family, 10.0, Weight::SemiBold) + 12.0;
    let badge = Rect::from_min_size(pos2(card.right() - 8.0 - badge_width, card.top() + 8.0), vec2(badge_width, 16.0));
    painter.rect_filled(badge, 8.0, argb(fill));
    text(painter, badge.center(), Align2::CENTER_CENTER, &s.family, 10.0, Weight::SemiBold, argb(ink));
    // the name, two lines at most, drawn rather than added as a widget: a widget would move the
    // row's cursor, and the next card would start over this one
    let mut job = egui::text::LayoutJob::default();
    let format = egui::TextFormat { font_id: Weight::SemiBold.font(14.0), color: argb(INK), ..Default::default() };
    job.append(&s.name, 0.0, format);
    job.wrap.max_width = badge.left() - 4.0 - (card.left() + 10.0);
    job.wrap.max_rows = 2;
    job.wrap.overflow_character = Some('…');
    let name = painter.layout_job(job);
    painter.galley(card.min + vec2(10.0, 9.0), name, argb(INK));
    let (line, colour) = match &s.note {
        Some(note) => (note.clone(), argb(WARN)),
        None => (format!("{} steps · CFG {}", s.steps, s.cfg), argb(DIM)),
    };
    text(
        painter,
        pos2(card.left() + 10.0, card.bottom() - 10.0),
        Align2::LEFT_BOTTOM,
        &line,
        11.0,
        Weight::Regular,
        colour,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use procraft_core::StyleKind;

    fn style(id: &str, name: &str) -> StyleInfo {
        StyleInfo {
            id: id.into(),
            name: name.into(),
            steps: 30,
            cfg: 4.0,
            family: "Anima".into(),
            kind: StyleKind::Yours,
            file: None,
            note: None,
            reason: None,
        }
    }

    /// The strip drawn once at the window's corner, with `events`; the style clicked, if any.
    fn frame(ctx: &egui::Context, styles: &[StyleInfo], events: Vec<egui::Event>) -> Option<String> {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
            events,
            ..Default::default()
        };
        let mut clicked = None;
        let mut out = ctx.run_ui(input, |root| {
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(root, |ui| {
                clicked = strip(ui, pos2(0.0, 0.0), 400.0, styles, "a");
            });
        });
        out.textures_delta.clear();
        clicked
    }

    #[test]
    fn each_card_takes_its_own_place_in_the_strip() {
        let ctx = egui::Context::default();
        let styles = [style("a", "Gone LoRA"), style("b", "Moonlit with a long name"), style("c", "Typo")];
        frame(&ctx, &styles, vec![]);
        // the middle of the second card: one card and one gap along
        let p = pos2(CARD_WIDTH + 6.0 + CARD_WIDTH / 2.0, CARD_HEIGHT / 2.0);
        let button = |pressed| egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(&ctx, &styles, vec![egui::Event::PointerMoved(p)]);
        frame(&ctx, &styles, vec![button(true)]);
        assert_eq!(frame(&ctx, &styles, vec![button(false)]).as_deref(), Some("b"));
    }
}
