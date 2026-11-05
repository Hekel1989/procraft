//! Image generation on the canvas (with the `generation` feature): a ComfyUI server makes pictures
//! from a prompt, and the picture there is, through `procraft-diffusion`; here they're taken from
//! the canvas and given back to it.
//!
//! One [`Generation`] serves the app: its server, the styles there, and the open artwork's run and
//! results. Its methods take the artwork's [`CanvasSession`]; when another artwork is used, the
//! last one's run is cancelled and its results not applied are dropped. Results are shown over the
//! canvas (the renderer's overlay, never part of the artwork) until one is applied, which adds it
//! as a layer at the top of the stack, the way it was shown, as one undo step. The app polls
//! [`Generation::state`] and [`Generation::server`], as it polls an export's progress.
//!
//! The model sees sRGB: the picture is converted from the canvas's profile on the way out and
//! the results back on the way in.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use procraft_diffusion::plan::context_box;
use procraft_diffusion::{
    Address, Connection, Feature, Picture, Request, Resolution as PlanResolution, RunId, Selection as RequestSelection,
    SelectionMode as RequestMode, Worker, largest_for_vram, plan_for,
};
use procraft_engine::cms::{Lut, builtin};
use procraft_engine::document::{Document, LayerId};
use procraft_engine::geom::IRect;
use procraft_engine::profile::Profile;
use procraft_engine::selection::Mask;

use crate::{CanvasSession, ThumbnailImage};

/// Results and previews are listed at most this big.
const THUMBNAIL_SIDE: u32 = 160;
/// The most pictures one run makes.
const MOST_PICTURES: u32 = 4;
/// A generated layer's name keeps this much of the prompt.
const NAME_PROMPT: usize = 32;
/// Results made this much smaller than the area are noted as scaled up.
const SCALED_UP: f64 = 1.5;

/// The app-wide settings: the server's address as typed (empty: none), whether NSFW is allowed,
/// and the largest picture in megapixels (none: from the server's GPU memory).
#[derive(Clone, Debug, Default, PartialEq, uniffi::Record)]
pub struct GenerationSettings {
    pub address: String,
    pub nsfw: bool,
    #[uniffi(default = None)]
    pub largest_megapixels: Option<f32>,
}

/// How large a picture to make before scaling it to the area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Enum)]
pub enum GenerationResolution {
    /// The model's own size (about 1 MP), scaled to the area after.
    Draft,
    /// Half the area's size each way, but never fewer pixels than the model's own size.
    Normal,
    /// The area's own size.
    #[default]
    Quality,
}

impl From<GenerationResolution> for PlanResolution {
    fn from(resolution: GenerationResolution) -> PlanResolution {
        match resolution {
            GenerationResolution::Draft => PlanResolution::Draft,
            GenerationResolution::Normal => PlanResolution::Normal,
            GenerationResolution::Quality => PlanResolution::Quality,
        }
    }
}

/// How a selection is generated into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum GenerationMode {
    /// Paint the area to fit its surroundings, guided by the prompt.
    Fill,
    /// Start from a neutral fill, so the prompt shapes a new thing in the area.
    Add,
    /// Take what is there away.
    Remove,
    /// Fill transparent edges in from the picture's borders.
    Expand,
}

impl From<GenerationMode> for RequestMode {
    fn from(mode: GenerationMode) -> RequestMode {
        match mode {
            GenerationMode::Fill => RequestMode::Fill,
            GenerationMode::Add => RequestMode::Add,
            GenerationMode::Remove => RequestMode::Remove,
            GenerationMode::Expand => RequestMode::Expand,
        }
    }
}

impl From<RequestMode> for GenerationMode {
    fn from(mode: RequestMode) -> GenerationMode {
        match mode {
            RequestMode::Fill => GenerationMode::Fill,
            RequestMode::Add => GenerationMode::Add,
            RequestMode::Remove => GenerationMode::Remove,
            RequestMode::Expand => GenerationMode::Expand,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ServerConnection {
    /// No address set.
    NotSet,
    Connecting,
    Connected,
    /// The last try failed ([`ServerStatus::problem`] says why).
    Failed,
}

/// What generation can do, as the server is checked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum GenerationFeature {
    Generate,
    Refine,
    Selections,
    ControlLayers,
}

impl From<Feature> for GenerationFeature {
    fn from(f: Feature) -> GenerationFeature {
        match f {
            Feature::Generate => GenerationFeature::Generate,
            Feature::Refine => GenerationFeature::Refine,
            Feature::Selections => GenerationFeature::Selections,
            Feature::ControlLayers => GenerationFeature::ControlLayers,
        }
    }
}

/// Whether a feature works with the server, and if not what it lacks ("anima-lllite-inpainting-
/// v2.safetensors in models/controlnet").
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct FeatureStatus {
    pub feature: GenerationFeature,
    pub label: String,
    pub ready: bool,
    pub missing: Vec<String>,
}

/// The server as last found.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ServerStatus {
    pub connection: ServerConnection,
    /// The address in use, as `host:port`; empty when none.
    pub address: String,
    /// Why it failed, when it did.
    pub problem: Option<String>,
    /// ComfyUI's version, once connected.
    pub version: String,
    /// The GPU's name and memory, once connected.
    pub gpu: String,
    pub features: Vec<FeatureStatus>,
    /// How many of the model family's checkpoints it has.
    pub checkpoints: u32,
    /// The model's licence line, once a checkpoint is found.
    pub licence: String,
    /// The GPU's memory in bytes, once connected.
    #[uniffi(default = 0)]
    pub vram_bytes: u64,
    /// The largest picture in force, in megapixels: the setting, or the GPU's default.
    #[uniffi(default = 0.0)]
    pub largest_megapixels: f32,
}

/// A style the server can run.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct StyleInfo {
    pub id: String,
    pub name: String,
    pub steps: u32,
    pub cfg: f32,
    /// Made from a checkpoint found on the server rather than built in.
    pub found: bool,
}

