//! The Style Studio on the desktop, as the Android app's (generation/StyleStudio.kt): a style's
//! settings in the full window, built like Brush Studio. The pages by name on a card at the left
//! (Model, LoRAs, Prompts, Sampling, Advanced), the open page's settings beside it, and the preview
//! on the right: Cancel and Done, the last try and the ones before it, the preview prompt and Try
//! it. Done writes the style's file in the styles folder (what was written in it by hand is kept);
//! Cancel leaves the folder as it was, without asking, as Brush Studio does.

use std::sync::Arc;
use std::time::Duration;

use egui::{Align2, Color32, Rect, Sense, TextureHandle, Ui, UiBuilder, pos2, vec2};
use procraft_core::{Generation, LoraSetting, ServerLists, StyleEdit};

use crate::look::{Look, Themed, Weight, argb, pc, text, text_width};

const TABS_FILL: Themed = Themed(0xFF272727, 0xFFEBEBEB);
const BACK: Themed = Themed(0xFF1F1F1F, 0xFFF3F3F3);
const LINE: Themed = Themed(0xFF303030, 0xFFCFCFCF);
const HEADER: Themed = Themed(0xFF9C9C9C, 0xFF818181);
const LABEL: Themed = Themed(0xFFEDEDED, 0xFF343434);
const DIM: Themed = Themed(0xFF8E8E8E, 0xFF9A9A9A);
const CARD_EDGE: Themed = Themed(0xFF373737, 0xFFDADADA);
const CARD_TITLE: Themed = Themed(0xFFC1C1C1, 0xFF4A4A4A);
const PAGE_INK: Themed = Themed(0xFF797979, 0xFF3E3E3E);
const LIST: Themed = Themed(0xFF242424, 0xFFFFFFFF);
const LIST_LINE: Themed = Themed(0xFF202020, 0xFFE5E5E5);
const FIELD: Themed = Themed(0xFF2A2A2A, 0xFFFFFFFF);
const BUTTON: Themed = Themed(0xFF1C1C1C, 0xFFFBFBFB);
const BUTTON_EDGE: Themed = Themed(0xFF383838, 0xFFE0E0E0);
const BUTTON_INK: Themed = Themed(0xFFAAAAAA, 0xFF585858);
const PREVIEW_FILL: Themed = Themed(0xFF2A2A2A, 0xFFE8E8E8);
const DONE: u32 = 0xFF0190FF;
const WARN: u32 = 0xFFF2B440;
const PAGES_WIDTH: f32 = 320.0;
const SETTINGS_WIDTH: f32 = 323.0;
/// How many tries are kept to compare.
const TRIES: usize = 4;
/// Where the preview prompt starts: a safe scene.
pub const PREVIEW_PROMPT: &str = "a lighthouse on a cliff, scenery, no humans";

const PAGES: [&str; 5] = ["Model", "LoRAs", "Prompts", "Sampling", "Advanced"];

/// A list the settings column shows in place of the page: what it chooses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Picker {
    Sampler,
    Scheduler,
    Vae,
    TextEncoder,
    Lora,
}

/// How the Studio ended.
pub enum Closed {
    Cancelled,
    /// Saved in this file of the styles folder.
    Saved(String),
}

pub struct StyleStudio {
    generation: Arc<Generation>,
    edit: StyleEdit,
    /// The file it's kept in, when editing one of the person's styles, and its text as it was.
    file: Option<String>,
    old_text: Option<String>,
    page: usize,
    picker: Option<Picker>,
    lists: ServerLists,
    seed: u64,
    /// The tries, newest first: their number, picture and seed; and the one shown.
    tries: Vec<(u64, TextureHandle, u64)>,
    shown: Option<u64>,
    error: Option<String>,
    /// The name is being typed in its heading, and its field has had the focus.
    renaming: bool,
    focused: bool,
}

impl StyleStudio {
    /// The Studio on `edit`, kept in `file` (with its text `old_text`) when it's one of theirs.
    pub fn new(
        generation: Arc<Generation>,
        edit: StyleEdit,
        file: Option<String>,
        old_text: Option<String>,
    ) -> StyleStudio {
        let lists = generation.server_lists();
        generation.close_studio();
        StyleStudio {
            generation,
            edit,
            file,
            old_text,
            page: 0,
            picker: None,
            lists,
            seed: random_seed(),
            tries: Vec::new(),
            shown: None,
            error: None,
            renaming: false,
            focused: false,
        }
    }

