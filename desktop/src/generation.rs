//! Image generation, as the Android app's (ui/GenerationUi.kt): the Generate popover hanging from
//! its bar button (the styles, the prompt, strength, seed and count, and Generate; and its server
//! view: the address, what the server has, NSFW), and the results bar at the foot of the window
//! while a run goes or there are results (their thumbnails, the one shown over the canvas, Again,
//! Apply). What runs is the core's (generation.rs); this shows it and polls it.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use egui::{Align2, Color32, Id, Order, Rect, Sense, TextureHandle, TextureOptions, Ui, UiBuilder, pos2, vec2};
use procraft_core::{
    CanvasSession, Generation, GenerationResolution, GenerationSettings, GenerationState, RunSettings,
    ServerConnection, ServerStatus, SizeInfo, ThumbnailImage,
};

use crate::icons::{self, Icon, Part};
use crate::look::{Look, Themed, Weight, argb, pc, text, text_width};
use crate::panel;

/// "AI", drawn from the style guide's capsules: A's legs and the I 3 units thick, its bar 2.4.
pub const GENERATE: Icon = Icon {
    name: "generate",
    width: 44.0,
    height: 44.0,
    parts: &[
        Part { d: "M14.2 30L19.2 14L24.2 30", stroke: 3.0, fill: false, even_odd: false },
        Part { d: "M16.4 24.4L22 24.4", stroke: 2.4, fill: false, even_odd: false },
        Part { d: "M29.6 14L29.6 30", stroke: 3.0, fill: false, even_odd: false },
    ],
};

const WIDTH: f32 = 400.0;
const PAD: f32 = 16.0;
const LABEL: Themed = Themed(0x99FFFFFF, 0x99000000);
const FIELD: Themed = Themed(0x12FFFFFF, 0x12000000);
const ROW: Themed = Themed(0x0DFFFFFF, 0xFFD8D8D8);
const CARD: Themed = Themed(0x2E5B5B5B, 0xFFD7D7D7);
const INK: Themed = Themed(0xFFFFFFFF, 0xFF222222);
const DIM: Themed = Themed(0x99FFFFFF, 0x99000000);
const TRACK: Themed = Themed(0xFF414141, 0xFFBEBEBE);
const KNOB: Themed = Themed(0xFFEDEDED, 0xFF3D3D3D);
const BAR_FILL: Themed = Themed(0xF0202020, 0xF7F1F1F1);
const OK: u32 = 0xFF3CCF6E;
const WARN: u32 = 0xFFF2B440;
const BAD: u32 = 0xFFE84F51;
const THUMB: f32 = 66.0;
/// How often the run and the server are looked at while something is under way.
const POLL: Duration = Duration::from_millis(100);
/// The resolution choices, in the row's order.
const RESOLUTIONS: [GenerationResolution; 3] =
    [GenerationResolution::Draft, GenerationResolution::Normal, GenerationResolution::Quality];
const RESOLUTION_NAMES: [&str; 3] = ["Draft", "Normal", "Quality"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Form,
    Server,
}

/// What the results bar asked of the interface.
pub enum Asked {
    /// A notice to show (applied, failed).
    Notice(String, bool),
}

/// The Generate popover's and results bar's state, and the app's [`Generation`].
pub struct GenerationUi {
    pub generation: Arc<Generation>,
    pub settings: GenerationSettings,
    /// The settings changed and are to be kept.
    pub settings_changed: bool,
    view: View,
    prompt: String,
    style: String,
    strength: f32,
    /// The seed kept for the next run (none: a random one each time), and as typed.
    seed: Option<u64>,
    seed_text: String,
    count: u32,
    resolution: GenerationResolution,
    address: String,
    state: GenerationState,
    server: ServerStatus,
    thumbnails: HashMap<u64, TextureHandle>,
    preview: Option<(u64, TextureHandle)>,
    /// The last run's settings, for Again.
    last: Option<RunSettings>,
}

impl GenerationUi {
    /// With the server, NSFW, resolution and largest picture as kept in the preferences.
    pub fn from_prefs(prefs: &crate::prefs::Prefs) -> GenerationUi {
        let resolution = match prefs.generation_resolution {
            0 => GenerationResolution::Draft,
            1 => GenerationResolution::Normal,
            _ => GenerationResolution::Quality,
        };
        let largest = (prefs.generation_largest > 0.0).then_some(prefs.generation_largest);
        GenerationUi::new(
            GenerationSettings {
                address: prefs.generation_address.clone(),
                nsfw: prefs.generation_nsfw,
                largest_megapixels: largest,
            },
            resolution,
        )
    }

    pub fn new(settings: GenerationSettings, resolution: GenerationResolution) -> GenerationUi {
        let generation = Generation::new(settings.clone());
        let server = generation.server();
        GenerationUi {
            generation,
            address: settings.address.clone(),
            settings,
            settings_changed: false,
            view: View::Form,
            prompt: String::new(),
            style: String::new(),
            strength: 1.0,
            seed: None,
            seed_text: String::new(),
            count: 1,
            resolution,
            state: GenerationState::default(),
            server,
            thumbnails: HashMap::new(),
            preview: None,
            last: None,
        }
    }

