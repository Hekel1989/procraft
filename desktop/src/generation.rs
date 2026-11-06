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
    CanvasSession, ControlLayerInfo, ControlSetting, ControlStep, Generation, GenerationMode, GenerationResolution,
    GenerationSettings, GenerationState, LayerControl, RunSettings, SelectionInfo, ServerConnection, ServerStatus,
    SizeInfo, ThumbnailImage,
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

/// Trace: a drawn line and a four-point sparkle, "lines, made for you" (24-unit grid).
const TRACE: Icon = Icon {
    name: "trace",
    width: 24.0,
    height: 24.0,
    parts: &[
        Part {
            d: "M3.2 19.5C5.4 13.9 7.8 12.1 9.8 14.9C11.8 17.7 13.8 18.3 16.2 12.7",
            stroke: 2.4,
            fill: false,
            even_odd: false,
        },
        Part {
            d: "M17.4 2Q18.08 5.12 21.2 5.8Q18.08 6.48 17.4 9.6Q16.72 6.48 13.6 5.8Q16.72 5.12 17.4 2Z",
            stroke: 0.0,
            fill: true,
            even_odd: false,
        },
    ],
};

/// Again: a round arrow coming back on itself (24-unit grid).
const AGAIN: Icon = Icon {
    name: "again",
    width: 24.0,
    height: 24.0,
    parts: &[
        Part { d: "M18.6 9.6A7 7 0 1 1 12 5", stroke: 2.4, fill: false, even_odd: false },
        Part { d: "M11 1.6L16.4 5L11 8.4Z", stroke: 0.0, fill: true, even_odd: false },
    ],
};

/// The Control part's + Line art / + Scribble row.
const ADD_ROW: f32 = 40.0;
/// The results tray's round buttons (↻, ···, ⌄), across.
const ROUND: f32 = 40.0;
/// The tray tab's height.
const TAB_HEIGHT: f32 = 42.0;

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
const MENU_FILL: Themed = Themed(0xF7313135, 0xF7FAFAFA);
const MENU_LINE: Themed = Themed(0x1FFFFFFF, 0x1F000000);
const OK: u32 = 0xFF3CCF6E;
const WARN: u32 = 0xFFF2B440;
const BAD: u32 = 0xFFE84F51;
const THUMB: f32 = 66.0;
/// A control layer's row in the form.
const CONTROL_ROW: f32 = 76.0;
/// The results bar's ‹ and › when it's scrolled.
const CHEVRON: f32 = 24.0;
/// How long a thumbnail is held, in seconds, before its Delete shows.
const HOLD: f64 = 0.5;
/// How often the run and the server are looked at while something is under way.
const POLL: Duration = Duration::from_millis(100);
/// The resolution choices, in the row's order.
const RESOLUTIONS: [GenerationResolution; 3] =
    [GenerationResolution::Draft, GenerationResolution::Normal, GenerationResolution::Quality];
const RESOLUTION_NAMES: [&str; 3] = ["Draft", "Normal", "Quality"];
const STEPS: [ControlStep; 4] = [ControlStep::Loose, ControlStep::Balanced, ControlStep::Close, ControlStep::Strict];
const STEP_NAMES: [&str; 4] = ["Loose", "Balanced", "Close", "Strict"];
const MODES: [GenerationMode; 4] =
    [GenerationMode::Fill, GenerationMode::Add, GenerationMode::Remove, GenerationMode::Expand];