    /// Draws the Studio over the whole window; how it ended, when it did. `prompt` is the preview
    /// prompt (kept app-wide), `canvas` the open canvas's size (the tries take its shape).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        look: &mut Look,
        prompt: &mut String,
        canvas: (u32, u32),
    ) -> Option<Closed> {
        self.poll(ctx);
        let mut closed = None;
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::unique("style studio")).fixed_pos(screen.min).order(egui::Order::Foreground).show(
            ctx,
            |ui| {
                ui.allocate_rect(screen, Sense::click_and_drag());
                ui.painter().rect_filled(screen, 0, argb(BACK));
                let pages =
                    Rect::from_min_size(screen.min + vec2(10.0, 10.0), vec2(PAGES_WIDTH, screen.height() - 20.0));
                let settings = Rect::from_min_max(
                    pos2(pages.right(), screen.top()),
                    pos2(pages.right() + SETTINGS_WIDTH, screen.bottom()),
                );
                let preview = Rect::from_min_max(pos2(settings.right() + 1.0, screen.top()), screen.max);
                ui.painter().line_segment(
                    [pos2(settings.right(), screen.top()), pos2(settings.right(), screen.bottom())],
                    egui::Stroke::new(1.0, argb(LINE)),
                );
                self.pages_card(ui, pages);
                self.settings(ui, look, settings);
                closed = self.preview(ui, look, preview, prompt, canvas);
            },
        );
        if closed.is_none() && crate::ui::escape(ctx) {
            closed = Some(Closed::Cancelled);
        }
        if closed.is_some() {
            self.generation.close_studio();
        }
        closed
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let state = self.generation.studio_state();
        if state.running {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if let Some(p) = state.picture
            && !self.tries.iter().any(|(id, _, _)| *id == p.id)
        {
            let texture = crate::generation::texture(ctx, &format!("try {}", p.id), &p.picture);
            self.tries.insert(0, (p.id, texture, p.seed));
            self.tries.truncate(TRIES);
            self.shown = Some(p.id);
        }
        if state.error.is_some() {
            self.error = state.error;
        }
    }

    /// The card at the left: "Style Studio", its pages, and Open styles folder.
    fn pages_card(&mut self, ui: &mut Ui, card: Rect) {
        let painter = ui.painter();
        painter.rect_filled(card, 20.0, argb(TABS_FILL));
        painter.rect_stroke(card, 20.0, egui::Stroke::new(1.0, argb(CARD_EDGE)), egui::StrokeKind::Inside);
        text(
            painter,
            card.min + vec2(24.0, 19.0),
            Align2::LEFT_TOP,
            "Style Studio",
            23.0,
            Weight::Bold,
            argb(CARD_TITLE),
        );
        let mut y = card.top() + 70.0;
        for (i, name) in PAGES.iter().enumerate() {
            let row = Rect::from_min_size(pos2(card.left() + 8.0, y), vec2(card.width() - 16.0, 44.0));
            let on = self.page == i;
            if on {
                ui.painter().rect_filled(row, 22.0, argb(pc::ACCENT));
            }
            let ink = if on { Color32::WHITE } else { argb(PAGE_INK) };
            text(
                ui.painter(),
                pos2(row.left() + 20.0, row.center().y),
                Align2::LEFT_CENTER,
                name,
                15.5,
                Weight::Regular,
                ink,
            );
            if ui.interact(row, ui.scope_id().with(("studio page", i)), Sense::click()).clicked() {
                self.page = i;
                self.picker = None;
            }
            y += 48.0;
        }
        let open = Rect::from_min_size(pos2(card.left() + 8.0, card.bottom() - 52.0), vec2(card.width() - 16.0, 44.0));
        text(
            ui.painter(),
            pos2(open.left() + 20.0, open.center().y),
            Align2::LEFT_CENTER,
            "Open styles folder",
            15.0,
            Weight::Regular,
            argb(pc::ACCENT),
        );
        if ui.interact(open, ui.scope_id().with("open styles folder"), Sense::click()).clicked() {
            crate::generation_styles::open_styles_folder();
        }
    }

    /// The open page's settings, or a list being chosen from.
    fn settings(&mut self, ui: &mut Ui, look: &mut Look, column: Rect) {
        let inner = column.shrink2(vec2(20.0, 0.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
        egui::ScrollArea::vertical().id_salt(("studio settings", self.page, self.picker.is_some())).show(
            &mut child,
            |ui| {
                ui.set_width(inner.width());
                ui.add_space(25.0);
                if let Some(picker) = self.picker {
                    self.picker_list(ui, picker);
                    return;
                }
                match self.page {
                    0 => self.model_page(ui),
                    1 => self.loras_page(ui, look),
                    2 => self.prompts_page(ui),
                    3 => self.sampling_page(ui, look),
                    _ => self.advanced_page(ui, look),
                }
                ui.add_space(60.0);
            },
        );
    }

    fn model_page(&mut self, ui: &mut Ui) {
        // the name heads the page, clicked to change it there, as Brush Studio's About page has it
        if self.renaming {
            let response = field(ui, &mut self.edit.name, false, "studio name");
            if !self.focused {
                response.request_focus();
                self.focused = true;
            } else if response.lost_focus() {
                self.renaming = false;
            }
        } else {
            let name =
                if self.edit.name.trim().is_empty() { "Untitled style".to_owned() } else { self.edit.name.clone() };
            let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
            text(
                ui.painter(),
                pos2(row.left() + 8.0, row.center().y),
                Align2::LEFT_CENTER,
                &name,
                21.0,
                Weight::Bold,
                argb(HEADER),
            );
            if response.on_hover_text("Click to rename").clicked() {
                self.renaming = true;
                self.focused = false;
            }
        }
        ui.add_space(18.0);
        heading(ui, "Model");
        let chosen = self.generation.checkpoint_for(self.edit.models.clone());
        let mut rows: Vec<(String, bool)> =
            self.lists.checkpoints.iter().map(|c| (c.clone(), Some(c) == chosen.as_ref())).collect();
        if chosen.is_none() {
            let written = self.edit.models.first().cloned().unwrap_or_default();
            rows.insert(0, (format!("{written} — not on this server"), true));
        }
        if self.lists.checkpoints.is_empty() {
            note(ui, "Not connected to a server");
        }
        let picked = list(
            ui,
            &rows.iter().map(|(name, on)| (name.as_str(), *on, &self.edit.family)).collect::<Vec<_>>(),
            "model",
        );
        if let Some(i) = picked
            && let Some(file) = self.lists.checkpoints.get(if chosen.is_none() { i.wrapping_sub(1) } else { i })
        {
            self.edit.models = vec![file.clone()];
        }
    }

    fn loras_page(&mut self, ui: &mut Ui, look: &mut Look) {
        heading(ui, "LoRAs");
        if self.edit.loras.is_empty() {
            note(ui, "None yet. A LoRA pushes a look or a character; add one below.");
        }
        let mut remove = None;
        for (i, lora) in self.edit.loras.iter_mut().enumerate() {
            let missing = !self.lists.loras.is_empty() && !self.lists.loras.contains(&lora.file);
            let (block, _) = ui.allocate_exact_size(vec2(ui.available_width(), 92.0), Sense::hover());
            ui.painter().rect_filled(block, 14.0, argb(LIST));
            let name = procraft_name(&lora.file);
            let ink = if missing { argb(WARN) } else { argb(LABEL) };
            text(ui.painter(), block.min + vec2(14.0, 12.0), Align2::LEFT_TOP, &name, 15.0, Weight::Medium, ink);
            if missing {
                text(
                    ui.painter(),
                    block.min + vec2(14.0, 32.0),
                    Align2::LEFT_TOP,
                    "not on this server",
                    11.0,
                    Weight::Regular,
                    argb(WARN),
                );
            }
            let cross = Rect::from_center_size(pos2(block.right() - 22.0, block.top() + 22.0), vec2(28.0, 28.0));
            text(ui.painter(), cross.center(), Align2::CENTER_CENTER, "×", 20.0, Weight::Regular, argb(DIM));
            if ui.interact(cross, ui.scope_id().with(("lora remove", i)), Sense::click()).clicked() {
                remove = Some(i);
            }
            let value = format!("{:.0} %", lora.strength * 100.0);
            text(
                ui.painter(),
                pos2(block.right() - 14.0, block.bottom() - 34.0),
                Align2::RIGHT_CENTER,
                &value,
                13.0,
                Weight::Regular,
                argb(DIM),
            );
            text(
                ui.painter(),
                pos2(block.left() + 14.0, block.bottom() - 34.0),
                Align2::LEFT_CENTER,
                "Strength",
                13.0,
                Weight::Regular,
                argb(DIM),
            );
            let track = Rect::from_min_max(
                pos2(block.left() + 14.0, block.bottom() - 16.0),
                pos2(block.right() - 14.0, block.bottom() - 12.0),
            );
            if let Some(v) =
                crate::generation::slider(ui, look, track, lora.strength / 2.0, &format!("lora strength {i}"))
            {
                lora.strength = (v * 40.0).round() / 20.0;
            }
            // its trigger words, kept on the server for every style that uses it
            ui.add_space(6.0);
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
            text(
                ui.painter(),
                pos2(row.left() + 8.0, row.center().y),
                Align2::LEFT_CENTER,
                "Trigger words",
                13.0,
                Weight::Regular,
                argb(DIM),
            );
            field(ui, &mut lora.trigger, false, &format!("lora trigger {i}"));
            note(ui, "Used by every style with this LoRA, added after your prompt.");
            ui.add_space(12.0);
        }
        if let Some(i) = remove {
            self.edit.loras.remove(i);
        }
        if button_row(ui, "+ Add LoRA", "add lora") {
            self.picker = Some(Picker::Lora);
        }
    }

    fn prompts_page(&mut self, ui: &mut Ui) {
        heading(ui, "Prompt");
        field(ui, &mut self.edit.template, true, "studio template");
        let hint = if self.edit.template.contains("{prompt}") {
            "{prompt} is where your prompt goes"
        } else {
            "Your prompt goes at the end"
        };
        note(ui, hint);
        ui.add_space(14.0);
        heading(ui, "Negative prompt");
        field(ui, &mut self.edit.negative, true, "studio negative");
    }

    fn sampling_page(&mut self, ui: &mut Ui, look: &mut Look) {
        heading(ui, "Sampling");
        if value_row(ui, "Sampler", &self.edit.sampler, "sampler") {
            self.picker = Some(Picker::Sampler);
        }
        ui.add_space(10.0);
        let steps = self.edit.steps as f32;
        if let Some(v) =
            labelled_slider(ui, look, "Steps", &format!("{}", self.edit.steps), (steps - 1.0) / 99.0, "steps")
        {
            self.edit.steps = (1.0 + v * 99.0).round() as u32;
        }
        if let Some(v) =
            labelled_slider(ui, look, "CFG", &format!("{:.1}", self.edit.cfg), (self.edit.cfg - 1.0) / 14.0, "cfg")
        {
            self.edit.cfg = ((1.0 + v * 14.0) * 10.0).round() / 10.0;
        }
    }

    fn advanced_page(&mut self, ui: &mut Ui, look: &mut Look) {
        heading(ui, "Advanced");
        if value_row(ui, "Scheduler", &self.edit.scheduler, "scheduler") {
            self.picker = Some(Picker::Scheduler);
        }
        ui.add_space(10.0);
        // Rescale CFG: overridden or not
        let on = self.edit.rescale_cfg.is_some();
        if switch_row(ui, "Rescale CFG", on, "rescale") {
            self.edit.rescale_cfg = if on { None } else { Some(0.7) };
        }
        if let Some(r) = self.edit.rescale_cfg
            && let Some(v) = labelled_slider(ui, look, "Multiplier", &format!("{r:.2}"), r, "rescale value")
        {
            self.edit.rescale_cfg = Some((v * 100.0).round() / 100.0);
        }
        ui.add_space(10.0);
        let vae = self.edit.vae.clone().unwrap_or_else(|| "The model's own".to_owned());
        if value_row(ui, "VAE", &vae, "vae") {
            self.picker = Some(Picker::Vae);
        }
        ui.add_space(10.0);
        let encoder = self.edit.text_encoder.clone().unwrap_or_else(|| "The model's own".to_owned());
        if value_row(ui, "Text encoder", &encoder, "text encoder") {
            self.picker = Some(Picker::TextEncoder);
        }
        ui.add_space(10.0);
        // Best size: Krita's Preferred Resolution, off unless overridden
        let on = self.edit.best_size.is_some();
        if switch_row(ui, "Best size", on, "best size") {
            self.edit.best_size = if on { None } else { Some(1024) };
        }
        if let Some(size) = self.edit.best_size {
            let at = (size as f32 - 256.0) / (2048.0 - 256.0);
            if let Some(v) = labelled_slider(ui, look, "One side", &format!("{size} px"), at, "best size value") {
                self.edit.best_size = Some(((256.0 + v * (2048.0 - 256.0)) / 8.0).round() as u32 * 8);
            }
            note(
                ui,
                "Pictures are made at about this size squared, then scaled. For models trained at an unusual size.",
            );
        }
    }

    /// A list of what the server has, chosen from in place of the page.
    fn picker_list(&mut self, ui: &mut Ui, picker: Picker) {
        let (title, options, current): (&str, Vec<String>, Option<String>) = match picker {
            Picker::Sampler => ("Sampler", self.lists.samplers.clone(), Some(self.edit.sampler.clone())),
            Picker::Scheduler => ("Scheduler", self.lists.schedulers.clone(), Some(self.edit.scheduler.clone())),
            Picker::Vae => ("VAE", own_first(&self.lists.vaes), Some(self.edit.vae.clone().unwrap_or_default())),
            Picker::TextEncoder => (
                "Text encoder",
                own_first(&self.lists.text_encoders),
                Some(self.edit.text_encoder.clone().unwrap_or_default()),
            ),
            Picker::Lora => (
                "Add a LoRA",
                self.lists.loras.iter().filter(|l| !self.edit.loras.iter().any(|x| &x.file == *l)).cloned().collect(),
                None,
            ),
        };
        let back = format!("‹ {}", PAGES.get(self.page).unwrap_or(&"Back"));
        let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
        text(
            ui.painter(),
            pos2(row.left() + 8.0, row.center().y),
            Align2::LEFT_CENTER,
            &back,
            15.0,
            Weight::Regular,
            argb(pc::ACCENT),
        );
        if response.clicked() {
            self.picker = None;
            return;
        }
        ui.add_space(6.0);
        heading(ui, title);
        if options.is_empty() {
            note(
                ui,
                if self.lists.checkpoints.is_empty() { "Not connected to a server" } else { "The server has none" },
            );
            return;
        }
        let shown: Vec<String> =
            options.iter().map(|o| if o.is_empty() { "The model's own".to_owned() } else { o.clone() }).collect();
        let rows: Vec<(&str, bool, &String)> = shown
            .iter()
            .zip(&options)
            .map(|(s, o)| (s.as_str(), current.as_ref() == Some(o), &self.edit.family))
            .collect();
        let Some(i) = list(ui, &rows, "picker") else { return };
        let Some(choice) = options.get(i).cloned() else { return };
        match picker {
            Picker::Sampler => self.edit.sampler = choice,
            Picker::Scheduler => self.edit.scheduler = choice,
            Picker::Vae => self.edit.vae = (!choice.is_empty()).then_some(choice),
            Picker::TextEncoder => self.edit.text_encoder = (!choice.is_empty()).then_some(choice),
            Picker::Lora => {
                let trigger = self.generation.lora_trigger(choice.clone());
                self.edit.loras.push(LoraSetting { file: choice, strength: 1.0, trigger });
            }
        }
        self.picker = None;
    }

    /// The preview: Cancel and Done, the try shown and the ones before, the prompt and Try it.
    fn preview(
        &mut self,
        ui: &mut Ui,
        look: &mut Look,
        area: Rect,
        prompt: &mut String,
        canvas: (u32, u32),
    ) -> Option<Closed> {
        let _ = look;
        let mut closed = None;
        // Cancel and Done, top right
        let done = Rect::from_center_size(pos2(area.right() - 42.0, area.top() + 41.0), vec2(44.0, 44.0));
        ui.painter().circle_filled(done.center(), 22.0, Color32::from_rgb(0x01, 0x90, 0xFF));
        let _ = DONE;
        text(ui.painter(), done.center(), Align2::CENTER_CENTER, "✓", 22.0, Weight::Bold, Color32::WHITE);
        let cancel = Rect::from_min_size(pos2(done.left() - 13.0 - 73.0, done.top()), vec2(73.0, 43.0));
        ui.painter().rect_filled(cancel, 21.5, argb(BUTTON));
        ui.painter().rect_stroke(cancel, 21.5, egui::Stroke::new(1.0, argb(BUTTON_EDGE)), egui::StrokeKind::Inside);
        text(ui.painter(), cancel.center(), Align2::CENTER_CENTER, "Cancel", 16.0, Weight::Regular, argb(BUTTON_INK));
        if ui.interact(cancel, ui.scope_id().with("studio cancel"), Sense::click()).clicked() {
            closed = Some(Closed::Cancelled);
        }
        if ui.interact(done, ui.scope_id().with("studio done"), Sense::click()).clicked() {
            match self.save() {
                Ok(file) => {
                    self.generation.clone().save_lora_notes(self.edit.loras.clone());
                    closed = Some(Closed::Saved(file));
                }
                Err(e) => self.error = Some(format!("Couldn't save the style: {e}")),
            }
        }
        // the picture: the try shown, in the canvas's shape
        let bottom_room = 170.0;
        let frame = Rect::from_min_max(
            pos2(area.left() + 30.0, area.top() + 90.0),
            pos2(area.right() - 30.0, area.bottom() - bottom_room),
        );
        let shape = vec2(canvas.0.max(1) as f32, canvas.1.max(1) as f32);
        let spot = crate::generation::fit(frame, shape);
        ui.painter().rect_filled(spot, 12.0, argb(PREVIEW_FILL));
        let state = self.generation.studio_state();
        let shown = self.shown.and_then(|id| self.tries.iter().find(|(i, _, _)| *i == id));
        match shown {
            Some((_, texture, _)) => {
                let fitted = crate::generation::fit(spot, texture.size_vec2());
                ui.painter().image(
                    texture.id(),
                    fitted,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None if !state.running => {
                text(
                    ui.painter(),
                    spot.center(),
                    Align2::CENTER_CENTER,
                    "Try it shows the style as it is",
                    15.0,
                    Weight::Regular,
                    argb(DIM),
                );
            }
            None => {}
        }
        if state.running {
            crate::generation::ring(ui, spot.center(), 22.0, state.progress);
            if state.waiting {
                text(
                    ui.painter(),
                    spot.center() + vec2(0.0, 40.0),
                    Align2::CENTER_TOP,
                    "Waiting for the canvas's run",
                    13.0,
                    Weight::Regular,
                    argb(DIM),
                );
            }
        }
        // the tries, newest on the left, tapped to show
        let mut x = spot.left();
        let row_top = frame.bottom() + 12.0;
        for (id, texture, _) in &self.tries {
            let cell = Rect::from_min_size(pos2(x, row_top), vec2(56.0, 56.0));
            let fitted = crate::generation::fit(cell, texture.size_vec2());
            ui.painter().image(
                texture.id(),
                fitted,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            if self.shown == Some(*id) {
                ui.painter().rect_stroke(
                    fitted,
                    4.0,
                    egui::Stroke::new(2.0, argb(pc::ACCENT)),
                    egui::StrokeKind::Outside,
                );
            }
            if ui.interact(cell, ui.scope_id().with(("try", *id)), Sense::click()).clicked() {
                self.shown = Some(*id);
            }
            x += 64.0;
        }
        // the preview prompt, the dice, Try it and the seed
        let line_y = area.bottom() - 70.0;
        let field_rect =
            Rect::from_min_max(pos2(area.left() + 30.0, line_y - 22.0), pos2(area.right() - 30.0, line_y + 22.0));
        ui.painter().rect_filled(field_rect, 12.0, argb(FIELD));
        ui.scope_builder(UiBuilder::new().max_rect(field_rect.shrink2(vec2(12.0, 11.0))), |ui| {
            ui.add(
                egui::TextEdit::singleline(prompt)
                    .frame(egui::Frame::NONE)
                    .margin(vec2(0.0, 0.0))
                    .font(Weight::Regular.font(14.0))
                    .text_color(argb(LABEL))
                    .desired_width(f32::INFINITY),
            );
        });
        let buttons_y = area.bottom() - 26.0;
        let dice = Rect::from_center_size(pos2(area.left() + 30.0 + 20.0, buttons_y), vec2(40.0, 40.0));
        ui.painter().circle_filled(dice.center(), 20.0, argb(BUTTON));
        text(ui.painter(), dice.center(), Align2::CENTER_CENTER, "⚄", 20.0, Weight::Regular, argb(BUTTON_INK));
        if ui.interact(dice, ui.scope_id().with("studio dice"), Sense::click()).on_hover_text("A new seed").clicked() {
            self.seed = random_seed();
        }
        let words =
            if state.running { format!("Cancel · {:.0}%", state.progress * 100.0) } else { "Try it".to_owned() };
        let w = text_width(ui.painter(), &words, 15.0, Weight::SemiBold) + 40.0;
        let try_rect = Rect::from_min_size(pos2(dice.right() + 12.0, buttons_y - 19.0), vec2(w, 38.0));
        let fill = if state.running { Color32::from_gray(90) } else { argb(pc::ACCENT) };
        ui.painter().rect_filled(try_rect, 19.0, fill);
        text(ui.painter(), try_rect.center(), Align2::CENTER_CENTER, &words, 15.0, Weight::SemiBold, Color32::WHITE);
        if ui.interact(try_rect, ui.scope_id().with("studio try"), Sense::click()).clicked() {
            if state.running {
                self.generation.cancel_try();
            } else {
                self.error = None;
                let (w, h) = canvas;
                if let Err(e) = self.generation.try_style(self.edit.clone(), prompt.clone(), self.seed, w, h) {
                    self.error = Some(e.to_string());
                }
            }
        }
        let seed = format!("Seed {}", self.seed);
        text(
            ui.painter(),
            pos2(try_rect.right() + 14.0, buttons_y),
            Align2::LEFT_CENTER,
            &seed,
            12.0,
            Weight::Regular,
            argb(DIM),
        );
        if let Some(error) = &self.error {
            let at = pos2(area.left() + 30.0, frame.bottom() + 76.0);
            text(ui.painter(), at, Align2::LEFT_TOP, &shorten(error, 90), 12.5, Weight::Regular, argb(WARN));
        }
        closed
    }

    /// Writes the style's file; the file's name. A renamed style's old file goes.
    fn save(&self) -> std::io::Result<String> {
        let dir = crate::generation_styles::styles_dir();
        std::fs::create_dir_all(&dir)?;
        let name = if self.edit.name.trim().is_empty() { "Style".to_owned() } else { self.edit.name.trim().to_owned() };
        let mut edit = self.edit.clone();
        edit.name = name.clone();
        let file = self.generation.style_file_name(name, self.file.clone());
        let text = self.generation.style_text(edit, self.old_text.clone());
        std::fs::write(dir.join(&file), text)?;
        if let Some(old) = &self.file
            && old != &file
        {
            std::fs::remove_file(dir.join(old))?;
        }
        Ok(file)
    }
}

fn random_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64 % 1_000_000_000)
        .unwrap_or(7)
}

/// The list's choices with "the model's own" (empty) first.
fn own_first(list: &[String]) -> Vec<String> {
    std::iter::once(String::new()).chain(list.iter().cloned()).collect()
}

/// A LoRA's file as a name: folders and extension dropped, `_` as spaces.
fn procraft_name(file: &str) -> String {
    let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
    name.strip_suffix(".safetensors").unwrap_or(name).replace('_', " ")
}

fn shorten(text: &str, most: usize) -> String {
    if text.chars().count() <= most {
        return text.to_owned();
    }
    let cut: String = text.chars().take(most).collect();
    format!("{cut}…")
}

/// A section's heading.
fn heading(ui: &mut Ui, words: &str) {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    text(
        ui.painter(),
        pos2(row.left() + 8.0, row.center().y),
        Align2::LEFT_CENTER,
        words,
        21.0,
        Weight::Bold,
        argb(HEADER),
    );
    ui.add_space(6.0);
}

/// A small note under a setting.
fn note(ui: &mut Ui, words: &str) {
    ui.add_space(4.0);
    ui.add(egui::Label::new(egui::RichText::new(words).font(Weight::Regular.font(12.5)).color(argb(DIM))).wrap());
    ui.add_space(4.0);
}

/// A text field, one line or several; its response.
fn field(ui: &mut Ui, value: &mut String, many: bool, id: &str) -> egui::Response {
    let edit = if many { egui::TextEdit::multiline(value).desired_rows(4) } else { egui::TextEdit::singleline(value) };
    egui::Frame::NONE
        .fill(argb(FIELD))
        .corner_radius(12.0)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.add(
                edit.id_salt(id)
                    .frame(egui::Frame::NONE)
                    .margin(vec2(0.0, 0.0))
                    .font(Weight::Regular.font(14.0))
                    .text_color(argb(LABEL))
                    .desired_width(f32::INFINITY),
            )
        })
        .inner
}