    /// Looks at the run and the server again; asks for frames while something is under way.
    pub fn poll(&mut self, ctx: &egui::Context) {
        self.state = self.generation.state();
        self.server = self.generation.server();
        let busy = self.state.running.is_some() || self.server.connection == ServerConnection::Connecting;
        if busy {
            ctx.request_repaint_after(POLL);
        }
        let ids: Vec<u64> = self.state.results.iter().map(|r| r.id).collect();
        self.thumbnails.retain(|id, _| ids.contains(id));
        for r in &self.state.results {
            self.thumbnails.entry(r.id).or_insert_with(|| texture(ctx, &format!("result {}", r.id), &r.thumbnail));
        }
    }

    /// A run's progress (0..1) while one goes.
    pub fn progress(&self) -> Option<f32> {
        self.state.running.as_ref().map(|r| r.progress)
    }

    /// Whether the results bar has anything to show.
    pub fn has_results(&self) -> bool {
        self.state.running.is_some() || !self.state.results.is_empty() || self.state.error.is_some()
    }

    /// The artwork is closing: its run is cancelled and its results dropped.
    pub fn leave(&mut self, session: &CanvasSession) {
        self.generation.leave(session);
        self.state = GenerationState::default();
    }

    /// Opens the server view (from the Prefs tab).
    pub fn open_server(&mut self) {
        self.view = View::Server;
    }

    /// The popover hanging from `button`; returns where it is.
    pub fn popover(
        &mut self,
        ctx: &egui::Context,
        look: &mut Look,
        session: &CanvasSession,
        button: Rect,
        backdrops: &mut Vec<f32>,
    ) -> Rect {
        let screen = ctx.content_rect();
        let (left, arrow_x) = panel::place(ctx, WIDTH, button.center().x);
        let top = screen.top() + pc::BAR_HEIGHT;
        let content = match (self.view, self.server.connection) {
            (View::Server, _) => self.server_height(),
            (View::Form, ServerConnection::NotSet) => 230.0,
            (View::Form, _) => self.form_height(),
        };
        let height = (panel::ARROW_HEIGHT + content).min(screen.bottom() - 20.0 - top);
        let rect = Rect::from_min_size(pos2(left, top), vec2(WIDTH, height));
        egui::Area::new(Id::unique("generate popover")).fixed_pos(rect.min).order(Order::Foreground).show(ctx, |ui| {
            ui.allocate_rect(rect, Sense::click());
            panel::frame(ui, look, "generate frame", rect, arrow_x, backdrops);
            let body = panel::body(rect);
            match (self.view, self.server.connection) {
                (View::Server, _) => self.server_view(ui, look, body),
                (View::Form, ServerConnection::NotSet) => self.not_set_up(ui, look, body),
                (View::Form, _) => self.form(ui, look, session, body),
            }
        });
        rect
    }

    fn form_height(&self) -> f32 {
        let error = if self.state.error.is_some() { 40.0 } else { 0.0 };
        56.0 + 22.0 + 84.0 + 22.0 + 90.0 + 54.0 + 52.0 + 22.0 + 46.0 + 24.0 + 60.0 + error + PAD
    }

    fn server_height(&self) -> f32 {
        let missing: usize = self.server.features.iter().map(|f| f.missing.len().max(1)).sum();
        56.0 + 100.0 + 44.0 * 3.0 + 22.0 * missing as f32 + 30.0 + 56.0 + 52.0 + 70.0 + PAD
    }

    /// The title and, on its right, the connection: a dot and the server; tapped, the server view.
    fn title(&mut self, ui: &Ui, body: Rect) {
        let ink = argb(panel::PANEL_TITLE);
        text(
            ui.painter(),
            pos2(body.left() + 24.0, body.top() + 22.5),
            Align2::LEFT_TOP,
            "Generate",
            24.0,
            Weight::Bold,
            ink,
        );
        let (colour, words) = match self.server.connection {
            ServerConnection::NotSet => (BAD, "not set up".to_owned()),
            ServerConnection::Connecting => (WARN, "connecting".to_owned()),
            ServerConnection::Failed => (BAD, self.server.address.clone()),
            ServerConnection::Connected => {
                let all = self.server.features.iter().all(|f| f.ready);
                (if all { OK } else { WARN }, self.server.address.clone())
            }
        };
        let right = body.right() - 24.0;
        let at = pos2(right, body.top() + 36.0);
        let w = text_width(ui.painter(), &words, 13.0, Weight::Regular);
        text(ui.painter(), at, Align2::RIGHT_CENTER, &words, 13.0, Weight::Regular, argb(DIM));
        let dot = pos2(right - w - 10.0, at.y);
        ui.painter().circle_filled(dot, 4.5, argb(colour));
        let hit = Rect::from_min_max(pos2(dot.x - 10.0, at.y - 14.0), pos2(right, at.y + 14.0));
        if ui.interact(hit, ui.scope_id().with("server dot"), Sense::click()).clicked() {
            self.view = View::Server;
        }
    }