/// What to run: the prompt, a style's id, the strength (1 makes a new picture, lower refines the
/// one there is), the seed (none: a random one), how many pictures (1 to 4) and how large to make
/// them (none: Quality).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct RunSettings {
    pub prompt: String,
    pub style: String,
    pub strength: f32,
    pub seed: Option<u64>,
    pub count: u32,
    #[uniffi(default = None)]
    pub resolution: Option<GenerationResolution>,
    /// The selection's mode; none guesses Fill or Expand from what is selected.
    #[uniffi(default = None)]
    pub mode: Option<GenerationMode>,
}

/// The run going now.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct RunInfo {
    pub id: u64,
    /// How many pictures it makes, and how many it has made.
    pub count: u32,
    pub done: u32,
    /// 0 to 1 over all its pictures.
    pub progress: f32,
    /// The latest sampling preview, small, premultiplied.
    pub preview: Option<ThumbnailImage>,
    /// How many previews have come: a new number, a new preview.
    pub previews: u32,
}

/// A picture made and kept to be shown or applied.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ResultInfo {
    pub id: u64,
    pub seed: u64,
    /// Small, premultiplied.
    pub thumbnail: ThumbnailImage,
    /// The size it was made at, when it's scaled up by more than 1.5× to fill the area; 0 when it
    /// isn't.
    #[uniffi(default = 0)]
    pub scaled_from_width: u32,
    #[uniffi(default = 0)]
    pub scaled_from_height: u32,
}

/// What a resolution makes for the open artwork: the target size, the passes, and the size
/// results are generated at before the final scale to the area.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SizeInfo {
    /// The target size after the largest-picture cap.
    pub width: u32,
    pub height: u32,
    pub passes: u32,
    /// The target was brought down to the largest picture.
    pub capped: bool,
    /// Results are scaled up by more than 1.5× to fill the area.
    pub scaled_up: bool,
    /// The size actually generated (the last pass).
    pub from_width: u32,
    pub from_height: u32,
}

/// The selection a run would use: its context box on the canvas and the mode guessed for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SelectionInfo {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub mode: GenerationMode,
}

/// How generation stands for the open artwork.
#[derive(Clone, Debug, Default, PartialEq, uniffi::Record)]
pub struct GenerationState {
    pub running: Option<RunInfo>,
    /// Oldest first.
    pub results: Vec<ResultInfo>,
    /// The result shown over the canvas.
    pub shown: Option<u64>,
    /// Why the last run stopped short, if it failed.
    pub error: Option<String>,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum GenerationError {
    /// The server or its models can't do what was asked.
    #[error("{reason}")]
    NotReady { reason: String },
    /// The canvas couldn't be read.
    #[error("{reason}")]
    Canvas { reason: String },
}

/// A result kept: where it goes on the canvas and its pixels there (premultiplied RGBA8, the
/// canvas's colours, the area's size).
struct Kept {
    id: u64,
    seed: u64,
    /// The layer's name if it's applied.
    name: String,
    area: IRect,
    pixels: Vec<u8>,
    thumbnail: ThumbnailImage,
    /// The size it was made at when scaled up to the area; 0 × 0 when it wasn't.
    scaled_from: (u32, u32),
}

/// The run going, and where its pictures go.
struct Running {
    id: RunId,
    area: IRect,
    /// The selection's mask at the area (one byte per pixel), multiplied into results' alpha.
    mask: Option<Vec<u8>>,
    /// The name its pictures' layers get.
    name: String,
    /// Its pictures taken into [`Artwork::results`] so far, and previews seen.
    taken: usize,
    previews: u32,
    preview: Option<ThumbnailImage>,
}

/// A guessed selection mode, kept until the artwork changes.
struct Guess {
    revision: u64,
    area: [u32; 4],
    mode: GenerationMode,
}

/// The open artwork's part: which session it is, its run, its results.
#[derive(Default)]
struct Artwork {
    /// The session's identity (its shared state's address); 0 before any.
    session: usize,
    running: Option<Running>,
    results: Vec<Kept>,
    next: u64,
    shown: Option<u64>,
    error: Option<String>,
    /// sRGB to the canvas's colours, for a canvas in another profile.
    from_srgb: Option<Lut>,
    /// How many strokes the session had when the result was shown, and the counter itself.
    shown_at: u64,
    strokes: Option<Arc<AtomicU64>>,
    selection_guess: Option<Guess>,
}

/// Image generation for the app: create one, give it the settings, and use it with the open
/// artwork's session.
#[derive(uniffi::Object)]
pub struct Generation {
    worker: Worker,
    settings: Mutex<GenerationSettings>,
    artwork: Mutex<Artwork>,
}

/// `text` as a server address (`host:port`, port 8188 if none given); none if it isn't one.
#[uniffi::export]
pub fn generation_address(text: String) -> Option<String> {
    Address::parse(&text).map(|a| a.to_string())
}

#[uniffi::export]
impl Generation {
    #[uniffi::constructor]
    pub fn new(settings: GenerationSettings) -> Arc<Generation> {
        let generation =
            Generation { worker: Worker::new(), settings: Mutex::new(settings.clone()), artwork: Mutex::default() };
        generation.worker.set_address(Address::parse(&settings.address));
        Arc::new(generation)
    }