const MODE_NAMES: [&str; 4] = ["Fill", "Add", "Remove", "Expand"];

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
    /// The prompt's caret as a byte offset, as the field last reported it.
    prompt_caret: usize,
    style: String,
    strength: f32,
    /// The seed kept for the next run (none: a random one each time), and as typed.
    seed: Option<u64>,
    seed_text: String,
    count: u32,
    resolution: GenerationResolution,
    /// The selection as the form last found it, the mode picked (none: the guess), and the box the
    /// pick belongs to (a new selection guesses again).
    selection: Option<SelectionInfo>,
    selection_area: Option<(u32, u32, u32, u32)>,
    mode: Option<GenerationMode>,
    address: String,
    state: GenerationState,
    server: ServerStatus,
    thumbnails: HashMap<u64, TextureHandle>,
    preview: Option<(u64, TextureHandle)>,
    /// How many results the bar is scrolled back from the newest.
    back: usize,
    /// The result held down (or right-clicked), showing its Delete above it.
    deleting: Option<u64>,
    /// The results tray is up (else tucked into its tab); its ··· menu is open; "Clear all
    /// results…" is being asked; a run was going at the last look (its first picture brings the
    /// tray up).
    tray_up: bool,
    tray_menu: bool,
    clear_alert: crate::alert::Alert,
    was_running: bool,
    /// The last run's settings, for Again.
    last: Option<RunSettings>,
    /// The control layers the popover lists, their pictures, and the step each was given while the
    /// app runs.
    controls: Vec<ControlLayerInfo>,
    control_signature: Vec<(u64, String, Option<LayerControl>, bool, u64)>,
    control_pictures: HashMap<u64, TextureHandle>,
    control_steps: HashMap<u64, ControlStep>,
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
        let ui = GenerationUi::new(
            GenerationSettings {
                address: prefs.generation_address.clone(),
                nsfw: prefs.generation_nsfw,
                largest_megapixels: largest,
            },
            resolution,
        );
        ui.generation.set_recent_prompts(prefs.generation_recent.clone());
        crate::prefs::set_generation_recent(prefs.generation_recent.clone());
        ui
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
            prompt_caret: 0,
            style: String::new(),
            strength: 1.0,
            seed: None,
            seed_text: String::new(),
            count: 1,
            resolution,
            selection: None,
            selection_area: None,
            mode: None,
            state: GenerationState::default(),
            server,
            thumbnails: HashMap::new(),
            preview: None,
            back: 0,
            deleting: None,
            tray_up: false,
            tray_menu: false,
            clear_alert: crate::alert::Alert::default(),
            was_running: false,
            last: None,
            controls: Vec::new(),
            control_signature: Vec::new(),
            control_pictures: HashMap::new(),
            control_steps: HashMap::new(),
        }
    }

    /// Looks at the run and the server again; asks for frames while something is under way.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let newest = self.state.results.last().map(|r| r.id);
        let failed = self.state.error.is_some();
        self.state = self.generation.state();
        // a run's first picture, or its failure, brings the tray up (results read back from the
        // artwork's folder don't)
        if self.was_running
            && (self.state.results.last().map(|r| r.id) != newest || (self.state.error.is_some() && !failed))
        {
            self.tray_up = true;
        }
        self.was_running = self.state.running.is_some();
        if self.state.made_layer.is_some() {
            // a trace landed in a control layer: the popover reads the layers again next frame
            self.control_signature.clear();
        }
        self.server = self.generation.server();
        if crate::prefs::set_generation_recent(self.generation.recent_prompts()) {
            self.settings_changed = true;
        }
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

    /// Reads the artwork's control layers again when one is made, painted on, changed, hidden or
    /// removed, and keeps their pictures; a layer's strength stays while the app runs.
    fn refresh_controls(&mut self, ctx: &egui::Context, session: &CanvasSession) {
        let layers = session.layers();
        let versions: HashMap<u64, u64> = if layers.iter().any(|l| l.control.is_some()) {
            session.layer_versions().into_iter().map(|v| (v.id, v.version)).collect()
        } else {
            HashMap::new()
        };
        let signature: Vec<(u64, String, Option<LayerControl>, bool, u64)> = layers
            .into_iter()
            .filter(|l| l.control.is_some())
            .map(|l| (l.id, l.name, l.control, l.visible, versions.get(&l.id).copied().unwrap_or(0)))
            .collect();
        if signature == self.control_signature {
            return;
        }
        self.control_signature = signature;
        self.controls = self.generation.control_layers(session);
        let ids: Vec<u64> = self.controls.iter().map(|c| c.id).collect();
        self.control_pictures.retain(|id, _| ids.contains(id));
        for control in &self.controls {
            self.control_pictures
                .entry(control.id)
                .or_insert_with(|| texture(ctx, &format!("control {}", control.id), &control.thumbnail));
        }
    }

    /// A run's progress (0..1) while one goes.
    pub fn progress(&self) -> Option<f32> {
        self.state.running.as_ref().map(|r| r.progress)
    }

    /// Whether the results tray has anything to show.
    pub fn has_results(&self) -> bool {
        self.state.running.is_some() || !self.state.results.is_empty() || self.state.error.is_some()
    }

    /// The artwork is closing: its run is cancelled and its results kept with the artwork.
    pub fn leave(&mut self, session: &CanvasSession) {
        self.generation.leave(session);
        self.state = GenerationState::default();
        self.tuck();
        self.was_running = false;
    }

    /// The artwork `session` lives in `folder`: its kept results come back, read on a thread of
    /// their own, with the tray tucked into its tab.
    pub fn set_folder(&mut self, session: &CanvasSession, folder: String) {
        self.generation.set_folder(session, folder);
        self.tuck();
    }

    /// The results tray is up: the canvas takes a press on it as one outside an open panel.
    pub fn tray_up(&self) -> bool {
        self.tray_up && self.has_results()
    }

    /// Tucks the results tray into its tab (a tap on the canvas, Esc, ⌄).
    pub fn tuck(&mut self) {
        self.tray_up = false;
        self.tray_menu = false;
        self.deleting = None;
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
        self.refresh_controls(ctx, session);
        self.selection = self.generation.selection(session);
        let area = self.selection.map(|s| (s.x, s.y, s.width, s.height));
        if area != self.selection_area {
            self.selection_area = area;
            self.mode = None;
        }
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
        let selection = if self.selection.is_some() { 70.0 } else { 0.0 };
        let controls = 22.0 + CONTROL_ROW * self.controls.len() as f32 + ADD_ROW + 10.0;
        let strip = crate::generation_tags::STRIP_HEIGHT + crate::generation_tags::STRIP_GAP;
        56.0 + 22.0
            + 84.0
            + 22.0
            + 90.0
            + strip
            + controls
            + selection
            + 54.0
            + 52.0
            + 22.0
            + 46.0
            + 24.0
            + 60.0
            + error
            + PAD
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
        // the server, as a button: its settings (address, NSFW, largest picture) behind it
        let right = body.right() - 24.0;
        let at = pos2(right - 22.0, body.top() + 36.0);
        let w = text_width(ui.painter(), &words, 13.0, Weight::Regular);
        let button = Rect::from_min_max(pos2(at.x - w - 27.0, at.y - 15.0), pos2(right, at.y + 15.0));
        let response = ui.interact(button, ui.scope_id().with("server button"), Sense::click());
        let fill = if response.hovered() { argb(FIELD) } else { argb(ROW) };
        ui.painter().rect_filled(button, 10.0, fill);
        text(ui.painter(), at, Align2::RIGHT_CENTER, &words, 13.0, Weight::Regular, argb(INK));
        text(ui.painter(), pos2(right - 9.0, at.y), Align2::RIGHT_CENTER, "›", 17.0, Weight::Regular, argb(DIM));
        ui.painter().circle_filled(pos2(at.x - w - 12.0, at.y), 4.5, argb(colour));
        if response.clicked() {
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
        let prompt_id = egui::Id::unique("generate prompt");
        let took_enter =
            crate::generation_tags::keys(ui, &self.generation, &mut self.prompt, &mut self.prompt_caret, prompt_id);
        let edit = egui::TextEdit::multiline(&mut self.prompt)
            .id(prompt_id)
            .font(Weight::Regular.font(15.0))
            .text_color(argb(INK))
            .frame(egui::Frame::NONE)
            .margin(vec2(0.0, 0.0))
            .hint_text("Describe it, or type tags…")
            .desired_width(inner.width())
            .desired_rows(3);
        // a click on the suggestion strip keeps the field focused, so the strip is still there to take it
        let on_strip = ui
            .ctx()
            .pointer_hover_pos()
            .is_some_and(|p| crate::generation_tags::strip_rect(pos2(left, y + 90.0), right - left).contains(p));
        let surrender = ui.ctx().options(|o| o.input_options.surrender_focus_on);
        if on_strip {
            ui.ctx().options_mut(|o| o.input_options.surrender_focus_on = egui::SurrenderFocusOn::Never);
        }
        let output = edit.show(&mut ui.new_child(UiBuilder::new().max_rect(inner)));
        ui.ctx().options_mut(|o| o.input_options.surrender_focus_on = surrender);
        let focused = output.response.has_focus();
        if let Some(range) = output.cursor_range {
            self.prompt_caret = crate::generation_tags::char_to_byte(&self.prompt, range.primary.index.0);
        }
        crate::generation_tags::strip(
            ui,
            &self.generation,
            &mut self.prompt,
            &mut self.prompt_caret,
            prompt_id,
            focused,
            pos2(left, y + 90.0),
            right - left,
        );
        let enter = !took_enter && focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        y += 90.0 + crate::generation_tags::STRIP_HEIGHT + crate::generation_tags::STRIP_GAP;
        // control layers: one row each, its picture, what it is, how closely to follow it in four
        // steps, a trace button and a × to stop using the layer as one; then + Line art and
        // + Scribble, each adding an empty one to draw on or trace into
        {
            label(ui, pos2(left + 2.0, y), "CONTROL");
            y += 22.0;
            let controls = self.controls.clone();
            let mut removed = None;
            let mut traced = None;
            for control in &controls {
                let row = Rect::from_min_max(pos2(left, y), pos2(right, y + CONTROL_ROW));
                ui.painter().rect_filled(row, 10.0, argb(ROW));
                let thumb = Rect::from_min_size(pos2(row.left() + 5.0, row.top() + 5.0), vec2(38.0, 38.0));
                ui.painter().rect_filled(thumb, 5.0, argb(FIELD));
                if let Some(t) = self.control_pictures.get(&control.id) {
                    let shown = fit(thumb.shrink(1.0), t.size_vec2());
                    ui.painter().image(
                        t.id(),
                        shown,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                let ink = if control.visible { argb(INK) } else { argb(DIM) };
                text(
                    ui.painter(),
                    pos2(thumb.right() + 10.0, row.top() + 16.0),
                    Align2::LEFT_CENTER,
                    &control.name,
                    14.0,
                    Weight::Medium,
                    ink,
                );
                let kind = match control.kind {
                    LayerControl::LineArt => "Line art",
                    LayerControl::Scribble => "Scribble",
                };
                let kind = if control.visible { kind.to_owned() } else { format!("{kind} · hidden") };
                text(
                    ui.painter(),
                    pos2(thumb.right() + 10.0, row.top() + 32.0),
                    Align2::LEFT_CENTER,
                    &kind,
                    11.0,
                    Weight::Regular,
                    argb(DIM),
                );
                // how closely to follow it: Loose · Balanced · Close · Strict
                let steps = Rect::from_min_max(
                    pos2(thumb.right() + 10.0, row.top() + 44.0),
                    pos2(row.right() - 38.0, row.bottom() - 6.0),
                );
                ui.painter().rect_filled(steps, 8.0, argb(FIELD));
                let cell_w = (steps.width() - 6.0) / STEPS.len() as f32;
                let step = self.control_steps.entry(control.id).or_default();
                for (i, (&value, name)) in STEPS.iter().zip(STEP_NAMES).enumerate() {
                    let cell = Rect::from_min_max(
                        pos2(steps.left() + 3.0 + i as f32 * cell_w, steps.top() + 3.0),
                        pos2(steps.left() + 3.0 + (i + 1) as f32 * cell_w - 2.0, steps.bottom() - 3.0),
                    );
                    let on = *step == value;
                    if on {
                        ui.painter().rect_filled(cell, 7.0, argb(pc::ACCENT));
                    }
                    let ink = if on { Color32::WHITE } else { argb(DIM) };
                    text(ui.painter(), cell.center(), Align2::CENTER_CENTER, name, 12.0, Weight::Medium, ink);
                    if ui.interact(cell, ui.scope_id().with(("control step", control.id, i)), Sense::click()).clicked()
                    {
                        *step = value;
                    }
                }
                // trace the picture into it; a ring while it goes
                let trace = Rect::from_center_size(pos2(row.right() - 58.0, row.top() + 19.0), vec2(30.0, 30.0));
                let idle = self.state.running.is_none();
                ui.painter().rect_filled(trace, 8.0, argb(FIELD));
                match self.state.running.as_ref().filter(|r| r.extract_layer == Some(control.id)) {
                    Some(r) => ring(ui, trace.center(), 9.0, r.progress),
                    None => {
                        let ink = if idle { argb(INK) } else { argb(DIM) };
                        look.icon(ui, &TRACE, trace.center(), 20.0, ink);
                    }
                }
                let tapped = ui.interact(trace, ui.scope_id().with(("trace", control.id)), Sense::click());
                if idle && tapped.on_hover_text("Trace the picture").clicked() {
                    traced = Some(control.id);
                }
                let remove = Rect::from_center_size(pos2(row.right() - 22.0, row.top() + 19.0), vec2(30.0, 30.0));
                text(ui.painter(), remove.center(), Align2::CENTER_CENTER, "×", 20.0, Weight::Regular, argb(DIM));
                if ui.interact(remove, ui.scope_id().with(("control off", control.id)), Sense::click()).clicked() {
                    removed = Some(control.id);
                }
                y += CONTROL_ROW;
            }
            if let Some(id) = removed {
                session.set_layer_control(id, None);
                self.controls.retain(|c| c.id != id);
                self.control_pictures.remove(&id);
                self.control_steps.remove(&id);
            }
            if let Some(id) = traced
                && let Err(e) = self.generation.trace_control(session, id)
            {
                self.state.error = Some(e.to_string());
            }
            y += 4.0;
            let half = (right - left - 8.0) / 2.0;
            for (i, (kind, words)) in
                [(LayerControl::LineArt, "Line art"), (LayerControl::Scribble, "Scribble")].into_iter().enumerate()
            {
                let x = left + i as f32 * (half + 8.0);
                let add = Rect::from_min_size(pos2(x, y), vec2(half, ADD_ROW));
                ui.painter().rect_filled(add, 10.0, argb(ROW));
                let words_w = text_width(ui.painter(), words, 14.0, Weight::Regular);
                let start = add.center().x - (words_w + 16.0) / 2.0;
                text(
                    ui.painter(),
                    pos2(start, add.center().y),
                    Align2::LEFT_CENTER,
                    "+",
                    17.0,
                    Weight::Regular,
                    argb(pc::ACCENT),
                );
                text(
                    ui.painter(),
                    pos2(start + 16.0, add.center().y),
                    Align2::LEFT_CENTER,
                    words,
                    14.0,
                    Weight::Regular,
                    argb(INK),
                );
                if ui.interact(add, ui.scope_id().with(("add control", i)), Sense::click()).clicked()
                    && session.add_control_layer(kind) == 0
                {
                    self.state.error = Some("No room for another layer".to_owned());
                }
            }
            y += ADD_ROW + 6.0;
        }
        // the selection's mode, the guessed one until the user picks another
        if let Some(selection) = self.selection {
            let chosen = self.mode.unwrap_or(selection.mode);
            label(ui, pos2(left + 2.0, y), "SELECTION");
            y += 22.0;
            let seg = Rect::from_min_max(pos2(left, y), pos2(right, y + 40.0));
            ui.painter().rect_filled(seg, 12.0, argb(ROW));
            let cell_w = (seg.width() - 6.0) / MODES.len() as f32;
            for (i, (&mode, name)) in MODES.iter().zip(MODE_NAMES).enumerate() {
                let cell = Rect::from_min_max(
                    pos2(seg.left() + 3.0 + i as f32 * cell_w, seg.top() + 3.0),
                    pos2(seg.left() + 3.0 + (i + 1) as f32 * cell_w - 2.0, seg.bottom() - 3.0),
                );
                let on = chosen == mode;
                if on {
                    ui.painter().rect_filled(cell, 9.0, argb(pc::ACCENT));
                }
                let ink = if on { Color32::WHITE } else { argb(DIM) };
                text(ui.painter(), cell.center(), Align2::CENTER_CENTER, name, 14.0, Weight::Medium, ink);
                if ui.interact(cell, ui.scope_id().with(("mode", i)), Sense::click()).clicked() {
                    self.mode = Some(mode);
                }
            }
            y += 48.0;
        }
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
        let detail = plan_text(&info);
        text(ui.painter(), pos2(left + 2.0, y), Align2::LEFT_TOP, &detail, 12.0, Weight::Regular, argb(DIM));
        y += 24.0;
        // Generate, or Cancel while a run goes
        let go = Rect::from_min_max(pos2(left, y), pos2(right, y + 48.0));
        let running = self.state.running.as_ref();
        let add_waiting = self.selection.is_some_and(|s| self.mode.unwrap_or(s.mode) == GenerationMode::Add)
            && self.prompt.trim().is_empty();
        let ready = self.server.connection == ServerConnection::Connected && !styles.is_empty() && !add_waiting;
        let words = match running {
            Some(r) if r.extracting => format!("Cancel · Tracing {}…", extract_name(r.extract_kind)),
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
        let controls = self
            .controls
            .iter()
            .filter(|c| c.visible)
            .map(|c| ControlSetting { layer: c.id, step: self.control_steps.get(&c.id).copied().unwrap_or_default() })
            .collect();
        let run = RunSettings {
            prompt: self.prompt.clone(),
            style: self.style.clone(),
            strength: self.strength,
            seed: seed.or(self.seed),
            count: self.count,
            resolution: Some(self.resolution),
            mode: self.mode,
            controls,
        };
        match self.generation.generate(session, run.clone()) {
            Ok(_) => {
                self.last = Some(run);
                if crate::prefs::set_generation_recent(self.generation.recent_prompts()) {
                    self.settings_changed = true;
                }
            }
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

    /// The results tray at the foot of the window (the results, the run going, ↻, Apply, ··· and
    /// ⌄), or, tucked, its tab. Esc tucks it when no popover is open (`popover_open`).
    pub fn results_tray(
        &mut self,
        ctx: &egui::Context,
        look: &mut Look,
        session: &CanvasSession,
        backdrops: &mut Vec<f32>,
        popover_open: bool,
    ) -> Option<Asked> {
        if let Some(crate::alert::Answer::Action) = self.clear_alert.show(ctx, look) {
            self.generation.discard_results(session);
            self.tuck();
        }
        if !self.has_results() {
            self.tuck();
            return None;
        }
        if self.tray_up && !popover_open && !self.clear_alert.is_open() && crate::ui::escape(ctx) {
            if self.tray_menu {
                self.tray_menu = false;
            } else {
                self.tuck();
            }
        }
        if !self.tray_up {
            self.tray_tab(ctx, look, backdrops);
            return None;
        }
        let screen = ctx.content_rect();
        let width = (screen.width() - 2.0 * 140.0).clamp(420.0, 900.0);
        let rect = Rect::from_min_size(
            pos2(screen.center().x - width / 2.0, screen.bottom() - 22.0 - 96.0),
            vec2(width, 96.0),
        );
        let mut asked = None;
        let mut more = None;
        let mut tucked = false;
        // a Delete showing sticks out above the bar: the area starts above it too, so the pointer
        // over it is over the bar, not the canvas (an area reaches right and down from its corner)
        let reach = if self.deleting.is_some() { 56.0 } else { 0.0 };
        let corner = pos2(rect.left(), rect.top() - reach);
        egui::Area::new(Id::unique("generate results")).fixed_pos(corner).order(Order::Foreground).show(ctx, |ui| {
            ui.allocate_rect(rect, Sense::click());
            panel::floating_frame(ui, look, "generate results frame", rect, backdrops);
            ui.painter().rect_filled(rect, 22.0, argb(BAR_FILL));
            let cy = rect.center().y;
            let mut x = rect.left() + 16.0;
            // the thumbnails, oldest first, as many as fit before the buttons; older ones behind a
            // ‹ (and newer behind a ›) when scrolled, by those or the mouse wheel
            let room = ((width - 16.0 - 330.0 - 2.0 * CHEVRON) / (THUMB + 10.0)).floor().max(1.0) as usize;
            let shown = self.state.shown;
            let total = self.state.results.len() + usize::from(self.state.running.is_some());
            let most_back = total.saturating_sub(room);
            let strip = Rect::from_min_max(
                pos2(rect.left(), rect.top()),
                pos2(rect.left() + 16.0 + 2.0 * CHEVRON + room as f32 * (THUMB + 10.0), rect.bottom()),
            );
            if ui.rect_contains_pointer(strip) {
                // whole wheel notches, not the smoothed delta, so one notch moves one result
                let wheel: f32 = ui.input(|i| {
                    i.raw
                        .events
                        .iter()
                        .filter_map(|e| match e {
                            egui::Event::MouseWheel { delta, .. } => Some(delta.y + delta.x),
                            _ => None,
                        })
                        .sum()
                });
                if wheel > 0.0 {
                    self.back += 1;
                } else if wheel < 0.0 {
                    self.back = self.back.saturating_sub(1);
                }
            }
            self.back = self.back.min(most_back);
            let skip = most_back - self.back;
            if skip > 0 {
                let cell = Rect::from_min_size(pos2(x, cy - THUMB / 2.0), vec2(CHEVRON, THUMB));
                text(ui.painter(), cell.center(), Align2::CENTER_CENTER, "‹", 26.0, Weight::Regular, argb(INK));
                if ui.interact(cell, ui.scope_id().with("older"), Sense::click()).clicked() {
                    self.back += room.max(1);
                }
            }
            x += CHEVRON;
            for r in self.state.results.iter().skip(skip).take(room) {
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
                let response = ui.interact(cell, ui.scope_id().with(("result", r.id)), Sense::click());
                // held (half a second) or right-clicked: a small Delete above it
                let held = response.is_pointer_button_down_on()
                    && ui.input(|i| i.pointer.press_start_time().is_some_and(|t| i.time - t > HOLD));
                if response.is_pointer_button_down_on() && !held {
                    // a still press makes no events: look again soon so the hold is noticed
                    ui.ctx().request_repaint_after(Duration::from_millis(50));
                }
                if held || response.secondary_clicked() || response.long_touched() {
                    self.deleting = Some(r.id);
                } else if response.clicked() && self.deleting != Some(r.id) {
                    let next = if shown == Some(r.id) { None } else { Some(r.id) };
                    self.generation.show_result(session, next);
                }
                if self.deleting == Some(r.id) {
                    let bubble = Rect::from_center_size(pos2(cell.center().x, cell.top() - 34.0), vec2(76.0, 32.0));
                    ui.allocate_rect(bubble, Sense::hover());
                    ui.painter().rect_filled(bubble, 10.0, argb(BAD));
                    text(
                        ui.painter(),
                        bubble.center(),
                        Align2::CENTER_CENTER,
                        "Delete",
                        14.0,
                        Weight::Medium,
                        Color32::WHITE,
                    );
                    let tapped = ui.interact(bubble, ui.scope_id().with(("delete", r.id)), Sense::click()).clicked();
                    if tapped {
                        self.generation.delete_result(session, r.id);
                        self.thumbnails.remove(&r.id);
                        self.deleting = None;
                    } else if !held
                        && ui.input(|i| i.pointer.any_pressed())
                        && !ui
                            .input(|i| i.pointer.interact_pos())
                            .is_some_and(|p| bubble.contains(p) || cell.contains(p))
                    {
                        self.deleting = None;
                    }
                }
                x += THUMB + 10.0;
            }
            if self.back > 0 {
                let cell = Rect::from_min_size(pos2(x, cy - THUMB / 2.0), vec2(CHEVRON, THUMB));
                text(ui.painter(), cell.center(), Align2::CENTER_CENTER, "›", 26.0, Weight::Regular, argb(INK));
                if ui.interact(cell, ui.scope_id().with("newer"), Sense::click()).clicked() {
                    self.back = self.back.saturating_sub(room.max(1));
                }
                x += CHEVRON;
            }
            if let Some(run) = self.state.running.as_ref().filter(|_| self.back == 0) {
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
                if run.extracting {
                    let words = format!("Tracing {}…", extract_name(run.extract_kind));
                    text(
                        ui.painter(),
                        pos2(cell.right() + 10.0, cy),
                        Align2::LEFT_CENTER,
                        &words,
                        12.0,
                        Weight::Regular,
                        argb(INK),
                    );
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
            // the buttons from the right: ⌄ (tuck), ··· (more), Apply, ↻ (again)
            let mut bx = rect.right() - 16.0;
            let tuck = Rect::from_center_size(pos2(bx - ROUND / 2.0, cy), vec2(ROUND, ROUND));
            tucked = round_button(ui, tuck, "tuck");
            chevron(ui.painter(), tuck.center(), false, argb(INK));
            bx = tuck.left() - 10.0;
            let dots = Rect::from_center_size(pos2(bx - ROUND / 2.0, cy), vec2(ROUND, ROUND));
            if round_button(ui, dots, "more") {
                self.tray_menu = !self.tray_menu;
            }
            for d in [-7.0, 0.0, 7.0] {
                ui.painter().circle_filled(dots.center() + vec2(d, 0.0), 2.4, argb(INK));
            }
            more = Some(dots);
            bx = dots.left() - 10.0;
            let target = shown.or(self.state.results.last().map(|r| r.id));
            if pill(ui, &mut bx, cy, "Apply", true, target.is_some())
                && let Some(id) = target
            {
                match self.generation.apply_result(session, id) {
                    Some(_) => asked = Some(Asked::Notice("Added as a layer".to_owned(), false)),
                    None => asked = Some(Asked::Notice("No room for another layer".to_owned(), true)),
                }
            }
            let again_ready = self.last.is_some() && self.state.running.is_none();
            let again = Rect::from_center_size(pos2(bx - ROUND / 2.0, cy), vec2(ROUND, ROUND));
            if round_button(ui, again, "again")
                && again_ready
                && let Some(last) = self.last.clone()
                && let Err(e) = self.generation.generate(session, last)
            {
                asked = Some(Asked::Notice(e.to_string(), true));
            }
            let ink = argb(INK).gamma_multiply(if again_ready { 1.0 } else { 0.4 });
            look.icon(ui, &AGAIN, again.center(), 22.0, ink);
            if let Some(error) = &self.state.error {
                let at = pos2(x + 4.0, cy);
                text(ui.painter(), at, Align2::LEFT_CENTER, &shorten(error, 48), 12.0, Weight::Regular, argb(WARN));
            }
        });
        if tucked {
            self.tuck();
        } else if self.tray_menu
            && let Some(dots) = more
        {
            self.tray_menu_show(ctx, rect, dots);
        }
        asked
    }

    /// The tray's ··· menu over its button `dots`: "Use the seed" of the result shown (or the
    /// newest), and "Clear all results…", which asks first. A press elsewhere closes it.
    fn tray_menu_show(&mut self, ctx: &egui::Context, tray: Rect, dots: Rect) {
        let menu = Rect::from_min_size(pos2(dots.right() - 240.0, tray.top() - 10.0 - 96.0), vec2(240.0, 96.0));
        let target = self.state.shown.or(self.state.results.last().map(|r| r.id));
        let seed = target.and_then(|id| self.state.results.iter().find(|r| r.id == id)).map(|r| r.seed);
        egui::Area::new(Id::unique("results tray menu")).fixed_pos(menu.min).order(Order::Foreground).show(ctx, |ui| {
            ui.allocate_rect(menu, Sense::click());
            ui.painter().rect_filled(menu, 14.0, argb(MENU_FILL));
            let first = Rect::from_min_size(menu.min, vec2(menu.width(), 48.0));
            let second = Rect::from_min_size(pos2(menu.left(), menu.top() + 48.0), vec2(menu.width(), 48.0));
            ui.painter().line_segment(
                [pos2(menu.left(), second.top()), pos2(menu.right(), second.top())],
                egui::Stroke::new(1.0, argb(MENU_LINE)),
            );
            let words = seed.map_or_else(|| "Use the seed".to_owned(), |s| format!("Use the seed {s}"));
            let ink = argb(INK).gamma_multiply(if seed.is_some() { 1.0 } else { 0.4 });
            text(
                ui.painter(),
                pos2(first.left() + 16.0, first.center().y),
                Align2::LEFT_CENTER,
                &words,
                15.0,
                Weight::Regular,
                ink,
            );
            if ui.interact(first, ui.scope_id().with("use the seed"), Sense::click()).clicked()
                && let Some(s) = seed
            {
                self.seed = Some(s);
                self.seed_text = s.to_string();
                self.tray_menu = false;
            }
            let at = pos2(second.left() + 16.0, second.center().y);
            text(
                ui.painter(),
                at,
                Align2::LEFT_CENTER,
                "Clear all results…",
                15.0,
                Weight::Regular,
                Color32::from_rgb(0xE8, 0x4F, 0x51),
            );
            if ui.interact(second, ui.scope_id().with("clear all results"), Sense::click()).clicked() {
                let n = self.state.results.len();
                let title = if n == 1 { "Clear the result?".to_owned() } else { format!("Clear all {n} results?") };
                self.clear_alert.open(&title, "They can't be brought back.", "Clear", None);
                self.tray_menu = false;
            }
        });
        let pressed_outside = ctx.input(|i| {
            i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !menu.contains(p) && !dots.contains(p))
        });
        if pressed_outside {
            self.tray_menu = false;
        }
    }

    /// The tray tucked away: its tab at the bottom centre, the shown (or newest) result and how
    /// many, or the run going; tapped, the tray comes up. None with no results and no run.
    fn tray_tab(&mut self, ctx: &egui::Context, look: &mut Look, backdrops: &mut Vec<f32>) {
        let running = self.state.running.as_ref();
        if self.state.results.is_empty() && running.is_none() {
            return;
        }
        let words = match running {
            Some(run) if run.extracting => format!("Tracing {}…", extract_name(run.extract_kind)),
            Some(run) => format!("Generating · {:.0}%", run.progress * 100.0),
            None => format!("Results · {}", self.state.results.len()),
        };
        let screen = ctx.content_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(Order::Foreground, Id::unique("tray tab measure")));
        let width = 8.0 + 28.0 + 10.0 + text_width(&painter, &words, 14.5, Weight::Medium) + 12.0 + 14.0 + 16.0;
        let rect = Rect::from_min_size(
            pos2(screen.center().x - width / 2.0, screen.bottom() - 22.0 - TAB_HEIGHT),
            vec2(width, TAB_HEIGHT),
        );
        let progress = running.map(|r| r.progress);
        let picture =
            self.state.shown.or(self.state.results.last().map(|r| r.id)).and_then(|id| self.thumbnails.get(&id));
        let mut open = false;
        egui::Area::new(Id::unique("results tray tab")).fixed_pos(rect.min).order(Order::Foreground).show(ctx, |ui| {
            open = ui.allocate_rect(rect, Sense::click()).clicked();
            panel::floating_frame(ui, look, "results tray tab frame", rect, backdrops);
            ui.painter().rect_filled(rect, TAB_HEIGHT / 2.0, argb(BAR_FILL));
            let cell = Rect::from_min_size(pos2(rect.left() + 8.0, rect.top() + 7.0), vec2(28.0, 28.0));
            match (progress, picture) {
                (Some(done), _) => ring(ui, cell.center(), 11.0, done),
                (None, Some(t)) => {
                    ui.painter().image(
                        t.id(),
                        fit(cell, t.size_vec2()),
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                (None, None) => {}
            }
            let at = pos2(cell.right() + 10.0, rect.center().y);
            text(ui.painter(), at, Align2::LEFT_CENTER, &words, 14.5, Weight::Medium, argb(INK));
            chevron(ui.painter(), pos2(rect.right() - 22.0, rect.center().y), true, argb(DIM));
        });
        if open {
            self.tray_up = true;
        }
    }
}

/// One of the results tray's round buttons at `rect`, its glyph drawn after; whether it was tapped.
fn round_button(ui: &Ui, rect: Rect, id: &str) -> bool {
    ui.painter().circle_filled(rect.center(), rect.width() / 2.0, argb(ROW));
    ui.interact(rect, ui.scope_id().with(("round", id)), Sense::click()).clicked()
}

/// A small chevron at `at`, pointing up or down.
fn chevron(painter: &egui::Painter, at: egui::Pos2, up: bool, colour: Color32) {
    let dy = if up { -3.5 } else { 3.5 };
    let stroke = egui::Stroke::new(2.2, colour);
    painter.line_segment([at + vec2(-7.0, -dy), at + vec2(0.0, dy)], stroke);
    painter.line_segment([at + vec2(0.0, dy), at + vec2(7.0, -dy)], stroke);
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

/// "line art" or "scribble", for what an extraction is making.
fn extract_name(kind: Option<LayerControl>) -> &'static str {
    match kind {
        Some(LayerControl::Scribble) => "scribble",
        _ => "line art",
    }
}

/// The small text under the resolution row: what will be made, and how.
fn plan_text(size: &SizeInfo) -> String {
    let mut words = format!("{} × {}", size.width, size.height);
    if size.passes > 1 {
        words.push_str(&format!(" · {} passes", size.passes));
    } else if size.scaled_up {
        words.push_str(" → scaled up");
    } else {
        words.push_str(" · 1 pass");
    }
    if size.capped {
        words.push_str(&format!(" · capped at {} MP", megapixels(size.largest_megapixels)));
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
    use super::*;
    use procraft_core::{ResultInfo, ThumbnailImage};

    /// One frame of the popover, under `button`, with `events`.
    fn frame(ctx: &egui::Context, ui: &mut GenerationUi, look: &mut Look, s: &CanvasSession, events: Vec<egui::Event>) {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1400.0, 1000.0))),
            events,
            ..Default::default()
        };
        input.time = Some(ctx.input(|i| i.time) + 0.05);
        let mut out = ctx.run_ui(input, |root| {
            let ctx = root.ctx().clone();
            let mut backdrops = Vec::new();
            ui.popover(&ctx, look, s, Rect::from_min_size(pos2(600.0, 10.0), vec2(40.0, 40.0)), &mut backdrops);
        });
        out.textures_delta.clear();
    }

    #[test]
    fn a_click_on_a_suggestion_completes_the_tag() {
        let ctx = egui::Context::default();
        let mut look = Look::default();
        let session = CanvasSession::new(400, 300);
        let settings = GenerationSettings { address: "127.0.0.1:1".into(), nsfw: false, largest_megapixels: None };
        let mut ui = GenerationUi::new(settings, GenerationResolution::Normal);
        ui.server.connection = ServerConnection::Connected;
        ui.prompt = "light".into();
        ui.prompt_caret = 5;
        frame(&ctx, &mut ui, &mut look, &session, vec![]);
        let id = egui::Id::unique("generate prompt");
        ctx.memory_mut(|m| m.request_focus(id));
        frame(&ctx, &mut ui, &mut look, &session, vec![]);
        // the first chip, under the field (its frame is the text's rect grown by 12 × 10)
        let field = ctx.read_response(id).expect("the prompt field").rect;
        let p = pos2(field.left() + 18.0, field.top() + 97.0);
        let button = |pressed| egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(&ctx, &mut ui, &mut look, &session, vec![egui::Event::PointerMoved(p)]);
        frame(&ctx, &mut ui, &mut look, &session, vec![button(true)]);
        frame(&ctx, &mut ui, &mut look, &session, vec![button(false)]);
        assert_ne!(ui.prompt, "light", "the click completed the tag");
        assert!(ctx.memory(|m| m.has_focus(id)), "the field is still being typed in");
    }

    /// One frame of the results tray alone, with `events`.
    fn tray_frame(
        ctx: &egui::Context,
        ui: &mut GenerationUi,
        look: &mut Look,
        s: &CanvasSession,
        events: Vec<egui::Event>,
    ) {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1400.0, 1000.0))),
            events,
            ..Default::default()
        };
        input.time = Some(ctx.input(|i| i.time) + 0.05);
        let mut out = ctx.run_ui(input, |root| {
            let ctx = root.ctx().clone();
            let mut backdrops = Vec::new();
            ui.results_tray(&ctx, look, s, &mut backdrops, false);
        });
        out.textures_delta.clear();
    }

    /// A click at `p` (moved there first).
    fn click(p: egui::Pos2) -> Vec<Vec<egui::Event>> {
        let button = |pressed| egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        vec![vec![egui::Event::PointerMoved(p)], vec![button(true)], vec![button(false)], vec![]]
    }

    #[test]
    fn the_tray_tucks_into_its_tab_and_the_tab_brings_it_back() {
        let ctx = egui::Context::default();
        let mut look = Look::default();
        let session = CanvasSession::new(400, 300);
        let settings = GenerationSettings { address: String::new(), nsfw: false, largest_megapixels: None };
        let mut ui = GenerationUi::new(settings, GenerationResolution::Normal);
        let thumbnail = ThumbnailImage { width: 2, height: 2, rgba: vec![255; 16] };
        let result =
            |id| ResultInfo { id, seed: 7, thumbnail: thumbnail.clone(), scaled_from_width: 0, scaled_from_height: 0 };
        ui.state.results = vec![result(1), result(2)];
        ui.tray_up = true;
        tray_frame(&ctx, &mut ui, &mut look, &session, vec![]);
        assert!(ui.tray_up());
        // ⌄ at the tray's right end: 16 in from the edge of a 900-wide tray centred in 1400
        let tray_right = 700.0 + 450.0;
        for events in click(pos2(tray_right - 16.0 - ROUND / 2.0, 1000.0 - 22.0 - 48.0)) {
            tray_frame(&ctx, &mut ui, &mut look, &session, events);
        }
        assert!(!ui.tray_up(), "⌄ tucked it");
        // the tab, at the bottom centre
        for events in click(pos2(700.0, 1000.0 - 22.0 - TAB_HEIGHT / 2.0)) {
            tray_frame(&ctx, &mut ui, &mut look, &session, events);
        }
        assert!(ui.tray_up(), "the tab brought it back");
        // Esc tucks it again, with no popover open
        let esc = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        tray_frame(&ctx, &mut ui, &mut look, &session, vec![esc]);
        assert!(!ui.tray_up(), "Esc tucked it");
        assert_eq!(ui.state.results.len(), 2, "nothing was lost");
    }
}