    fn form(&mut self, ui: &mut Ui, look: &mut Look, session: &CanvasSession, body: Rect) {
        self.title(ui, body);
        let (left, right) = (body.left() + PAD, body.right() - PAD);
        let mut y = body.top() + 56.0;
        // the styles: cards in a row, the chosen one ringed
        label(ui, pos2(left + 2.0, y), "STYLE");
        y += 22.0;
        let styles = self.generation.styles();
        if !styles.iter().any(|s| s.id == self.style)
            && let Some(first) = styles.first()
        {
            self.style = first.id.clone();
        }
        let mut x = left;
        for s in styles.iter().take(3) {
            let card = Rect::from_min_size(pos2(x, y), vec2(116.0, 76.0));
            let on = s.id == self.style;
            ui.painter().rect_filled(card, 12.0, argb(CARD));
            if on {
                ui.painter().rect_stroke(
                    card,
                    12.0,
                    egui::Stroke::new(2.0, argb(pc::ACCENT)),
                    egui::StrokeKind::Inside,
                );
            }
            let name = Rect::from_min_size(card.min + vec2(10.0, 10.0), vec2(card.width() - 20.0, 40.0));
            ui.scope_builder(UiBuilder::new().max_rect(name), |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(&s.name).font(Weight::SemiBold.font(14.0)).color(argb(INK)))
                        .wrap(),
                );
            });
            let sub = format!("{} steps · CFG {}", s.steps, s.cfg);
            text(
                ui.painter(),
                pos2(card.left() + 10.0, card.bottom() - 10.0),
                Align2::LEFT_BOTTOM,
                &sub,
                11.0,
                Weight::Regular,
                argb(DIM),
            );
            if ui.interact(card, ui.scope_id().with(("style", &s.id)), Sense::click()).clicked() {
                self.style = s.id.clone();
            }
            x += 116.0 + 6.0;
        }
        if styles.is_empty() {
            let words = "No Anima model on the server";
            text(
                ui.painter(),
                pos2(left + 2.0, y + 30.0),
                Align2::LEFT_CENTER,
                words,
                14.0,
                Weight::Regular,
                argb(DIM),
            );
        }
        y += 84.0;
        // the prompt
        label(ui, pos2(left + 2.0, y), "PROMPT");
        y += 22.0;
        let field = Rect::from_min_max(pos2(left, y), pos2(right, y + 82.0));
        ui.painter().rect_filled(field, 12.0, argb(FIELD));
        let inner = field.shrink2(vec2(12.0, 10.0));
        let edit = egui::TextEdit::multiline(&mut self.prompt)
            .font(Weight::Regular.font(15.0))
            .text_color(argb(INK))
            .frame(egui::Frame::NONE)
            .margin(vec2(0.0, 0.0))
            .hint_text("Describe it, or type tags…")
            .desired_width(inner.width())
            .desired_rows(3);
        let response = edit.show(&mut ui.new_child(UiBuilder::new().max_rect(inner))).response;
        let enter = response.has_focus() && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        y += 90.0;
        // strength
        let value = if self.strength >= 1.0 {
            "New image".to_owned()
        } else {
            format!("{:.0}% · Refine", self.strength * 100.0)
        };
        text(ui.painter(), pos2(left + 2.0, y), Align2::LEFT_TOP, "Strength", 14.0, Weight::Regular, argb(INK));
        text(ui.painter(), pos2(right - 2.0, y), Align2::RIGHT_TOP, &value, 14.0, Weight::Regular, argb(INK));
        let track = Rect::from_min_max(pos2(left + 8.0, y + 30.0), pos2(right - 8.0, y + 34.0));
        if let Some(t) = slider(ui, look, track, self.strength, "strength") {
            // the last stretch snaps to a new image
            self.strength = if t > 0.97 { 1.0 } else { (t * 100.0).round() / 100.0 };
        }
        y += 54.0;
        // seed and count
        let seed = Rect::from_min_max(pos2(left, y), pos2(right - 152.0 - 8.0, y + 40.0));
        ui.painter().rect_filled(seed, 12.0, argb(ROW));
        text(
            ui.painter(),
            pos2(seed.left() + 12.0, seed.center().y),
            Align2::LEFT_CENTER,
            "Seed",
            14.0,
            Weight::Regular,
            argb(INK),
        );
        match self.seed {
            None => {
                text(
                    ui.painter(),
                    pos2(seed.right() - 12.0, seed.center().y),
                    Align2::RIGHT_CENTER,
                    "Random",
                    14.0,
                    Weight::Regular,
                    argb(DIM),
                );
                if ui.interact(seed, ui.scope_id().with("seed"), Sense::click()).clicked() {
                    let s = self.state.results.last().map_or(1, |r| r.seed);
                    self.seed = Some(s);
                    self.seed_text = s.to_string();
                }
            }
            Some(_) => {
                let field = Rect::from_min_max(
                    pos2(seed.left() + 56.0, seed.top() + 8.0),
                    pos2(seed.right() - 34.0, seed.bottom() - 8.0),
                );
                egui::TextEdit::singleline(&mut self.seed_text)
                    .font(Weight::Regular.font(14.0))
                    .text_color(argb(INK))
                    .frame(egui::Frame::NONE)
                    .margin(vec2(0.0, 0.0))
                    .align(Align2::RIGHT_CENTER)
                    .desired_width(field.width())
                    .show(&mut ui.new_child(UiBuilder::new().max_rect(field)));
                self.seed = Some(self.seed_text.trim().parse().unwrap_or(0));
                let clear = Rect::from_center_size(pos2(seed.right() - 18.0, seed.center().y), vec2(24.0, 24.0));
                text(ui.painter(), clear.center(), Align2::CENTER_CENTER, "×", 18.0, Weight::Regular, argb(DIM));
                if ui.interact(clear, ui.scope_id().with("seed random"), Sense::click()).clicked() {
                    self.seed = None;
                }
            }
        }
        let seg = Rect::from_min_max(pos2(right - 152.0, y), pos2(right, y + 40.0));
        ui.painter().rect_filled(seg, 12.0, argb(ROW));
        for n in 1..=4u32 {
            let cell =
                Rect::from_min_size(pos2(seg.left() + 3.0 + (n - 1) as f32 * 36.5, seg.top() + 3.0), vec2(36.5, 34.0));
            let on = self.count == n;
            if on {
                ui.painter().rect_filled(cell, 9.0, argb(pc::ACCENT));
            }
            let ink = if on { Color32::WHITE } else { argb(DIM) };
            text(ui.painter(), cell.center(), Align2::CENTER_CENTER, &n.to_string(), 14.0, Weight::Medium, ink);
            if ui.interact(cell, ui.scope_id().with(("count", n)), Sense::click()).clicked() {
                self.count = n;
            }
        }
        y += 52.0;
        // resolution: three choices, then what the chosen one makes of this area
        label(ui, pos2(left + 2.0, y), "RESOLUTION");
        y += 22.0;
        let seg = Rect::from_min_max(pos2(left, y), pos2(right, y + 40.0));
        ui.painter().rect_filled(seg, 12.0, argb(ROW));
        let cell_w = (seg.width() - 6.0) / 3.0;
        for (i, (&resolution, name)) in RESOLUTIONS.iter().zip(RESOLUTION_NAMES).enumerate() {
            let cell = Rect::from_min_max(
                pos2(seg.left() + 3.0 + i as f32 * cell_w, seg.top() + 3.0),
                pos2(seg.left() + 3.0 + (i + 1) as f32 * cell_w - 2.0, seg.bottom() - 3.0),
            );
            let on = self.resolution == resolution;
            if on {
                ui.painter().rect_filled(cell, 9.0, argb(pc::ACCENT));
            }
            let ink = if on { Color32::WHITE } else { argb(DIM) };
            text(ui.painter(), cell.center(), Align2::CENTER_CENTER, name, 14.0, Weight::Medium, ink);
            if ui.interact(cell, ui.scope_id().with(("resolution", i)), Sense::click()).clicked()
                && self.resolution != resolution
            {
                self.resolution = resolution;
                self.settings_changed = true;
            }
        }
        y += 46.0;
        let info = self.generation.size_for(session, self.resolution);
        let detail = plan_text(&info, self.largest());
        text(ui.painter(), pos2(left + 2.0, y), Align2::LEFT_TOP, &detail, 12.0, Weight::Regular, argb(DIM));
        y += 24.0;
        // Generate, or Cancel while a run goes
        let go = Rect::from_min_max(pos2(left, y), pos2(right, y + 48.0));
        let running = self.state.running.as_ref();
        let ready = self.server.connection == ServerConnection::Connected && !styles.is_empty();
        let words = match running {
            Some(r) => format!("Cancel · {:.0}%", r.progress * 100.0),
            None if self.count > 1 => format!("Generate {}", self.count),
            None => "Generate".to_owned(),
        };
        let fill = if running.is_some() { Color32::from_gray(90) } else { argb(pc::ACCENT) };
        let fill = if ready || running.is_some() { fill } else { fill.gamma_multiply(0.4) };
        ui.painter().rect_filled(go, 14.0, fill);
        text(ui.painter(), go.center(), Align2::CENTER_CENTER, &words, 16.0, Weight::SemiBold, Color32::WHITE);
        let tapped =
            ui.interact(go, ui.scope_id().with("go"), Sense::click()).clicked() || (enter && running.is_none());
        if tapped {
            if running.is_some() {
                self.generation.cancel();
            } else if ready {
                self.run(session, None);
            }
        }
        y += 60.0;
        if let Some(error) = &self.state.error {
            let at = Rect::from_min_max(pos2(left + 2.0, y), pos2(right, y + 36.0));
            ui.scope_builder(UiBuilder::new().max_rect(at), |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(error).font(Weight::Regular.font(13.0)).color(argb(WARN)))
                        .wrap(),
                );
            });
        }
    }

    /// Starts a run with the form's settings (a new seed unless one is kept, or `seed`).
    fn run(&mut self, session: &CanvasSession, seed: Option<u64>) {
        let run = RunSettings {
            prompt: self.prompt.clone(),
            style: self.style.clone(),
            strength: self.strength,
            seed: seed.or(self.seed),
            count: self.count,
            resolution: Some(self.resolution),
        };
        match self.generation.generate(session, run.clone()) {
            Ok(_) => self.last = Some(run),
            Err(e) => self.state.error = Some(e.to_string()),
        }
    }

    /// The resolution choice as the preferences keep it: Draft 0, Normal 1, Quality 2.
    pub fn resolution_index(&self) -> usize {
        RESOLUTIONS.iter().position(|r| *r == self.resolution).unwrap_or(2)
    }

    /// The largest picture as the preferences keep it; 0 when it's the GPU's suggestion.
    pub fn largest_for_prefs(&self) -> f32 {
        self.settings.largest_megapixels.unwrap_or(0.0)
    }

    /// The largest picture in force: the setting, else the GPU's suggestion.
    fn largest(&self) -> f32 {
        self.settings.largest_megapixels.unwrap_or(self.server.largest_megapixels).clamp(1.0, 16.0)
    }

    /// Uses `largest` from now on (none: the GPU's suggestion) and keeps it.
    fn set_largest(&mut self, largest: Option<f32>) {
        self.settings.largest_megapixels = largest.map(|mp| mp.clamp(1.0, 16.0));
        self.generation.set_settings(self.settings.clone());
        self.settings_changed = true;
    }

    fn not_set_up(&mut self, ui: &mut Ui, look: &mut Look, body: Rect) {
        self.title(ui, body);
        let centre = body.center().x;
        look.icon(ui, &GENERATE, pos2(centre, body.top() + 92.0), 54.0, argb(DIM));
        let words = ["Generate needs ComfyUI running on a PC.", "Set its address to connect."];
        for (i, w) in words.iter().enumerate() {
            let at = pos2(centre, body.top() + 128.0 + i as f32 * 20.0);
            text(ui.painter(), at, Align2::CENTER_TOP, w, 14.0, Weight::Regular, argb(INK));
        }
        let go = Rect::from_center_size(pos2(centre, body.top() + 196.0), vec2(200.0, 42.0));
        ui.painter().rect_filled(go, 14.0, argb(pc::ACCENT));
        text(ui.painter(), go.center(), Align2::CENTER_CENTER, "Set up", 16.0, Weight::SemiBold, Color32::WHITE);
        if ui.interact(go, ui.scope_id().with("set up"), Sense::click()).clicked() {
            self.view = View::Server;
        }
    }

    /// The server: its address, what it has and what that's enough for, NSFW, and the notes.
    fn server_view(&mut self, ui: &mut Ui, look: &mut Look, body: Rect) {
        let back = Rect::from_min_size(pos2(body.left() + 8.0, body.top() + 17.0), vec2(44.0, 34.0));
        look.icon(ui, &icons::CHEVRON_LEFT, back.center(), 22.0, argb(panel::PANEL_TITLE));
        let title = pos2(back.right(), body.top() + 34.0);
        text(
            ui.painter(),
            title,
            Align2::LEFT_CENTER,
            "Image generation",
            22.0,
            Weight::Bold,
            argb(panel::PANEL_TITLE),
        );
        if ui.interact(back, ui.scope_id().with("back"), Sense::click()).clicked() {
            self.view = View::Form;
        }
        let (left, right) = (body.left() + PAD, body.right() - PAD);
        let mut y = body.top() + 64.0;
        // the address and Connect
        let group = Rect::from_min_max(pos2(left, y), pos2(right, y + 88.0));
        ui.painter().rect_filled(group, 14.0, argb(ROW));
        text(
            ui.painter(),
            pos2(left + 16.0, y + 22.0),
            Align2::LEFT_CENTER,
            "Server",
            15.0,
            Weight::Regular,
            argb(INK),
        );
        let field = Rect::from_min_max(pos2(left + 84.0, y + 8.0), pos2(right - 10.0, y + 36.0));
        ui.painter().rect_filled(field, 9.0, argb(FIELD));
        let edit = egui::TextEdit::singleline(&mut self.address)
            .font(Weight::Regular.font(14.0))
            .text_color(argb(INK))
            .frame(egui::Frame::NONE)
            .margin(vec2(0.0, 0.0))
            .hint_text("192.168.1.10:8188")
            .desired_width(field.width() - 16.0)
            .show(&mut ui.new_child(UiBuilder::new().max_rect(field.shrink2(vec2(8.0, 6.0)))));
        let enter = edit.response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let connect = Rect::from_min_max(pos2(left, y + 44.0), pos2(right, y + 88.0));
        text(ui.painter(), connect.center(), Align2::CENTER_CENTER, "Connect", 15.0, Weight::Medium, argb(pc::ACCENT));
        if ui.interact(connect, ui.scope_id().with("connect"), Sense::click()).clicked() || enter {
            let address = procraft_core::generation_address(self.address.clone()).unwrap_or_default();
            self.address = address.clone();
            if address == self.settings.address {
                self.generation.connect();
            } else {
                self.settings.address = address;
                self.generation.set_settings(self.settings.clone());
                self.settings_changed = true;
            }
        }
        y += 100.0;
        // what the server is and has
        let lines = self.status_lines();
        let height: f32 = lines.iter().map(|l| 22.0 + 22.0 * l.2.len() as f32).sum::<f32>() + 16.0;
        let group = Rect::from_min_max(pos2(left, y), pos2(right, y + height));
        ui.painter().rect_filled(group, 14.0, argb(ROW));
        let mut ly = y + 8.0;
        for (name, (mark, colour), details) in &lines {
            text(
                ui.painter(),
                pos2(left + 16.0, ly + 11.0),
                Align2::LEFT_CENTER,
                name,
                14.0,
                Weight::Regular,
                argb(INK),
            );
            text(
                ui.painter(),
                pos2(right - 16.0, ly + 11.0),
                Align2::RIGHT_CENTER,
                mark,
                14.0,
                Weight::Medium,
                argb(*colour),
            );
            ly += 22.0;
            for d in details {
                text(
                    ui.painter(),
                    pos2(left + 28.0, ly + 10.0),
                    Align2::LEFT_CENTER,
                    d,
                    12.0,
                    Weight::Regular,
                    argb(DIM),
                );
                ly += 22.0;
            }
        }
        y += height + 12.0;
        // the largest picture: the GPU's suggestion until it's changed, then 1–16 MP by halves
        let row = Rect::from_min_max(pos2(left, y), pos2(right, y + 44.0));
        ui.painter().rect_filled(row, 14.0, argb(ROW));
        text(
            ui.painter(),
            pos2(left + 16.0, row.center().y),
            Align2::LEFT_CENTER,
            "Largest picture",
            15.0,
            Weight::Regular,
            argb(INK),
        );
        let current = self.largest();
        let value = Rect::from_center_size(pos2(right - 92.0, row.center().y), vec2(104.0, 32.0));
        let minus = Rect::from_center_size(pos2(value.left() - 18.0, row.center().y), vec2(28.0, 28.0));
        let plus = Rect::from_center_size(pos2(right - 18.0, row.center().y), vec2(28.0, 28.0));
        text(ui.painter(), minus.center(), Align2::CENTER_CENTER, "−", 20.0, Weight::Regular, argb(DIM));
        text(ui.painter(), plus.center(), Align2::CENTER_CENTER, "+", 20.0, Weight::Regular, argb(DIM));
        let words = match self.settings.largest_megapixels {
            Some(_) => format!("{} MP", megapixels(current)),
            None => format!("Auto · {} MP", megapixels(current)),
        };
        text(ui.painter(), value.center(), Align2::CENTER_CENTER, &words, 14.0, Weight::Regular, argb(INK));
        if ui.interact(minus, ui.scope_id().with("largest less"), Sense::click()).clicked() {
            self.set_largest(Some((current - 0.5).max(1.0)));
        }
        if ui.interact(plus, ui.scope_id().with("largest more"), Sense::click()).clicked() {
            self.set_largest(Some((current + 0.5).min(16.0)));
        }
        if ui.interact(value, ui.scope_id().with("largest auto"), Sense::click()).clicked() {
            self.set_largest(None);
        }
        y += 56.0;
        // NSFW
        let row = Rect::from_min_max(pos2(left, y), pos2(right, y + 44.0));
        ui.painter().rect_filled(row, 14.0, argb(ROW));
        text(
            ui.painter(),
            pos2(left + 16.0, row.center().y),
            Align2::LEFT_CENTER,
            "Show NSFW tags",
            15.0,
            Weight::Regular,
            argb(INK),
        );
        let knob = Rect::from_center_size(pos2(right - 38.0, row.center().y), vec2(50.0, 30.0));
        ui.painter().rect_filled(
            knob,
            15.0,
            if self.settings.nsfw { argb(pc::ACCENT) } else { Color32::from_rgb(0x74, 0x73, 0x77) },
        );
        let kx = if self.settings.nsfw { knob.right() - 15.0 } else { knob.left() + 15.0 };
        ui.painter().circle_filled(pos2(kx, knob.center().y), 12.0, Color32::WHITE);
        if ui.interact(row, ui.scope_id().with("nsfw"), Sense::click()).clicked() {
            self.settings.nsfw = !self.settings.nsfw;
            self.generation.set_settings(self.settings.clone());
            self.settings_changed = true;
        }
        y += 56.0;
        let mut notes = vec!["Pictures and prompts go to this server and nowhere else.".to_owned()];
        if !self.server.licence.is_empty() {
            notes.push(format!("Anima: {}", self.server.licence));
        }
        for n in notes {
            let at = Rect::from_min_max(pos2(left + 4.0, y), pos2(right - 4.0, y + 32.0));
            ui.scope_builder(UiBuilder::new().max_rect(at), |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(n).font(Weight::Regular.font(12.5)).color(argb(DIM))).wrap(),
                );
            });
            y += 34.0;
        }
    }

    /// The server's status as lines: a name, a mark and its colour, and lines under it.
    fn status_lines(&self) -> Vec<(String, (String, u32), Vec<String>)> {
        let s = &self.server;
        match s.connection {
            ServerConnection::NotSet => vec![("Not set up".to_owned(), (String::new(), OK), Vec::new())],
            ServerConnection::Connecting => vec![("Connecting…".to_owned(), (String::new(), OK), Vec::new())],
            ServerConnection::Failed => {
                let why = s.problem.clone().unwrap_or_default();
                vec![("Can't connect".to_owned(), ("✕".to_owned(), BAD), wrap(&why, 52))]
            }
            ServerConnection::Connected => {
                let mut lines = vec![
                    ("ComfyUI".to_owned(), (format!("{} ✓", s.version), OK), Vec::new()),
                    (
                        "Anima models".to_owned(),
                        (format!("{} found", s.checkpoints), if s.checkpoints > 0 { OK } else { BAD }),
                        Vec::new(),
                    ),
                ];
                for f in &s.features {
                    let mark = if f.ready { ("✓".to_owned(), OK) } else { ("✕".to_owned(), BAD) };
                    let details = f.missing.iter().map(|m| format!("needs {m}")).collect();
                    lines.push((f.label.clone(), mark, details));
                }
                lines
            }
        }
    }

    /// The bar at the foot of the window: the results, the run going, Again and Apply.
    pub fn results_bar(
        &mut self,
        ctx: &egui::Context,
        look: &mut Look,
        session: &CanvasSession,
        backdrops: &mut Vec<f32>,
    ) -> Option<Asked> {
        if !self.has_results() {
            return None;
        }
        let screen = ctx.content_rect();
        let width = (screen.width() - 2.0 * 140.0).clamp(420.0, 900.0);
        let rect = Rect::from_min_size(
            pos2(screen.center().x - width / 2.0, screen.bottom() - 22.0 - 96.0),
            vec2(width, 96.0),
        );
        let mut asked = None;
        egui::Area::new(Id::unique("generate results")).fixed_pos(rect.min).order(Order::Foreground).show(ctx, |ui| {
            ui.allocate_rect(rect, Sense::click());
            panel::floating_frame(ui, look, "generate results frame", rect, backdrops);
            ui.painter().rect_filled(rect, 22.0, argb(BAR_FILL));
            let cy = rect.center().y;
            let mut x = rect.left() + 16.0;
            // the thumbnails, oldest first, as many as fit before the buttons
            let room = ((width - 16.0 - 330.0) / (THUMB + 10.0)).floor().max(1.0) as usize;
            let shown = self.state.shown;
            let skip = (self.state.results.len() + usize::from(self.state.running.is_some())).saturating_sub(room);
            for r in self.state.results.iter().skip(skip) {
                let cell = Rect::from_min_size(pos2(x, cy - THUMB / 2.0), vec2(THUMB, THUMB));
                if let Some(t) = self.thumbnails.get(&r.id) {
                    ui.painter().image(
                        t.id(),
                        fit(cell, t.size_vec2()),
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                if shown == Some(r.id) {
                    ui.painter().rect_stroke(
                        cell,
                        10.0,
                        egui::Stroke::new(2.5, argb(pc::ACCENT)),
                        egui::StrokeKind::Outside,
                    );
                }
                if ui.interact(cell, ui.scope_id().with(("result", r.id)), Sense::click()).clicked() {
                    let next = if shown == Some(r.id) { None } else { Some(r.id) };
                    self.generation.show_result(session, next);
                }
                x += THUMB + 10.0;
            }
            if let Some(run) = &self.state.running {
                let cell = Rect::from_min_size(pos2(x, cy - THUMB / 2.0), vec2(THUMB, THUMB));
                ui.painter().rect_filled(cell, 10.0, Color32::from_gray(42));
                if let Some(p) = &run.preview {
                    // a texture made again only for a new preview
                    let key = (run.id << 32) | u64::from(run.previews);
                    if self.preview.as_ref().is_none_or(|(k, _)| *k != key) {
                        self.preview = Some((key, texture(ctx, "generate preview", p)));
                    }
                    if let Some((_, t)) = &self.preview {
                        ui.painter().image(
                            t.id(),
                            fit(cell, t.size_vec2()),
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::from_white_alpha(160),
                        );
                    }
                }
                ring(ui, cell.center(), 16.0, run.progress);
                if ui.interact(cell, ui.scope_id().with("running"), Sense::click()).clicked() {
                    self.generation.cancel();
                }
            }
            // what the shown picture was made at, when the area is much larger
            if let Some(r) = self.state.shown.and_then(|id| self.state.results.iter().find(|r| r.id == id))
                && r.scaled_from_width > 0
            {
                let words = format!("scaled up from {} × {}", r.scaled_from_width, r.scaled_from_height);
                text(
                    ui.painter(),
                    pos2(x + 4.0, cy - 12.0),
                    Align2::LEFT_CENTER,
                    &words,
                    12.0,
                    Weight::Regular,
                    argb(DIM),
                );
            }
            // the buttons from the right: ✕, Apply, Again, the seed
            let mut bx = rect.right() - 16.0;
            let close = Rect::from_min_max(pos2(bx - 30.0, cy - 15.0), pos2(bx, cy + 15.0));
            text(ui.painter(), close.center(), Align2::CENTER_CENTER, "×", 24.0, Weight::Regular, argb(DIM));
            if ui.interact(close, ui.scope_id().with("discard"), Sense::click()).clicked() {
                self.generation.discard_results(session);
            }
            bx = close.left() - 12.0;
            let target = shown.or(self.state.results.last().map(|r| r.id));
            if pill(ui, &mut bx, cy, "Apply", true, target.is_some())
                && let Some(id) = target
            {
                match self.generation.apply_result(session, id) {
                    Some(_) => asked = Some(Asked::Notice("Added as a layer".to_owned(), false)),
                    None => asked = Some(Asked::Notice("No room for another layer".to_owned(), true)),
                }
            }
            let again = self.last.is_some() && self.state.running.is_none();
            if pill(ui, &mut bx, cy, "Again", false, again)
                && let Some(last) = self.last.clone()
                && let Err(e) = self.generation.generate(session, last)
            {
                asked = Some(Asked::Notice(e.to_string(), true));
            }
            if let Some(r) = target.and_then(|id| self.state.results.iter().find(|r| r.id == id)) {
                let words = format!("seed {}", r.seed);
                let w = text_width(ui.painter(), &words, 12.0, Weight::Regular) + 20.0;
                let tag = Rect::from_min_max(pos2(bx - w, cy - 15.0), pos2(bx, cy + 15.0));
                ui.painter().rect_filled(tag, 10.0, argb(ROW));
                text(ui.painter(), tag.center(), Align2::CENTER_CENTER, &words, 12.0, Weight::Regular, argb(DIM));
                if ui.interact(tag, ui.scope_id().with("seed tag"), Sense::click()).clicked() {
                    self.seed = Some(r.seed);
                    self.seed_text = r.seed.to_string();
                }
            }
            if let Some(error) = &self.state.error {
                let at = pos2(x + 4.0, cy);
                text(ui.painter(), at, Align2::LEFT_CENTER, &shorten(error, 48), 12.0, Weight::Regular, argb(WARN));
            }
        });
        asked
    }
}

/// A label over a part of the form.
fn label(ui: &Ui, at: egui::Pos2, words: &str) {
    text(ui.painter(), at, Align2::LEFT_TOP, words, 12.0, Weight::Medium, argb(LABEL));
}

/// A slider along `track` at `value` (0..1); where it's dragged or tapped to, if it was.
fn slider(ui: &Ui, _look: &mut Look, track: Rect, value: f32, id: &str) -> Option<f32> {
    let hit = track.expand2(vec2(10.0, 14.0));
    let response = ui.interact(hit, ui.scope_id().with(id), Sense::click_and_drag());
    let at = track.left() + track.width() * value.clamp(0.0, 1.0);
    ui.painter().rect_filled(track, 2.0, argb(TRACK));
    ui.painter().rect_filled(Rect::from_min_max(track.min, pos2(at, track.bottom())), 2.0, argb(KNOB));
    let knob = Rect::from_center_size(
        pos2(at.clamp(track.left() + 15.0, track.right() - 15.0), track.center().y),
        vec2(30.0, 18.0),
    );
    ui.painter().rect_filled(knob, 9.0, argb(KNOB));
    if response.dragged() || response.clicked() {
        let x = response.interact_pointer_pos()?.x;
        return Some(((x - track.left()) / track.width()).clamp(0.0, 1.0));
    }
    None
}

/// A button from `x` leftwards (its right edge), filled blue if `primary`; whether it was tapped.
fn pill(ui: &Ui, x: &mut f32, cy: f32, words: &str, primary: bool, enabled: bool) -> bool {
    let w = text_width(ui.painter(), words, 14.0, Weight::Medium) + 32.0;
    let rect = Rect::from_min_max(pos2(*x - w, cy - 19.0), pos2(*x, cy + 19.0));
    *x = rect.left() - 10.0;
    let fill = if primary { argb(pc::ACCENT) } else { argb(ROW) };
    let alpha = if enabled { 1.0 } else { 0.4 };
    ui.painter().rect_filled(rect, 12.0, fill.gamma_multiply(alpha));
    let ink = if primary { Color32::WHITE } else { argb(INK) };
    text(ui.painter(), rect.center(), Align2::CENTER_CENTER, words, 14.0, Weight::Medium, ink.gamma_multiply(alpha));
    enabled && ui.interact(rect, ui.scope_id().with(("pill", words)), Sense::click()).clicked()
}

/// A progress ring: grey all round, blue for the part done.
fn ring(ui: &Ui, centre: egui::Pos2, radius: f32, done: f32) {
    ui.painter().circle_stroke(centre, radius, egui::Stroke::new(3.0, Color32::from_gray(70)));
    let steps = 48;
    let end = (done.clamp(0.0, 1.0) * steps as f32).round() as usize;
    let points: Vec<egui::Pos2> = (0..=end)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 / steps as f32 * std::f32::consts::TAU;
            centre + vec2(a.cos(), a.sin()) * radius
        })
        .collect();
    if points.len() > 1 {
        ui.painter().add(egui::Shape::line(points, egui::Stroke::new(3.0, argb(pc::ACCENT))));
    }
}