/// A row with a name and its value, tapped to choose another; whether it was.
fn value_row(ui: &mut Ui, name: &str, value: &str, id: &str) -> bool {
    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
    ui.painter().rect_filled(row, 14.0, argb(LIST));
    text(
        ui.painter(),
        pos2(row.left() + 14.0, row.center().y),
        Align2::LEFT_CENTER,
        name,
        15.0,
        Weight::Regular,
        argb(LABEL),
    );
    let shown = shorten(value, 22);
    text(
        ui.painter(),
        pos2(row.right() - 30.0, row.center().y),
        Align2::RIGHT_CENTER,
        &shown,
        13.0,
        Weight::Regular,
        argb(DIM),
    );
    text(
        ui.painter(),
        pos2(row.right() - 14.0, row.center().y),
        Align2::RIGHT_CENTER,
        "›",
        17.0,
        Weight::Regular,
        argb(DIM),
    );
    let _ = id;
    response.clicked()
}

/// A row with a name and an Override switch; whether it was tapped.
fn switch_row(ui: &mut Ui, name: &str, on: bool, id: &str) -> bool {
    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
    ui.painter().rect_filled(row, 14.0, argb(LIST));
    text(
        ui.painter(),
        pos2(row.left() + 14.0, row.center().y),
        Align2::LEFT_CENTER,
        name,
        15.0,
        Weight::Regular,
        argb(LABEL),
    );
    text(
        ui.painter(),
        pos2(row.right() - 70.0, row.center().y),
        Align2::RIGHT_CENTER,
        "Override",
        12.0,
        Weight::Regular,
        argb(DIM),
    );
    let track = Rect::from_center_size(pos2(row.right() - 38.0, row.center().y), vec2(44.0, 26.0));
    ui.painter().rect_filled(track, 13.0, if on { argb(pc::ACCENT) } else { argb(LIST_LINE).gamma_multiply(3.0) });
    let knob = if on { track.right() - 13.0 } else { track.left() + 13.0 };
    ui.painter().circle_filled(pos2(knob, track.center().y), 10.0, Color32::WHITE);
    let _ = id;
    response.clicked()
}