    /// Uses `settings` from now on; a new address connects to it.
    pub fn set_settings(&self, settings: GenerationSettings) {
        self.worker.set_address(Address::parse(&settings.address));
        *self.settings.lock() = settings;
    }

    /// Connects again (after a failure, or to find models added since).
    pub fn connect(&self) {
        self.worker.connect();
    }

    pub fn server(&self) -> ServerStatus {
        let server = self.worker.server();
        let family = procraft_diffusion::Family::Anima;
        let readiness = server.readiness.iter().find(|(f, _)| *f == family).map(|(_, r)| r);
        let features = Feature::ALL
            .iter()
            .map(|&f| {
                let missing: Vec<String> = match readiness {
                    Some(r) => r.missing(f).iter().map(ToString::to_string).collect(),
                    None => Vec::new(),
                };
                FeatureStatus {
                    feature: f.into(),
                    label: f.label().to_owned(),
                    ready: readiness.is_some() && missing.is_empty(),
                    missing,
                }
            })
            .collect();
        let checkpoints =
            server.models.as_ref().map_or(0, |m| m.diffusion_models.iter().filter(|f| family.recognises(f)).count());
        let (connection, problem) = match &server.connection {
            Connection::NotSet => (ServerConnection::NotSet, None),
            Connection::Connecting => (ServerConnection::Connecting, None),
            Connection::Connected => (ServerConnection::Connected, None),
            Connection::Failed(e) => (ServerConnection::Failed, Some(e.to_string())),
        };
        let vram_bytes = server.stats.as_ref().and_then(|s| s.gpu.as_ref()).map_or(0, |(_, bytes)| *bytes);
        ServerStatus {
            connection,
            address: server.address.as_ref().map(ToString::to_string).unwrap_or_default(),
            problem,
            version: server.stats.as_ref().map(|s| s.comfyui_version.clone()).unwrap_or_default(),
            gpu: server.stats.as_ref().and_then(|s| s.gpu.as_ref()).map(|(name, _)| name.clone()).unwrap_or_default(),
            features,
            checkpoints: u32::try_from(checkpoints).unwrap_or(u32::MAX),
            licence: if checkpoints > 0 { family.licence().to_owned() } else { String::new() },
            vram_bytes,
            largest_megapixels: self.largest_megapixels(&server),
        }
    }

    /// The selection a run would use, if there is one: its context box and guessed mode.
    pub fn selection(&self, session: &CanvasSession) -> Option<SelectionInfo> {
        let selected = current_selection(session)?;
        let [x, y, width, height] = context_box(selected.bounds, selected.canvas);
        let mut artwork = self.artwork_of(session);
        let area = [x, y, width, height];
        if let Some(guess) =
            artwork.selection_guess.as_ref().filter(|g| g.revision == selected.revision && g.area == area)
        {
            return Some(SelectionInfo { x, y, width, height, mode: guess.mode });
        }
        let mode = session
            .picture_for_generation(true)
            .ok()
            .and_then(|picture| picture.picture)
            .map_or(GenerationMode::Fill, |picture| guess_mode(&selected.mask, &picture, selected.canvas));
        let revision = session.shared.lock().revision;
        artwork.selection_guess = Some(Guess { revision, area, mode });
        Some(SelectionInfo { x, y, width, height, mode })
    }

    /// What `resolution` makes of `session`'s area (the selection's context box when there is a
    /// selection): its target, passes and generated size.
    pub fn size_for(&self, session: &CanvasSession, resolution: GenerationResolution) -> SizeInfo {
        let server = self.worker.server();
        let selection = selection_bounds(session);
        let (width, height) = match selection {
            Some((bounds, canvas)) => {
                let [_, _, width, height] = context_box(bounds, canvas);
                (width, height)
            }
            None => {
                let state = session.shared.lock_finished();
                (state.doc.width(), state.doc.height())
            }
        };
        // A selection is one pass at the target (see the worker): its second pass would have to
        // repeat the whole inpaint structure.
        let plan = plan_for(
            width,
            height,
            procraft_diffusion::Family::Anima.native_pixels(),
            resolution.into(),
            self.largest_megapixels(&server),
            selection.is_some(),
        );
        SizeInfo {
            width: plan.target.width,
            height: plan.target.height,
            passes: plan.passes(),
            capped: plan.capped,
            scaled_up: plan.stretched(width, height, SCALED_UP),
            from_width: plan.size.width,
            from_height: plan.size.height,
        }
    }

    /// The styles the server can run: the built-in ones first.
    pub fn styles(&self) -> Vec<StyleInfo> {
        self.worker
            .server()
            .styles()
            .into_iter()
            .map(|s| StyleInfo { id: s.id, name: s.name, steps: s.steps, cfg: s.cfg, found: s.found })
            .collect()
    }