/// `size` fitted inside `cell`, centred.
fn fit(cell: Rect, size: egui::Vec2) -> Rect {
    let scale = (cell.width() / size.x.max(1.0)).min(cell.height() / size.y.max(1.0));
    Rect::from_center_size(cell.center(), size * scale)
}

fn texture(ctx: &egui::Context, name: &str, image: &ThumbnailImage) -> TextureHandle {
    let size = [image.width as usize, image.height as usize];
    let picture = egui::ColorImage::from_rgba_premultiplied(size, &image.rgba);
    ctx.load_texture(name, picture, TextureOptions::LINEAR)
}

/// The small text under the resolution row: what will be made, and how.
fn plan_text(size: &SizeInfo, largest: f32) -> String {
    let mut words = format!("{} × {}", size.width, size.height);
    if size.passes > 1 {
        words.push_str(&format!(" · {} passes", size.passes));
    } else if size.scaled_up {
        words.push_str(" → scaled up");
    } else {
        words.push_str(" · 1 pass");
    }
    if size.capped {
        words.push_str(&format!(" · capped at {} MP", megapixels(largest)));
    }
    words
}

/// `mp` without a needless `.0`.
fn megapixels(mp: f32) -> String {
    if (mp - mp.round()).abs() < 0.01 { format!("{mp:.0}") } else { format!("{mp:.1}") }
}

/// `text` cut to `most` characters.
fn shorten(text: &str, most: usize) -> String {
    let mut cut: String = text.chars().take(most).collect();
    if cut.len() < text.len() {
        cut.push('…');
    }
    cut
}

/// `text` in lines of at most `width` characters, broken at spaces.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let fits = lines.last().is_some_and(|l| l.chars().count() + word.chars().count() < width);
        if !fits && lines.last().is_some_and(|l| !l.is_empty()) {
            lines.push(String::new());
        }
        if let Some(line) = lines.last_mut() {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    lines
}