/// A button-like row; whether it was tapped.
fn button_row(ui: &mut Ui, words: &str, id: &str) -> bool {
    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
    ui.painter().rect_filled(row, 14.0, argb(LIST));
    text(ui.painter(), row.center(), Align2::CENTER_CENTER, words, 15.0, Weight::Medium, argb(pc::ACCENT));
    let _ = id;
    response.clicked()
}

/// A slider with its name and value above it; where it was moved to (0..1), if it was.
fn labelled_slider(ui: &mut Ui, look: &mut Look, name: &str, value: &str, at: f32, id: &str) -> Option<f32> {
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
    text(
        ui.painter(),
        pos2(row.left() + 8.0, row.top() + 10.0),
        Align2::LEFT_CENTER,
        name,
        15.0,
        Weight::Regular,
        argb(LABEL),
    );
    text(
        ui.painter(),
        pos2(row.right() - 8.0, row.top() + 10.0),
        Align2::RIGHT_CENTER,
        value,
        14.0,
        Weight::Regular,
        argb(DIM),
    );
    let track = Rect::from_min_max(pos2(row.left() + 8.0, row.top() + 36.0), pos2(row.right() - 8.0, row.top() + 40.0));
    crate::generation::slider(ui, look, track, at.clamp(0.0, 1.0), id)
}