    /// Starts a run on `session`'s artwork: from the picture as it is when strength is below 1, or
    /// inside the selection when there is one. Its pictures come into
    /// [`state`](Self::state)'s results as they're made.
    pub fn generate(&self, session: &CanvasSession, run: RunSettings) -> Result<u64, GenerationError> {
        let server = self.worker.server();
        if server.connection != Connection::Connected {
            return Err(GenerationError::NotReady { reason: "not connected to a server".to_owned() });
        }
        let styles = server.styles();
        let style = styles
            .iter()
            .find(|s| s.id == run.style)
            .or_else(|| styles.first())
            .cloned()
            .ok_or_else(|| GenerationError::NotReady { reason: "the server has no Anima model".to_owned() })?;
        let selected = current_selection(session);
        let refine = run.strength < 1.0;
        let feature = if selected.is_some() {
            Feature::Selections
        } else if refine {
            Feature::Refine
        } else {
            Feature::Generate
        };
        if !server.ready(style.family, feature) {
            return Err(GenerationError::NotReady { reason: format!("the server isn't ready to {}", feature.label()) });
        }
        let mut artwork = self.artwork_of(session);
        if artwork.running.is_some() {
            return Err(GenerationError::NotReady { reason: "a run is going".to_owned() });
        }
        let picture = session.picture_for_generation(refine || selected.is_some())?;
        artwork.from_srgb = picture.from_srgb;
        let mut area = IRect::from_size(picture.width, picture.height);
        let mut size = (picture.width, picture.height);
        let mut request_picture = picture.picture;
        let mut request_selection = None;
        let mut mask = None;
        if let Some(selected) = selected {
            let [x, y, width, height] = context_box(selected.bounds, selected.canvas);
            let Some(full) = request_picture.as_ref() else {
                return Err(GenerationError::Canvas { reason: "the canvas isn't drawn yet".to_owned() });
            };
            let mode = run.mode.unwrap_or_else(|| guess_mode(&selected.mask, full, selected.canvas));
            if mode == GenerationMode::Add && run.prompt.trim().is_empty() {
                return Err(GenerationError::NotReady { reason: "Add needs a prompt".to_owned() });
            }
            area = IRect::new(
                i32::try_from(x).unwrap_or(i32::MAX),
                i32::try_from(y).unwrap_or(i32::MAX),
                i32::try_from(x.saturating_add(width)).unwrap_or(i32::MAX),
                i32::try_from(y.saturating_add(height)).unwrap_or(i32::MAX),
            );
            size = (width, height);
            let cropped =
                full.cropped(x, y, width, height).map_err(|e| GenerationError::Canvas { reason: e.to_string() })?;
            request_picture = Some(cropped);
            let part = crate::layer_ops::mask_part(&selected.mask, area);
            request_selection = Some(RequestSelection { mask: part.clone(), mode: mode.into() });
            mask = Some(part);
        }
        let request = Request {
            prompt: run.prompt,
            style,
            strength: run.strength.clamp(0.0, 1.0),
            seed: run.seed.unwrap_or_else(random_seed),
            count: run.count.clamp(1, MOST_PICTURES),
            picture: request_picture,
            selection: request_selection,
            size,
            resolution: run.resolution.unwrap_or_default().into(),
            largest: Some(self.largest_megapixels(&server)),
            nsfw: self.settings.lock().nsfw,
        };
        let name = layer_name(&request.prompt);
        let id = self.worker.submit(request);
        artwork.running = Some(Running { id, area, mask, name, taken: 0, previews: 0, preview: None });
        artwork.error = None;
        Ok(id)
    }

    /// Stops the run going (the pictures it made are kept).
    pub fn cancel(&self) {
        if let Some(running) = &self.artwork.lock().running {
            self.worker.cancel(running.id);
        }
    }

    /// How things stand: the run going, the results, the one shown. Takes in what the run made
    /// since last asked.
    pub fn state(&self) -> GenerationState {
        let mut guard = self.artwork.lock();
        let artwork = &mut *guard;
        let mut running = None;
        if let Some(r) = &mut artwork.running
            && let Some(run) = self.worker.run(r.id)
        {
            for result in run.results.iter().skip(r.taken) {
                artwork.next += 1;
                let kept = keep(artwork.next, result, r, artwork.from_srgb.as_ref());
                artwork.results.extend(kept);
            }
            r.taken = run.results.len();
            if run.previews != r.previews {
                r.previews = run.previews;
                r.preview = run.preview.as_ref().and_then(thumbnail);
            }
            if run.finished {
                artwork.error = run.error.filter(|e| *e != procraft_diffusion::Error::Cancelled).map(|e| e.to_string());
                self.worker.forget(r.id);
            } else {
                running = Some(RunInfo {
                    id: r.id,
                    count: run.count,
                    done: u32::try_from(run.results.len()).unwrap_or(0),
                    progress: run.progress,
                    preview: r.preview.clone(),
                    previews: r.previews,
                });
            }
        }
        if running.is_none() {
            artwork.running = None;
        }
        // a stroke starting took the shown preview off the canvas (the bar's results stay)
        if let Some(strokes) = &artwork.strokes
            && artwork.shown.is_some()
            && artwork.shown_at != strokes.load(Ordering::Relaxed)
        {
            artwork.shown = None;
        }
        GenerationState {
            running,
            results: artwork
                .results
                .iter()
                .map(|k| ResultInfo {
                    id: k.id,
                    seed: k.seed,
                    thumbnail: k.thumbnail.clone(),
                    scaled_from_width: k.scaled_from.0,
                    scaled_from_height: k.scaled_from.1,
                })
                .collect(),
            shown: artwork.shown,
            error: artwork.error.clone(),
        }
    }

    /// Shows result `result` over `session`'s canvas, or none.
    pub fn show_result(&self, session: &CanvasSession, result: Option<u64>) {
        let mut artwork = self.artwork_of(session);
        let kept = result.and_then(|id| artwork.results.iter().find(|k| k.id == id));
        session.set_overlay(kept.map(|k| (k.area, k.pixels.as_slice())));
        artwork.shown = kept.map(|k| k.id);
        artwork.shown_at = artwork.strokes.as_ref().map_or(0, |strokes| strokes.load(Ordering::Relaxed));
    }

    /// Adds result `result` to `session`'s artwork as a new layer at the top of the stack (where
    /// it showed), named after its prompt, as one undo step; returns the layer's id.
    pub fn apply_result(&self, session: &CanvasSession, result: u64) -> Option<u64> {
        let mut artwork = self.artwork_of(session);
        let kept = artwork.results.iter().find(|k| k.id == result)?;
        session.set_overlay(None);
        let layer = session.add_generated(kept.area, &kept.pixels, &kept.name);
        artwork.shown = None;
        layer
    }

    /// Drops the results (and the one shown); a run going carries on.
    pub fn discard_results(&self, session: &CanvasSession) {
        let mut artwork = self.artwork_of(session);
        session.set_overlay(None);
        artwork.results.clear();
        artwork.shown = None;
        artwork.error = None;
    }

    /// The artwork is closing: its run is cancelled and its results dropped.
    pub fn leave(&self, session: &CanvasSession) {
        let mut artwork = self.artwork.lock();
        if artwork.session == session_key(session) {
            session.set_overlay(None);
            self.forget(&mut artwork);
        }
    }
}

impl Generation {
    /// The largest picture in force: the setting, else the server's GPU memory.
    fn largest_megapixels(&self, server: &procraft_diffusion::ServerState) -> f32 {
        let setting = self.settings.lock().largest_megapixels;
        setting.filter(|mp| mp.is_finite() && *mp > 0.0).unwrap_or_else(|| {
            let vram = server.stats.as_ref().and_then(|s| s.gpu.as_ref()).map_or(0, |(_, bytes)| *bytes);
            largest_for_vram(vram)
        })
    }

    /// The artwork's part for `session`: a fresh one if it was another session's.
    fn artwork_of(&self, session: &CanvasSession) -> parking_lot::MutexGuard<'_, Artwork> {
        let mut artwork = self.artwork.lock();
        let key = session_key(session);
        if artwork.session != key {
            self.forget(&mut artwork);
            artwork.session = key;
            artwork.strokes = Some(Arc::clone(&session.shared.preview_strokes));
        }
        artwork
    }

    fn forget(&self, artwork: &mut Artwork) {
        if let Some(running) = artwork.running.take() {
            self.worker.forget(running.id);
        }
        let next = artwork.next;
        *artwork = Artwork { next, ..Artwork::default() };
    }
}

fn session_key(session: &CanvasSession) -> usize {
    Arc::as_ptr(&session.shared) as usize
}

/// The selection on `session`, if anything is selected.
struct Selected {
    mask: Mask,
    bounds: [u32; 4],
    canvas: (u32, u32),
    revision: u64,
}

fn current_selection(session: &CanvasSession) -> Option<Selected> {
    let state = session.shared.lock();
    let mask = state.selection.as_ref()?;
    let bounds = mask.bounds()?;
    Some(Selected {
        mask: mask.clone(),
        bounds,
        canvas: (state.doc.width(), state.doc.height()),
        revision: state.revision,
    })
}

/// The selection's bounds and the canvas size, without copying the mask.
fn selection_bounds(session: &CanvasSession) -> Option<([u32; 4], (u32, u32))> {
    let state = session.shared.lock();
    let bounds = state.selection.as_ref()?.bounds()?;
    Some((bounds, (state.doc.width(), state.doc.height())))
}

/// Whether a selection is mostly transparent pixels at the canvas edge (then Expand), else Fill.
fn guess_mode(mask: &Mask, picture: &Picture, canvas: (u32, u32)) -> GenerationMode {
    let Some([x, y, width, height]) = mask.bounds() else { return GenerationMode::Fill };
    let pixels = picture.rgba.as_chunks::<4>().0;
    let mut selected = 0u64;
    let mut clear = 0u64;
    for (i, &value) in mask.data.iter().enumerate() {
        if value == 0 {
            continue;
        }
        selected += 1;
        if pixels.get(i).is_some_and(|px| px[3] < 8) {
            clear += 1;
        }
    }
    let edge = x == 0 || y == 0 || x.saturating_add(width) >= canvas.0 || y.saturating_add(height) >= canvas.1;
    if edge && clear * 2 > selected { GenerationMode::Expand } else { GenerationMode::Fill }
}

fn random_seed() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64 % 1_000_000_000_000).unwrap_or(0)
}

/// "Generated · " and the prompt's first words.
fn layer_name(prompt: &str) -> String {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return "Generated".to_owned();
    }
    let mut cut: String = prompt.chars().take(NAME_PROMPT).collect();
    if cut.len() < prompt.len() {
        cut.push('…');
    }
    format!("Generated · {cut}")
}