/// Rows to choose one of, the chosen one ticked, each with the family's badge; the row tapped.
fn list(ui: &mut Ui, rows: &[(&str, bool, &String)], id: &str) -> Option<usize> {
    let mut picked = None;
    for (i, (name, on, _family)) in rows.iter().enumerate() {
        let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
        ui.painter().rect_filled(row, if i == 0 { 14.0 } else { 0.0 }, argb(LIST));
        if i > 0 {
            ui.painter().line_segment(
                [pos2(row.left() + 14.0, row.top()), pos2(row.right(), row.top())],
                egui::Stroke::new(1.0, argb(LIST_LINE)),
            );
        }
        let ink = if name.ends_with("not on this server") { argb(WARN) } else { argb(LABEL) };
        text(
            ui.painter(),
            pos2(row.left() + 14.0, row.center().y),
            Align2::LEFT_CENTER,
            &shorten(name, 34),
            14.0,
            Weight::Regular,
            ink,
        );
        if *on {
            text(
                ui.painter(),
                pos2(row.right() - 14.0, row.center().y),
                Align2::RIGHT_CENTER,
                "✓",
                16.0,
                Weight::Bold,
                argb(pc::ACCENT),
            );
        }
        if response.clicked() {
            picked = Some(i);
        }
        let _ = id;
    }
    picked
}