/// A result as kept: in the canvas's colours, premultiplied, with its thumbnail.
fn keep(id: u64, result: &procraft_diffusion::RunResult, run: &Running, from_srgb: Option<&Lut>) -> Option<Kept> {
    let area = run.area;
    let from = result.generated;
    let thumbnail = thumbnail(&result.picture)?;
    let mut pixels = result.picture.rgba.clone();
    if let Some(lut) = from_srgb {
        lut.apply_rgba(&mut pixels, false);
    }
    if let Some(mask) = &run.mask {
        for (px, &value) in pixels.as_chunks_mut::<4>().0.iter_mut().zip(mask) {
            let alpha = u16::from(px[3]);
            px[3] = ((alpha * u16::from(value) + 127) / 255) as u8;
        }
    }
    premultiply(&mut pixels);
    let scaled_from = if area.width() as f64 / f64::from(from.0.max(1)) > SCALED_UP
        || area.height() as f64 / f64::from(from.1.max(1)) > SCALED_UP
    {
        from
    } else {
        (0, 0)
    };
    (pixels.len() == area.width() as usize * area.height() as usize * 4).then_some(Kept {
        id,
        seed: result.seed,
        name: run.name.clone(),
        area,
        pixels,
        thumbnail,
        scaled_from,
    })
}

fn thumbnail(picture: &Picture) -> Option<ThumbnailImage> {
    let small = picture.thumbnail(THUMBNAIL_SIDE).ok()?;
    let mut rgba = small.rgba;
    premultiply(&mut rgba);
    Some(ThumbnailImage { width: small.width, height: small.height, rgba })
}

fn premultiply(rgba: &mut [u8]) {
    for px in rgba.as_chunks_mut::<4>().0 {
        let a = u16::from(px[3]);
        for c in px.iter_mut().take(3) {
            *c = ((u16::from(*c) * a + 127) / 255) as u8;
        }
    }
}

fn unpremultiply(rgba: &mut [u8]) {
    for px in rgba.as_chunks_mut::<4>().0 {
        let a = u16::from(px[3]);
        if a > 0 && a < 255 {
            for c in px.iter_mut().take(3) {
                *c = ((u16::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

/// The canvas as the model sees it: its size, the picture in sRGB (when it's wanted), and the
/// table taking results back to the canvas's colours.
struct ForGeneration {
    width: u32,
    height: u32,
    picture: Option<Picture>,
    from_srgb: Option<Lut>,
}

/// Tables between the canvas's colours and sRGB, both ways; none for an sRGB canvas.
fn srgb_tables(doc: &Document) -> Option<(Lut, Lut)> {
    if doc.profile == Profile::Srgb && doc.icc.is_none() {
        return None;
    }
    let (canvas, srgb) = (doc.rgb_profile(), builtin(Profile::Srgb));
    Some((Lut::build(&canvas, &srgb, None, 33), Lut::build(&srgb, &canvas, None, 33)))
}

impl CanvasSession {
    /// The canvas for a run: its size, and the finished picture (straight RGBA, sRGB) if `picture`.
    fn picture_for_generation(&self, picture: bool) -> Result<ForGeneration, GenerationError> {
        let mut guard = self.shared.lock_finished();
        let st = &mut *guard;
        let (width, height) = (st.doc.width(), st.doc.height());
        let tables = srgb_tables(&st.doc);
        let (to_srgb, from_srgb) = tables.map_or((None, None), |(to, from)| (Some(to), Some(from)));
        if !picture {
            return Ok(ForGeneration { width, height, picture: None, from_srgb });
        }
        st.finish_stroke();
        st.settle();
        st.flush();
        let unreadable = |reason: String| GenerationError::Canvas { reason };
        let (Some(canvas), Some(gpu)) = (st.canvas.as_mut(), st.gpu.as_ref()) else {
            return Err(unreadable("the canvas isn't drawn yet".to_owned()));
        };
        canvas.render_finished(gpu, &mut st.doc);
        let mut rgba = canvas.read_composite(gpu).map_err(|e| unreadable(e.to_string()))?;
        if let Some(lut) = &to_srgb {
            lut.apply_rgba(&mut rgba, true);
        }
        unpremultiply(&mut rgba);
        let picture = Picture::new(width, height, rgba).map_err(|e| unreadable(e.to_string()))?;
        Ok(ForGeneration { width, height, picture: Some(picture), from_srgb })
    }

    /// Shows `overlay` (an area and its premultiplied pixels) over the canvas, or nothing.
    fn set_overlay(&self, overlay: Option<(IRect, &[u8])>) {
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        let (Some(canvas), Some(gpu)) = (st.canvas.as_mut(), st.gpu.as_ref()) else { return };
        if overlay.is_none() && !canvas.has_overlay() {
            return;
        }
        if let Err(e) = canvas.set_overlay(gpu, overlay) {
            log::warn!("generation: no overlay: {e}");
        }
        self.shared.frame(st);
    }

    /// A new layer at the top of the stack holding `pixels` (premultiplied, `area`'s size) at
    /// `area`, named `name`, as one undo step; its id.
    fn add_generated(&self, area: IRect, pixels: &[u8], name: &str) -> Option<u64> {
        let id = self.change("generate", |d| {
            if !d.has_room(1) || area.intersect(IRect::from_size(d.width(), d.height())) != area {
                return None;
            }
            let id = d.add_layer();
            d.rename(id, name);
            let others = d.members(None).iter().filter(|&&m| m != id).count();
            d.move_layer(id, others);
            Some(id.0)
        })?;
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        if let (Some(canvas), Some(gpu)) = (st.canvas.as_mut(), st.gpu.as_ref()) {
            canvas.upload_layer_part(gpu, LayerId(id), area, pixels).ok()?;
        }
        st.revision += 1;
        self.shared.frame(st);
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procraft_gpu::Gpu;

    /// A session on a headless GPU, or none without one.
    fn session(side: u32) -> Option<Arc<CanvasSession>> {
        let gpu = Gpu::headless().map_err(|e| eprintln!("skipped: {e}")).ok()?;
        let s = CanvasSession::new(side, side);
        {
            let mut st = s.shared.lock();
            st.gpu = Some(gpu);
            st.ensure_canvas();
        }
        Some(s)
    }

    fn composite(s: &CanvasSession) -> Vec<u8> {
        let mut guard = s.shared.lock();
        let st = &mut *guard;
        let (canvas, gpu) = (st.canvas.as_mut().unwrap(), st.gpu.as_ref().unwrap());
        canvas.render_finished(gpu, &mut st.doc);
        canvas.read_composite(gpu).unwrap()
    }

    fn at(px: &[u8], side: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * side + x) * 4) as usize;
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    }

    #[test]
    fn a_result_applied_is_a_layer_at_the_top_and_one_undo_step() {
        let Some(s) = session(64) else { return };
        // a second layer, then the first made current: the result still goes on top
        let first = s.shared.lock().doc.current();
        s.change("layer", |d| Some(d.add_layer()));
        s.change("current", |d| d.set_current(first).then_some(()));
        let red = [255, 0, 0, 255].repeat(16 * 16);
        let area = IRect::new(8, 8, 24, 24);
        let id = s.add_generated(area, &red, "Generated · test").unwrap();
        {
            let st = s.shared.lock();
            assert_eq!(st.doc.members(None).last(), Some(&LayerId(id)), "on top");
            assert_eq!(st.doc.layer(LayerId(id)).unwrap().name, "Generated · test");
        }
        let px = composite(&s);
        assert_eq!(at(&px, 64, 10, 10), [255, 0, 0, 255]);
        assert_eq!(at(&px, 64, 40, 40), [255, 255, 255, 255]);
        s.undo();
        assert!(s.shared.lock().doc.layer(LayerId(id)).is_none(), "one undo takes it away");
        assert_eq!(at(&composite(&s), 64, 10, 10), [255, 255, 255, 255]);
        // an area off the canvas is refused
        assert!(s.add_generated(IRect::new(60, 60, 76, 76), &red, "off").is_none());
    }

    #[test]
    fn the_canvas_goes_out_straight_and_previews_stay_out_of_it() {
        let Some(s) = session(32) else { return };
        let picture = s.picture_for_generation(true).unwrap();
        assert_eq!((picture.width, picture.height), (32, 32));
        assert!(picture.picture.unwrap().rgba.iter().all(|&c| c == 255), "white, opaque");
        assert!(s.picture_for_generation(false).unwrap().picture.is_none());
        let blue = [0, 0, 255, 255].repeat(32 * 32);
        s.set_overlay(Some((IRect::from_size(32, 32), &blue)));
        assert!(s.shared.lock().canvas.as_ref().unwrap().has_overlay());
        assert_eq!(at(&composite(&s), 32, 5, 5), [255, 255, 255, 255], "the overlay isn't in the picture");
        s.set_overlay(None);
        assert!(!s.shared.lock().canvas.as_ref().unwrap().has_overlay());
    }

    #[test]
    fn a_size_follows_the_resolution_and_the_cap() {
        let session = CanvasSession::new(2800, 2000);
        let settings = GenerationSettings { address: String::new(), nsfw: false, largest_megapixels: Some(6.0) };
        let generation = Generation::new(settings);
        let quality = generation.size_for(&session, GenerationResolution::Quality);
        assert_eq!((quality.width, quality.height), (2800, 2000));
        assert_eq!(quality.passes, 2);
        assert_eq!((quality.from_width, quality.from_height), (2800, 2000));
        assert!(!quality.capped);
        let normal = generation.size_for(&session, GenerationResolution::Normal);
        assert_eq!((normal.width, normal.height), (1400, 1000));
        assert_eq!(normal.passes, 1, "near the model's size: one pass");
        assert_eq!((normal.from_width, normal.from_height), (1408, 1008));
        assert!(normal.scaled_up);
        let draft = generation.size_for(&session, GenerationResolution::Draft);
        assert_eq!((draft.width, draft.height), (1216, 864));
        assert_eq!(draft.passes, 1);
        assert!(draft.scaled_up);

        let large = CanvasSession::new(4096, 4096);
        let capped = generation.size_for(&large, GenerationResolution::Quality);
        assert!(capped.capped);
        assert_eq!(capped.width % 16, 0);
        assert_eq!(capped.height % 16, 0);
        assert!(u64::from(capped.width) * u64::from(capped.height) <= 6_000_000);
    }

    #[test]
    fn layers_are_named_after_the_prompt() {
        assert_eq!(layer_name("  "), "Generated");
        assert_eq!(layer_name("1girl, stars"), "Generated · 1girl, stars");
        assert_eq!(layer_name(&"a".repeat(40)), format!("Generated · {}…", "a".repeat(32)));
    }

    #[test]
    fn premultiplying_goes_both_ways_near_enough() {
        let mut px = vec![200, 100, 50, 128, 10, 20, 30, 255, 9, 9, 9, 0];
        let before = px.clone();
        premultiply(&mut px);
        assert_eq!(&px[4..8], &before[4..8]);
        unpremultiply(&mut px);
        for (a, b) in px.iter().zip(&before).take(8) {
            assert!(a.abs_diff(*b) <= 1, "{px:?} {before:?}");
        }
    }

    #[test]
    fn a_selection_info_is_its_context_box_and_guessed_mode() {
        let Some(s) = session(200) else { return };
        {
            let mut state = s.shared.lock();
            let mut mask = Mask::empty(200, 200);
            for row in 90..110 {
                for column in 90..110 {
                    mask.data[row * 200 + column] = 255;
                }
            }
            state.selection = Some(mask);
            state.revision += 1;
        }
        let settings = GenerationSettings { address: String::new(), nsfw: false, largest_megapixels: Some(2.0) };
        let generation = Generation::new(settings);
        let info = generation.selection(&s).unwrap();
        assert_eq!((info.x, info.y, info.width, info.height), (26, 26, 148, 148));
        assert_eq!(info.mode, GenerationMode::Fill);
        let size = generation.size_for(&s, GenerationResolution::Quality);
        assert_eq!((size.width, size.height), (148, 148), "the context box is the area");
    }

    #[test]
    fn a_selection_guesses_expand_only_over_transparency_at_an_edge() {
        let mut mask = Mask::empty(16, 16);
        for y in 0..16 {
            for x in 0..8 {
                mask.data[y * 16 + x] = 255;
            }
        }
        let clear = Picture::new(16, 16, vec![0; 16 * 16 * 4]).unwrap();
        assert_eq!(guess_mode(&mask, &clear, (16, 16)), GenerationMode::Expand);
        let opaque = Picture::new(16, 16, vec![255; 16 * 16 * 4]).unwrap();
        assert_eq!(guess_mode(&mask, &opaque, (16, 16)), GenerationMode::Fill);

        let mut inside = Mask::empty(16, 16);
        for y in 4..12 {
            for x in 4..12 {
                inside.data[y * 16 + x] = 255;
            }
        }
        assert_eq!(guess_mode(&inside, &clear, (16, 16)), GenerationMode::Fill);
    }

    #[test]
    fn a_selection_result_keeps_the_context_box_and_is_clear_outside_the_mask() {
        let [x, y, width, height] = context_box([90, 90, 20, 20], (200, 200));
        assert_eq!([x, y, width, height], [26, 26, 148, 148]);
        let mut mask = Mask::empty(width, height);
        for row in 0..height {
            for column in 0..width / 2 {
                mask.data[row as usize * width as usize + column as usize] = 255;
            }
        }
        let run = Running {
            id: 0,
            area: IRect::new(x as i32, y as i32, (x + width) as i32, (y + height) as i32),
            mask: Some(mask.data.clone()),
            name: "Generated · test".to_owned(),
            taken: 0,
            previews: 0,
            preview: None,
        };
        let picture = Picture::new(width, height, [255u8, 0, 0, 255].repeat((width * height) as usize)).unwrap();
        let result = procraft_diffusion::RunResult { seed: 1, picture, generated: (width, height) };
        let kept = keep(1, &result, &run, None).unwrap();
        assert_eq!((kept.area.x0, kept.area.y0, kept.area.x1, kept.area.y1), (26, 26, 174, 174));
        assert_eq!(kept.pixels.first_chunk::<4>(), Some(&[255, 0, 0, 255]));
        let outside = (width as usize / 2) * 4;
        assert_eq!(kept.pixels.get(outside..outside + 4), Some([0, 0, 0, 0].as_slice()));

        let Some(s) = session(200) else { return };
        let id = s.add_generated(kept.area, &kept.pixels, &kept.name).unwrap();
        let px = composite(&s);
        assert_eq!(at(&px, 200, 30, 30), [255, 0, 0, 255], "inside the mask");
        assert_eq!(at(&px, 200, 170, 170), [255, 255, 255, 255], "outside it stays clear");
        assert_eq!(s.shared.lock().doc.layer(LayerId(id)).unwrap().name, "Generated · test");
    }

    #[test]
    fn starting_a_stroke_hides_the_shown_result_but_keeps_the_bar_s() {
        let Some(s) = session(64) else { return };
        let settings = GenerationSettings { address: String::new(), nsfw: false, largest_megapixels: Some(2.0) };
        let generation = Generation::new(settings);
        let kept = Kept {
            id: 1,
            seed: 1,
            name: "Generated · test".to_owned(),
            area: IRect::new(8, 8, 24, 24),
            pixels: [255u8, 0, 0, 255].repeat(16 * 16),
            thumbnail: ThumbnailImage { width: 1, height: 1, rgba: vec![255, 0, 0, 255] },
            scaled_from: (0, 0),
        };
        generation.artwork_of(&s).results.push(kept);
        generation.show_result(&s, Some(1));
        assert_eq!(generation.state().shown, Some(1));
        assert!(s.shared.lock().canvas.as_ref().unwrap().has_overlay());
        s.stroke_begin(
            crate::PointerKind::Finger,
            vec![crate::PenSample {
                x: 10.0,
                y: 10.0,
                pressure: 1.0,
                tilt: 0.0,
                orientation: 0.0,
                time_ms: 0.0,
                roll: 0.0,
            }],
        );
        assert!(!s.shared.lock().canvas.as_ref().unwrap().has_overlay());
        let state = generation.state();
        assert_eq!(state.shown, None);
        assert_eq!(state.results.len(), 1, "the bar keeps the result");
    }
}
