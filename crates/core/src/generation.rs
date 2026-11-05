//! Image generation on the canvas (with the `generation` feature): a ComfyUI server makes pictures
//! from a prompt, and the picture there is, through `procraft-diffusion`; here they're taken from
//! the canvas and given back to it.
//!
//! One [`Generation`] serves the app: its server, the styles there, and the open artwork's run and
//! results. Its methods take the artwork's [`CanvasSession`]; when another artwork is used, the
//! last one's run is cancelled and its results are kept with its folder (in `generation/` inside
//! it, written on a thread of its own), so opening it again brings them back. Once a result is in
//! its folder, only its thumbnail and metadata stay in memory: its full pixels live in its PNG,
//! and at most the one shown (or one just made, until it's written) keeps them here. Results are
//! shown over the canvas (the renderer's overlay, never part of the artwork) until one is applied,
//! which adds it as a layer at the top of the stack, the way it was shown, as one undo step. The
//! app polls [`Generation::state`] and [`Generation::server`], as it polls an export's progress.
//!
//! The model sees sRGB: the picture is converted from the canvas's profile on the way out and
//! the results back on the way in.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use procraft_diffusion::plan::context_box;
use procraft_diffusion::{
    Address, Connection, Control as RequestControl, ControlKind as RequestControlKind, ControlStep as RequestStep,
    Extraction as RequestExtraction, Feature, Picture, Request, Resolution as PlanResolution, RunId,
    Selection as RequestSelection, SelectionMode as RequestMode, Worker, largest_for_vram, plan_for,
};
use procraft_engine::cms::{Lut, builtin};
use procraft_engine::document::{Document, LayerId};
use procraft_engine::geom::IRect;
use procraft_engine::profile::Profile;
use procraft_engine::selection::Mask;

use crate::layers::LayerControl;
use crate::{CanvasSession, ThumbnailImage};

/// Results and previews are listed at most this big.
const THUMBNAIL_SIDE: u32 = 160;
/// A control layer's picture in the popover is at most this big.
const CONTROL_THUMBNAIL_SIDE: u32 = 96;
/// The most pictures one run makes.
const MOST_PICTURES: u32 = 4;
/// A generated layer's name keeps this much of the prompt.
const NAME_PROMPT: usize = 32;
/// Results made this much smaller than the area are noted as scaled up.
const SCALED_UP: f64 = 1.5;
/// The most results one artwork keeps; adding another drops the oldest.
const KEPT_RESULTS: usize = 24;
/// The artwork's private folder of generation results, inside its own folder.
const RESULTS_DIR: &str = "generation";
/// The results' index in [`RESULTS_DIR`], listing them oldest first.
const RESULTS_INDEX: &str = "index.json";
/// The index's format version.
const RESULTS_VERSION: u32 = 1;

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

impl From<LayerControl> for RequestControlKind {
    fn from(kind: LayerControl) -> RequestControlKind {
        match kind {
            LayerControl::LineArt => RequestControlKind::LineArt,
            LayerControl::Scribble => RequestControlKind::Scribble,
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
    ControlFromPicture,
}

impl From<Feature> for GenerationFeature {
    fn from(f: Feature) -> GenerationFeature {
        match f {
            Feature::Generate => GenerationFeature::Generate,
            Feature::Refine => GenerationFeature::Refine,
            Feature::Selections => GenerationFeature::Selections,
            Feature::ControlLayers => GenerationFeature::ControlLayers,
            Feature::ControlFromPicture => GenerationFeature::ControlFromPicture,
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

/// How closely a control layer is followed, from a loose shaping of the picture's start to lines
/// held to the last step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Enum)]
pub enum ControlStep {
    Loose,
    #[default]
    Balanced,
    Close,
    Strict,
}

impl From<ControlStep> for RequestStep {
    fn from(step: ControlStep) -> RequestStep {
        match step {
            ControlStep::Loose => RequestStep::Loose,
            ControlStep::Balanced => RequestStep::Balanced,
            ControlStep::Close => RequestStep::Close,
            ControlStep::Strict => RequestStep::Strict,
        }
    }
}

/// A control layer in a run: layer `layer` followed as closely as `step` says. A visible control
/// layer missing from the run's list is followed at Balanced.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct ControlSetting {
    pub layer: u64,
    pub step: ControlStep,
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
    /// How closely to follow each control layer; layers left out are followed at Balanced.
    #[uniffi(default = [])]
    pub controls: Vec<ControlSetting>,
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
    /// It traces the picture into a control layer rather than making a picture.
    #[uniffi(default = false)]
    pub extracting: bool,
    /// The kind of lines it traces, when tracing.
    #[uniffi(default = None)]
    pub extract_kind: Option<LayerControl>,
    /// The control layer it traces into, when tracing.
    #[uniffi(default = None)]
    pub extract_layer: Option<u64>,
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

/// A layer whose pixels guide generation, as the popover lists it.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ControlLayerInfo {
    pub id: u64,
    pub name: String,
    pub kind: LayerControl,
    /// Small, premultiplied.
    pub thumbnail: ThumbnailImage,
    /// Hidden control layers are ignored by runs.
    pub visible: bool,
}

/// What a resolution makes for the open artwork: the target size, the passes, and the size
/// results are generated at before the final scale to the area.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
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
    /// The cap in force, in megapixels: the Largest picture setting or the model's own limit.
    #[uniffi(default = 0.0)]
    pub largest_megapixels: f32,
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
    /// The control layer a finished trace drew into, reported once so the UIs can refresh their
    /// layer lists.
    #[uniffi(default = None)]
    pub made_layer: Option<u64>,
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
/// canvas's colours, the area's size). The pixels are in memory only for the result shown, one
/// just made until the saver has written it, or (with no folder) all of a session's results;
/// otherwise they're read from the result's PNG when needed. When in memory they're shared with
/// a save waiting to write them, so handing them to the background saver doesn't copy a picture.
#[derive(Clone)]
struct Kept {
    id: u64,
    seed: u64,
    /// The layer's name if it's applied.
    name: String,
    area: IRect,
    /// The pixels, when they're in memory.
    pixels: Option<Arc<Vec<u8>>>,
    thumbnail: ThumbnailImage,
    /// The size it was made at when scaled up to the area; 0 × 0 when it wasn't.
    scaled_from: (u32, u32),
    /// Its PNG is in the artwork's folder.
    saved: bool,
}

/// What the results' index keeps of one result, oldest first.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct SavedResult {
    id: u64,
    seed: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    area: [i32; 4],
    #[serde(default)]
    scaled_from: [u32; 2],
    /// The thumbnail PNG's size.
    #[serde(default)]
    thumbnail: [u32; 2],
}

/// `<folder>/generation/index.json`: the format version and every result kept, in order.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ResultsIndex {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    results: Vec<SavedResult>,
}

/// A result on its way to the background saver: the index's fields, its thumbnail, and its pixels
/// when they're still in memory (a result already written only needs its index entry).
struct WriteResult {
    id: u64,
    seed: u64,
    name: String,
    area: [i32; 4],
    scaled_from: [u32; 2],
    pixels: Option<Arc<Vec<u8>>>,
    thumbnail: ThumbnailImage,
}

/// A save finished on the background thread: where, and which results' PNGs are now written.
struct SavedResults {
    folder: PathBuf,
    ids: Vec<u64>,
}

/// What the background saver does, one at a time and in the order asked.
enum SaveJob {
    /// Write the results and an index of them, and delete files the index no longer lists.
    Save { folder: PathBuf, results: Vec<WriteResult> },
    /// Delete the artwork's kept results.
    Discard { folder: PathBuf },
}

/// The background writer: the UI only hands it jobs, so encoding and writing never hold it up.
struct Saver {
    jobs: Mutex<Option<mpsc::Sender<SaveJob>>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

/// A load finished on its own thread: for which session and which folder, and what it found.
struct LoadedResults {
    session: usize,
    folder: PathBuf,
    epoch: u64,
    results: Vec<Kept>,
    next: u64,
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
    /// A run tracing the picture into a control layer, rather than making a picture.
    extract: Option<Extracting>,
}

/// What a trace run makes: the kind of lines, and the control layer they go into.
struct Extracting {
    kind: LayerControl,
    layer: LayerId,
}

/// A guessed selection mode, kept until the artwork changes.
struct Guess {
    revision: u64,
    area: [u32; 4],
    mode: GenerationMode,
}

/// The open artwork's part: which session it is, its folder, its run, its results.
#[derive(Default)]
struct Artwork {
    /// The session's identity (its shared state's address); 0 before any.
    session: usize,
    /// The session itself, to take the overlay off when a shown result is dropped.
    shared: Option<std::sync::Weak<crate::Shared>>,
    /// The artwork's folder, once the app has said where it is; none for a session-only artwork.
    folder: Option<PathBuf>,
    /// Its results have been read from `folder` (or there are none to read).
    loaded: bool,
    /// A background read of `folder` is under way.
    loading: bool,
    /// Which read is current; one started before a change is ignored when it ends.
    load_epoch: u64,
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
    pub(crate) settings: Mutex<GenerationSettings>,
    artwork: Mutex<Artwork>,
    /// What finished loads report to; [`Generation::state`] takes them in when ready.
    loaded: Mutex<mpsc::Receiver<LoadedResults>>,
    load_sender: mpsc::Sender<LoadedResults>,
    /// Numbers the loads, so one for another artwork or folder is ignored.
    loads: AtomicU64,
    /// Joins the writer before the receiver below goes, so its last report isn't lost.
    saver: Saver,
    /// What finished saves report to; the pixels they wrote can then leave memory.
    saved: Mutex<mpsc::Receiver<SavedResults>>,
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
        let (load_sender, loaded) = mpsc::channel();
        let (saved_sender, saved) = mpsc::channel();
        let generation = Generation {
            worker: Worker::new(),
            settings: Mutex::new(settings.clone()),
            artwork: Mutex::default(),
            loaded: Mutex::new(loaded),
            load_sender,
            loads: AtomicU64::new(0),
            saver: Saver::new(saved_sender),
            saved: Mutex::new(saved),
        };
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

    /// The artwork's control layers, top first, each with a small thumbnail; hidden ones are
    /// included (runs ignore them).
    pub fn control_layers(&self, session: &CanvasSession) -> Vec<ControlLayerInfo> {
        let layers: Vec<(u64, String, LayerControl, bool)> = {
            let st = session.shared.lock();
            st.doc
                .layers()
                .iter()
                .rev()
                .filter_map(|l| l.control.map(|c| (l.id.0, l.name.clone(), c.kind.into(), l.visible)))
                .collect()
        };
        layers
            .into_iter()
            .filter_map(|(id, name, kind, visible)| {
                let thumbnail = session.layer_thumbnail(id, CONTROL_THUMBNAIL_SIDE)?;
                Some(ControlLayerInfo { id, name, kind, thumbnail, visible })
            })
            .collect()
    }

    /// Whether the server can trace the picture into control layers (it has the line extractors).
    pub fn can_extract(&self) -> bool {
        self.worker.server().ready(procraft_diffusion::Family::Anima, Feature::ControlFromPicture)
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
        let family = procraft_diffusion::Family::Anima;
        let largest = self.largest_megapixels(&server).min(family.most_megapixels());
        let plan = plan_for(width, height, family.native_pixels(), resolution.into(), largest, selection.is_some());
        SizeInfo {
            width: plan.target.width,
            height: plan.target.height,
            passes: plan.passes(),
            capped: plan.capped,
            scaled_up: plan.stretched(width, height, SCALED_UP),
            from_width: plan.size.width,
            from_height: plan.size.height,
            largest_megapixels: largest,
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

    /// The artwork open in `session` lives in `folder`: its kept results are read from
    /// `<folder>/generation/` on a thread of their own, and [`state`](Self::state) reports them
    /// once ready. A different folder for the session starts over from that folder's results.
    pub fn set_folder(&self, session: &CanvasSession, folder: String) {
        let Some(folder) = (!folder.trim().is_empty()).then(|| PathBuf::from(folder)) else { return };
        let mut artwork = self.artwork_of(session);
        if artwork.folder.as_ref() == Some(&folder) && artwork.loaded {
            return;
        }
        // Results made before an artwork had a folder stay, and go into it now.
        if artwork.folder.is_none() && !artwork.results.is_empty() {
            artwork.folder = Some(folder.clone());
            artwork.loaded = true;
            artwork.loading = false;
            artwork.load_epoch = 0;
            self.save_results(&folder, &artwork);
            return;
        }
        artwork.load_epoch = 0;
        artwork.loading = false;
        artwork.loaded = false;
        artwork.folder = Some(folder);
        artwork.results.clear();
        artwork.shown = None;
        artwork.error = None;
        self.start_load(&mut artwork);
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
        if artwork.loading {
            return Err(GenerationError::NotReady { reason: "the artwork's results are still loading".to_owned() });
        }
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
        let controls = run_controls(session, &run.controls, area)?;
        if !controls.is_empty() && !server.ready(style.family, Feature::ControlLayers) {
            return Err(GenerationError::NotReady {
                reason: format!("the server isn't ready to {}", Feature::ControlLayers.label()),
            });
        }
        let request = Request {
            prompt: run.prompt,
            style,
            strength: run.strength.clamp(0.0, 1.0),
            seed: run.seed.unwrap_or_else(random_seed),
            count: run.count.clamp(1, MOST_PICTURES),
            picture: request_picture,
            selection: request_selection,
            controls,
            size,
            resolution: run.resolution.unwrap_or_default().into(),
            largest: Some(self.largest_megapixels(&server)),
            nsfw: self.settings.lock().nsfw,
        };
        crate::generation_tags::remember(&request.prompt);
        let name = layer_name(&request.prompt);
        let id = self.worker.submit(request);
        artwork.running = Some(Running { id, area, mask, name, taken: 0, previews: 0, preview: None, extract: None });
        artwork.error = None;
        Ok(id)
    }

    /// Starts tracing the picture as it is seen into control layer `layer`, through the server's
    /// line-art or scribble extractor (the layer's kind): the whole canvas, or the selection when
    /// there is one, without the control layers. The lines replace what the layer had there, as
    /// one undo step, once the run ends ([`state`](Self::state) reports the layer then); the run
    /// shows its progress like any other. Returns the run's id.
    pub fn trace_control(&self, session: &CanvasSession, layer: u64) -> Result<u64, GenerationError> {
        let layer = LayerId(layer);
        let kind: LayerControl = {
            let st = session.shared.lock();
            match st.doc.layer(layer).and_then(|l| l.control) {
                Some(control) => control.kind.into(),
                None => {
                    return Err(GenerationError::NotReady { reason: "that layer isn't a control layer".to_owned() });
                }
            }
        };
        let server = self.worker.server();
        if server.connection != Connection::Connected {
            return Err(GenerationError::NotReady { reason: "not connected to a server".to_owned() });
        }
        let family = procraft_diffusion::Family::Anima;
        if !server.ready(family, Feature::ControlFromPicture) {
            let missing: Vec<String> = server
                .readiness
                .iter()
                .find(|(f, _)| *f == family)
                .map(|(_, r)| r.missing(Feature::ControlFromPicture).iter().map(ToString::to_string).collect())
                .unwrap_or_default();
            let reason = if missing.iter().any(|m| m.contains("comfyui_controlnet_aux")) {
                "needs comfyui_controlnet_aux".to_owned()
            } else {
                format!("the server isn't ready to {}", Feature::ControlFromPicture.label())
            };
            return Err(GenerationError::NotReady { reason });
        }
        let mut artwork = self.artwork_of(session);
        if artwork.loading {
            return Err(GenerationError::NotReady { reason: "the artwork's results are still loading".to_owned() });
        }
        if artwork.running.is_some() {
            return Err(GenerationError::NotReady { reason: "a run is going".to_owned() });
        }
        // the visible picture, control layers left out, over the whole canvas or the selection's
        // bounds (not the context box: the lines are wanted exactly where they were drawn)
        let selected = current_selection(session);
        let area = match &selected {
            Some(selected) => {
                let [x, y, width, height] = selected.bounds;
                IRect::new(
                    i32::try_from(x).unwrap_or(i32::MAX),
                    i32::try_from(y).unwrap_or(i32::MAX),
                    i32::try_from(x.saturating_add(width)).unwrap_or(i32::MAX),
                    i32::try_from(y.saturating_add(height)).unwrap_or(i32::MAX),
                )
            }
            None => {
                let state = session.shared.lock_finished();
                IRect::from_size(state.doc.width(), state.doc.height())
            }
        };
        let mask = selected.as_ref().map(|selected| crate::layer_ops::mask_part(&selected.mask, area));
        let picture = session.extraction_picture(area)?;
        let id =
            self.worker.extract(RequestExtraction { kind: kind.into(), picture, size: (area.width(), area.height()) });
        artwork.running = Some(Running {
            id,
            area,
            mask,
            name: String::new(),
            taken: 0,
            previews: 0,
            preview: None,
            extract: Some(Extracting { kind, layer }),
        });
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
    /// since last asked, and results its folder's read has brought back.
    pub fn state(&self) -> GenerationState {
        let mut guard = self.artwork.lock();
        let artwork = &mut *guard;
        self.take_loaded(artwork);
        self.take_saved(artwork);
        self.ensure_loading(artwork);
        let mut running = None;
        let made = self.take_made(artwork);
        let mut made_layer = None;
        if let Some(r) = &mut artwork.running
            && let Some(run) = self.worker.run(r.id)
        {
            if run.previews != r.previews {
                r.previews = run.previews;
                r.preview = run.preview.as_ref().and_then(thumbnail);
            }
            if run.finished {
                artwork.error = run.error.filter(|e| *e != procraft_diffusion::Error::Cancelled).map(|e| e.to_string());
                if let Some(extract) = &r.extract
                    && artwork.error.is_none()
                    && let Some(result) = run.results.first()
                {
                    match trace_into(artwork.shared.as_ref(), r.area, r.mask.as_deref(), extract, &result.picture) {
                        Ok(id) => made_layer = Some(id),
                        Err(reason) => artwork.error = Some(reason),
                    }
                }
                self.worker.forget(r.id);
            } else {
                running = Some(RunInfo {
                    id: r.id,
                    count: run.count,
                    done: u32::try_from(run.results.len()).unwrap_or(0),
                    progress: run.progress,
                    preview: r.preview.clone(),
                    previews: r.previews,
                    extracting: r.extract.is_some(),
                    extract_kind: r.extract.as_ref().map(|e| e.kind),
                    extract_layer: r.extract.as_ref().map(|e| e.layer.0),
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
            release_hidden_pixels(artwork);
        }
        let trimmed = trim_results(artwork);
        if trimmed && artwork.shown.is_some_and(|id| !artwork.results.iter().any(|k| k.id == id)) {
            if let Some(shared) = artwork.shared.as_ref().and_then(std::sync::Weak::upgrade) {
                // on the shared state: a CanvasSession made here would detach the window when dropped
                set_overlay(&shared, None);
            }
            artwork.shown = None;
        }
        if trimmed {
            release_hidden_pixels(artwork);
        }
        if made || trimmed {
            self.save_kept(artwork);
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
            made_layer,
            error: artwork.error.clone(),
        }
    }

    /// Shows result `result` over `session`'s canvas, or none. A result whose pixels are only in
    /// its PNG is read here (a tap, not a loop); the one shown is the only one that keeps them.
    pub fn show_result(&self, session: &CanvasSession, result: Option<u64>) {
        let mut artwork = self.artwork_of(session);
        // let the one shown go before the next is read, so only one is ever held
        if result != artwork.shown {
            artwork.shown = None;
            release_hidden_pixels(&mut artwork);
        }
        let shown = result.and_then(|id| self.pixels_for(&mut artwork, id).map(|pixels| (id, pixels)));
        if let Some((id, pixels)) = shown
            && let Some(area) = artwork.results.iter().find(|k| k.id == id).map(|k| k.area)
        {
            session.set_overlay(Some((area, pixels.as_slice())));
            artwork.shown = Some(id);
        } else {
            session.set_overlay(None);
            artwork.shown = None;
        }
        artwork.shown_at = artwork.strokes.as_ref().map_or(0, |strokes| strokes.load(Ordering::Relaxed));
        release_hidden_pixels(&mut artwork);
    }

    /// Adds result `result` to `session`'s artwork as a new layer at the top of the stack (where
    /// it showed), named after its prompt, as one undo step; returns the layer's id. Its pixels
    /// are read from its PNG when they aren't in memory.
    pub fn apply_result(&self, session: &CanvasSession, result: u64) -> Option<u64> {
        let mut artwork = self.artwork_of(session);
        // the result applied takes the overlay's place; let the one shown go first
        if artwork.shown != Some(result) {
            session.set_overlay(None);
            artwork.shown = None;
            release_hidden_pixels(&mut artwork);
        }
        let pixels = self.pixels_for(&mut artwork, result)?;
        let (area, name) = artwork.results.iter().find(|k| k.id == result).map(|k| (k.area, k.name.clone()))?;
        session.set_overlay(None);
        let layer = session.add_generated(area, pixels.as_slice(), &name);
        artwork.shown = None;
        release_hidden_pixels(&mut artwork);
        layer
    }

    /// Deletes result `result` (hidden first if it's shown), from the bar and from the artwork's
    /// folder: for the pictures nobody wants to keep.
    pub fn delete_result(&self, session: &CanvasSession, result: u64) {
        let mut artwork = self.artwork_of(session);
        if artwork.shown == Some(result) {
            session.set_overlay(None);
            artwork.shown = None;
        }
        let before = artwork.results.len();
        artwork.results.retain(|kept| kept.id != result);
        if artwork.results.len() != before
            && let Some(folder) = artwork.folder.clone()
        {
            self.save_results(&folder, &artwork);
        }
    }

    /// Drops the results (and the one shown), deleting them from the artwork's folder; a run
    /// going carries on.
    pub fn discard_results(&self, session: &CanvasSession) {
        let mut artwork = self.artwork_of(session);
        session.set_overlay(None);
        artwork.results.clear();
        artwork.shown = None;
        artwork.error = None;
        if let Some(folder) = artwork.folder.clone() {
            // a read still under way must not bring them back
            artwork.load_epoch = 0;
            artwork.loading = false;
            artwork.loaded = true;
            self.saver.discard(folder);
        }
    }

    /// The artwork is closing: its run is cancelled (the pictures it finished are kept), it is
    /// hidden, and (when it has a folder) its results are kept in the folder and their pixels let
    /// go; opening it again reads them back. A session-only artwork keeps them in memory.
    pub fn leave(&self, session: &CanvasSession) {
        let mut artwork = self.artwork.lock();
        if artwork.session != session_key(session) {
            return;
        }
        session.set_overlay(None);
        if let Some(running) = &artwork.running {
            self.worker.cancel(running.id);
        }
        let _ = self.take_made(&mut artwork);
        if let Some(running) = artwork.running.take() {
            self.worker.forget(running.id);
        }
        let _ = trim_results(&mut artwork);
        artwork.shown = None;
        artwork.error = None;
        artwork.load_epoch = 0;
        artwork.loading = false;
        if let Some(folder) = artwork.folder.take() {
            // A read still under way: what is in the folder is already what there is; an empty
            // save here would put the index over it.
            if artwork.loaded && !artwork.results.is_empty() {
                self.save_results(&folder, &artwork);
            }
            artwork.results.clear();
            artwork.loaded = true;
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
            artwork.shared = Some(Arc::downgrade(&session.shared));
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

    /// Takes in the results a background read brought, if one is for the artwork as it is now.
    fn take_loaded(&self, artwork: &mut Artwork) {
        let loaded = self.loaded.lock();
        while let Ok(found) = loaded.try_recv() {
            if found.session != artwork.session
                || found.epoch != artwork.load_epoch
                || artwork.folder.as_ref() != Some(&found.folder)
            {
                continue;
            }
            let LoadedResults { mut results, next, .. } = found;
            // results made while the folder was being read go after the saved ones, with ids of
            // their own
            if !artwork.results.is_empty() {
                let mut next = next.max(artwork.next);
                for kept in &mut artwork.results {
                    let old = kept.id;
                    kept.id = next;
                    next = next.saturating_add(1);
                    if artwork.shown == Some(old) {
                        artwork.shown = Some(kept.id);
                    }
                }
                let made = std::mem::take(&mut artwork.results);
                results.extend(made);
                artwork.next = next;
            } else {
                artwork.next = artwork.next.max(next);
            }
            artwork.results = results;
            artwork.loaded = true;
            artwork.loading = false;
            artwork.load_epoch = 0;
        }
    }

    /// Takes in what a background save wrote: those results are on disk now, so unless one is
    /// shown its pixels leave memory (the writer's copy went with the job).
    fn take_saved(&self, artwork: &mut Artwork) {
        let saved = self.saved.lock();
        while let Ok(done) = saved.try_recv() {
            if artwork.folder.as_ref() != Some(&done.folder) {
                continue;
            }
            let shown = artwork.shown;
            for kept in &mut artwork.results {
                if done.ids.contains(&kept.id) {
                    kept.saved = true;
                    if shown != Some(kept.id) {
                        kept.pixels = None;
                    }
                }
            }
        }
    }

    /// Result `id`'s pixels: those in memory, or (when only its PNG has them) read from there and
    /// kept for the one shown. None if it isn't a result or its PNG can't be read.
    fn pixels_for(&self, artwork: &mut Artwork, id: u64) -> Option<Arc<Vec<u8>>> {
        let kept = artwork.results.iter().find(|k| k.id == id)?;
        if let Some(pixels) = &kept.pixels {
            return Some(Arc::clone(pixels));
        }
        let folder = artwork.folder.clone()?;
        let area = kept.area;
        let path = folder.join(RESULTS_DIR).join(result_name(id));
        let pixels = match crate::project::read_png(&path, area.width(), area.height()) {
            Ok(pixels) => Arc::new(pixels),
            Err(e) => {
                log::warn!("generation: result {id}: {e}");
                return None;
            }
        };
        if let Some(kept) = artwork.results.iter_mut().find(|k| k.id == id) {
            kept.pixels = Some(Arc::clone(&pixels));
        }
        Some(pixels)
    }

    /// Reads the artwork's results if its folder was set but not read yet.
    fn ensure_loading(&self, artwork: &mut Artwork) {
        if artwork.folder.is_some() && !artwork.loaded && !artwork.loading {
            self.start_load(artwork);
        }
    }

    /// Reads `artwork`'s results on a thread of their own; [`Generation::state`] takes them in.
    fn start_load(&self, artwork: &mut Artwork) {
        let Some(folder) = artwork.folder.clone() else { return };
        let epoch = self.loads.fetch_add(1, Ordering::Relaxed).saturating_add(1);
        artwork.load_epoch = epoch;
        artwork.loading = true;
        artwork.loaded = false;
        let session = artwork.session;
        let sender = self.load_sender.clone();
        let started = std::thread::Builder::new().name("generation-results".into()).spawn(move || {
            let found = load_results(&folder);
            let _ = sender.send(LoadedResults { session, folder, epoch, results: found.results, next: found.next });
        });
        if let Err(e) = started {
            log::error!("generation: reading the artwork's results: {e}");
            artwork.loading = false;
            artwork.loaded = true;
        }
    }

    /// The results the run going has made since last taken into `artwork`; whether it made any.
    fn take_made(&self, artwork: &mut Artwork) -> bool {
        let mut made = false;
        if let Some(r) = &mut artwork.running
            && let Some(run) = self.worker.run(r.id)
        {
            // an extraction's picture becomes a layer, not a result in the bar
            if r.extract.is_some() {
                return false;
            }
            for result in run.results.iter().skip(r.taken) {
                artwork.next = artwork.next.saturating_add(1);
                if let Some(kept) = keep(artwork.next, result, r, artwork.from_srgb.as_ref()) {
                    artwork.results.push(kept);
                    made = true;
                }
            }
            r.taken = run.results.len();
        }
        made
    }

    /// Hands `artwork`'s results to the saver, if it has a folder.
    fn save_kept(&self, artwork: &Artwork) {
        if let Some(folder) = artwork.folder.clone() {
            self.save_results(&folder, artwork);
        }
    }

    /// Hands `artwork`'s results to the background saver for `folder`.
    fn save_results(&self, folder: &Path, artwork: &Artwork) {
        let results = artwork
            .results
            .iter()
            .map(|kept| {
                let IRect { x0, y0, x1, y1 } = kept.area;
                let (scaled_from_width, scaled_from_height) = kept.scaled_from;
                WriteResult {
                    id: kept.id,
                    seed: kept.seed,
                    name: kept.name.clone(),
                    area: [x0, y0, x1, y1],
                    scaled_from: [scaled_from_width, scaled_from_height],
                    pixels: kept.pixels.as_ref().map(Arc::clone),
                    thumbnail: kept.thumbnail.clone(),
                }
            })
            .collect();
        self.saver.save(folder.to_path_buf(), results);
    }
}

/// What a finished read found: the results, oldest first, and the next id to use.
struct FoundResults {
    results: Vec<Kept>,
    next: u64,
}

impl Saver {
    fn new(done: mpsc::Sender<SavedResults>) -> Saver {
        let (sender, receiver) = mpsc::channel();
        let started = std::thread::Builder::new().name("generation-save".into()).spawn(move || {
            while let Ok(job) = receiver.recv() {
                if let Some(saved) = run_save_job(job)
                    && let Err(e) = done.send(saved)
                {
                    log::error!("generation: reporting saved results: {e}");
                }
            }
        });
        let thread = match started {
            Ok(thread) => Some(thread),
            Err(e) => {
                log::error!("generation: no thread to keep results: {e}");
                None
            }
        };
        Saver { jobs: Mutex::new(Some(sender)), thread: Mutex::new(thread) }
    }

    fn save(&self, folder: PathBuf, results: Vec<WriteResult>) {
        self.send(SaveJob::Save { folder, results });
    }

    fn discard(&self, folder: PathBuf) {
        self.send(SaveJob::Discard { folder });
    }

    fn send(&self, job: SaveJob) {
        let jobs = self.jobs.lock();
        if let Some(jobs) = jobs.as_ref()
            && let Err(e) = jobs.send(job)
        {
            log::error!("generation: keeping results: {e}");
        }
    }
}

impl Drop for Saver {
    fn drop(&mut self) {
        // Closing the queue lets the thread finish what it was given, then it ends.
        self.jobs.lock().take();
        if let Some(thread) = self.thread.lock().take() {
            let _ = thread.join();
        }
    }
}

/// Does one saver job, on the saver's thread; a save says what it wrote.
fn run_save_job(job: SaveJob) -> Option<SavedResults> {
    match job {
        SaveJob::Save { folder, results } => {
            let ids = write_results(&folder, &results);
            Some(SavedResults { folder, ids })
        }
        SaveJob::Discard { folder } => {
            let dir = folder.join(RESULTS_DIR);
            match fs::remove_dir_all(&dir) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => log::error!("generation: discarding results: {e}"),
            }
            None
        }
    }
}

/// What a result's PNG is called in `<folder>/generation/`.
fn result_name(id: u64) -> String {
    format!("{id}.png")
}

/// What a result's thumbnail PNG is called beside it.
fn thumbnail_name(id: u64) -> String {
    format!("{id}.thumb.png")
}

/// The result id a PNG in the results folder belongs to, full or thumbnail; none for other files.
fn result_id(path: &Path) -> Option<u64> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".thumb.png").or_else(|| name.strip_suffix(".png"))?;
    stem.parse().ok()
}

/// Writes `results` into `<folder>/generation/`: a lossless PNG and a small thumbnail PNG each,
/// then the index, each beside its place until whole and renamed into it. Files the index no
/// longer lists go afterwards; the ids written are returned.
fn write_results(folder: &Path, results: &[WriteResult]) -> Vec<u64> {
    let dir = folder.join(RESULTS_DIR);
    if let Err(e) = fs::create_dir_all(&dir) {
        log::error!("generation: making the results folder: {e}");
        return Vec::new();
    }
    let mut written = Vec::new();
    let mut index = Vec::new();
    for result in results {
        let [x0, y0, x1, y1] = result.area;
        let area = IRect::new(x0, y0, x1, y1);
        let (width, height) = (area.width(), area.height());
        if width == 0 || height == 0 {
            continue;
        }
        let path = dir.join(result_name(result.id));
        if !path.is_file() {
            let Some(pixels) = &result.pixels else {
                log::error!("generation: result {} has no pixels to write", result.id);
                continue;
            };
            let saved = crate::project::write_atomically(&path, |p| {
                crate::project::write_png(p, width, height, pixels.as_slice(), &[])
            });
            if let Err(e) = saved {
                log::error!("generation: writing result {}: {e}", result.id);
                continue;
            }
        }
        let (thumb_width, thumb_height) = (result.thumbnail.width, result.thumbnail.height);
        let thumb_path = dir.join(thumbnail_name(result.id));
        if !thumb_path.is_file() {
            let saved = crate::project::write_atomically(&thumb_path, |p| {
                crate::project::write_png(p, thumb_width, thumb_height, &result.thumbnail.rgba, &[])
            });
            if let Err(e) = saved {
                log::error!("generation: writing result {}'s thumbnail: {e}", result.id);
                continue;
            }
        }
        written.push(result.id);
        index.push(SavedResult {
            id: result.id,
            seed: result.seed,
            name: result.name.clone(),
            area: result.area,
            scaled_from: result.scaled_from,
            thumbnail: [thumb_width, thumb_height],
        });
    }
    let text = match serde_json::to_vec(&ResultsIndex { version: RESULTS_VERSION, results: index }) {
        Ok(text) => text,
        Err(e) => {
            log::error!("generation: writing the results index: {e}");
            return Vec::new();
        }
    };
    let placed = crate::project::write_atomically(&dir.join(RESULTS_INDEX), |p| {
        fs::write(p, &text).map_err(|e| crate::project::file_error("writing the results", e))
    });
    if let Err(e) = placed {
        log::error!("generation: writing the results index: {e}");
        return Vec::new();
    }
    // Only now that the index is in place: files it doesn't list (the oldest past the limit, or
    // those a save cut short left behind) go.
    clean_results(&dir, &written);
    written
}

/// Removes result PNGs (full and thumbnail) in `dir` whose ids `kept` doesn't list.
fn clean_results(dir: &Path, kept: &[u64]) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(std::ffi::OsStr::to_str) != Some("png") {
            continue;
        }
        if result_id(&path).is_some_and(|id| !kept.contains(&id)) {
            let _ = fs::remove_file(path);
        }
    }
}

/// Reads `<folder>/generation/`: the index and each result's thumbnail, on a thread of its own.
/// The full pixels stay in their PNGs until a result is shown or applied.
fn load_results(folder: &Path) -> FoundResults {
    let dir = folder.join(RESULTS_DIR);
    // ids an interrupted save may have written without the index: don't use them again
    let mut next = highest_result_id(&dir);
    let index =
        fs::read(dir.join(RESULTS_INDEX)).ok().and_then(|bytes| serde_json::from_slice::<ResultsIndex>(&bytes).ok());
    let Some(index) = index else {
        return FoundResults { results: Vec::new(), next: next.saturating_add(1) };
    };
    if index.version > RESULTS_VERSION {
        log::warn!("generation: results of a newer version ({})", index.version);
        return FoundResults { results: Vec::new(), next: next.saturating_add(1) };
    }
    let mut results = Vec::new();
    for saved in index.results {
        let SavedResult {
            id,
            seed,
            name,
            area: [x0, y0, x1, y1],
            scaled_from: [from_width, from_height],
            thumbnail: [thumb_width, thumb_height],
        } = saved;
        next = next.max(id);
        let area = IRect::new(x0, y0, x1, y1);
        let (width, height) = (area.width(), area.height());
        // an index damaged anyhow: no result is that large, and no thumbnail bigger than these
        let most = procraft_engine::document::canvas_edge_most();
        if width == 0 || height == 0 || width > most || height > most {
            continue;
        }
        if thumb_width == 0 || thumb_height == 0 || thumb_width > THUMBNAIL_SIDE || thumb_height > THUMBNAIL_SIDE {
            continue;
        }
        let path = dir.join(thumbnail_name(id));
        let rgba = match crate::project::read_png(&path, thumb_width, thumb_height) {
            Ok(rgba) => rgba,
            Err(e) => {
                log::warn!("generation: result {id}'s thumbnail: {e}");
                continue;
            }
        };
        results.push(Kept {
            id,
            seed,
            name,
            area,
            pixels: None,
            thumbnail: ThumbnailImage { width: thumb_width, height: thumb_height, rgba },
            scaled_from: (from_width, from_height),
            saved: true,
        });
    }
    if results.len() > KEPT_RESULTS {
        let excess = results.len().saturating_sub(KEPT_RESULTS);
        results.drain(..excess);
    }
    FoundResults { results, next: next.saturating_add(1) }
}

/// The highest id a result PNG in `dir` has, or 0.
fn highest_result_id(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .filter(|entry| entry.path().extension().and_then(std::ffi::OsStr::to_str) == Some("png"))
        .filter_map(|entry| result_id(&entry.path()))
        .max()
        .unwrap_or(0)
}

/// Drops the oldest results past [`KEPT_RESULTS`]; whether any went.
fn trim_results(artwork: &mut Artwork) -> bool {
    let mut trimmed = false;
    while artwork.results.len() > KEPT_RESULTS {
        artwork.results.remove(0);
        trimmed = true;
    }
    trimmed
}

/// Lets go of the pixels of every result that isn't shown: its PNG has them, so they can be read
/// again if it's shown later. A result just made keeps them until the saver says it's written.
fn release_hidden_pixels(artwork: &mut Artwork) {
    let shown = artwork.shown;
    for kept in &mut artwork.results {
        if kept.saved && shown != Some(kept.id) {
            kept.pixels = None;
        }
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

/// The visible control layers' pictures over `area`, each followed as closely as its setting says
/// (Balanced when the run has none for it). A layer with nothing drawn in `area` is left out: a
/// blank guide would only steer the picture towards empty space.
fn run_controls(
    session: &CanvasSession,
    settings: &[ControlSetting],
    area: IRect,
) -> Result<Vec<RequestControl>, GenerationError> {
    let layers: Vec<(LayerId, LayerControl)> = {
        let st = session.shared.lock();
        st.doc.control_layers().filter_map(|l| l.control.map(|c| (l.id, c.kind.into()))).collect()
    };
    let mut controls = Vec::new();
    for (id, kind) in layers {
        let step = settings.iter().find(|s| s.layer == id.0).map_or_else(ControlStep::default, |s| s.step);
        let (strength, end_percent) = procraft_diffusion::Family::Anima.control_settings(step.into());
        let Some(picture) = session.control_picture(id, area)? else { continue };
        controls.push(RequestControl { kind: kind.into(), strength, end_percent, picture });
    }
    Ok(controls)
}

/// `picture` scaled down to the family's most megapixels, its shape kept, if it is bigger.
fn scaled_to_cap(picture: Picture) -> Picture {
    let most = f64::from(procraft_diffusion::Family::Anima.most_megapixels()) * 1_000_000.0;
    let pixels = f64::from(picture.width) * f64::from(picture.height);
    if pixels <= most {
        return picture;
    }
    let scale = (most / pixels).sqrt();
    let width = (f64::from(picture.width) * scale).round().max(1.0) as u32;
    let height = (f64::from(picture.height) * scale).round().max(1.0) as u32;
    picture.resized(width, height).unwrap_or(picture)
}

/// Puts a trace run's lines into its control layer over `area` (within the selection's `mask`
/// when there was one), replacing what the layer had there, as one undo step; the layer's id, or
/// why it couldn't.
fn trace_into(
    shared: Option<&std::sync::Weak<crate::Shared>>,
    area: IRect,
    mask: Option<&[u8]>,
    extract: &Extracting,
    lines: &Picture,
) -> Result<u64, String> {
    let shared = shared.and_then(std::sync::Weak::upgrade).ok_or_else(|| "the canvas is closed".to_owned())?;
    // on the artwork's shared state itself: a CanvasSession made here would detach the window
    // when dropped, and the canvas would stop drawing
    replace_control_pixels(&shared, extract.layer, area, lines.lines_alpha(), mask)
        .ok_or_else(|| "The control layer is gone".to_owned())
}

/// Shows `overlay` (an area and its premultiplied pixels) over `shared`'s canvas, or nothing.
fn set_overlay(shared: &crate::Shared, overlay: Option<(IRect, &[u8])>) {
    let mut guard = shared.lock();
    let st = &mut *guard;
    let (Some(canvas), Some(gpu)) = (st.canvas.as_mut(), st.gpu.as_ref()) else { return };
    if overlay.is_none() && !canvas.has_overlay() {
        return;
    }
    if let Err(e) = canvas.set_overlay(gpu, overlay) {
        log::warn!("generation: no overlay: {e}");
    }
    shared.frame(st);
}

/// Replaces control layer `layer`'s pixels over `area` with `lines` (premultiplied, `area`'s
/// size), within `mask` when there is one (outside it the layer keeps its own), as one undo step
/// named "trace control layer"; the layer's id, or none when it is gone or isn't a control layer.
fn replace_control_pixels(
    shared: &crate::Shared,
    layer: LayerId,
    area: IRect,
    mut lines: Vec<u8>,
    mask: Option<&[u8]>,
) -> Option<u64> {
    let mut guard = shared.lock_finished();
    let st = &mut *guard;
    st.finish_stroke();
    st.settle();
    st.flush();
    if st.doc.layer(layer).and_then(|l| l.control).is_none()
        || area.intersect(IRect::from_size(st.doc.width(), st.doc.height())) != area
    {
        return None;
    }
    let before = st.doc.clone();
    let (Some(canvas), Some(gpu)) = (st.canvas.as_mut(), st.gpu.as_ref()) else { return None };
    if let Some(mask) = mask {
        canvas.render_finished(gpu, &mut st.doc);
        // a layer never painted has no pixels yet: nothing of its own to keep
        let own = canvas.read_layer_part(gpu, layer, area).unwrap_or_else(|_| vec![0; lines.len()]);
        for ((px, old), &m) in lines.as_chunks_mut::<4>().0.iter_mut().zip(own.as_chunks::<4>().0).zip(mask) {
            let m = u16::from(m);
            for (c, &o) in px.iter_mut().zip(old) {
                *c = ((u16::from(*c) * m + u16::from(o) * (255 - m) + 127) / 255) as u8;
            }
        }
    }
    canvas.name_next_step("trace control layer");
    canvas.write_layer_parts_and_document(vec![(layer, area, lines)], before);
    st.revision += 1;
    shared.frame(st);
    Some(layer.0)
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
        pixels: Some(Arc::new(pixels)),
        thumbnail,
        scaled_from,
        saved: false,
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
        // control layers guide the run with their own pixels, not painted in as colour
        let left_out: Vec<LayerId> = st.doc.control_layers().map(|l| l.id).collect();
        let mut rgba =
            canvas.read_composite_without(gpu, &mut st.doc, &left_out).map_err(|e| unreadable(e.to_string()))?;
        if let Some(lut) = &to_srgb {
            lut.apply_rgba(&mut rgba, true);
        }
        unpremultiply(&mut rgba);
        let picture = Picture::new(width, height, rgba).map_err(|e| unreadable(e.to_string()))?;
        Ok(ForGeneration { width, height, picture: Some(picture), from_srgb })
    }

    /// Layer `id`'s pixels over `area`, flattened onto white and turned into white lines on black
    /// (a control image), at the area's size; none when nothing is drawn there.
    fn control_picture(&self, id: LayerId, area: IRect) -> Result<Option<Picture>, GenerationError> {
        let mut guard = self.shared.lock_finished();
        let st = &mut *guard;
        st.finish_stroke();
        st.settle();
        st.flush();
        let unreadable = |reason: String| GenerationError::Canvas { reason };
        let (Some(canvas), Some(gpu)) = (st.canvas.as_mut(), st.gpu.as_ref()) else {
            return Err(unreadable("the canvas isn't drawn yet".to_owned()));
        };
        canvas.render_finished(gpu, &mut st.doc);
        let pixels = match canvas.read_layer_part(gpu, id, area) {
            Ok(pixels) => pixels,
            // a layer never painted has no pixels yet
            Err(e) if e.to_string().contains("no layer") => return Ok(None),
            Err(e) => return Err(unreadable(e.to_string())),
        };
        if pixels.as_chunks::<4>().0.iter().all(|px| px[3] == 0) {
            return Ok(None);
        }
        Picture::control(area.width(), area.height(), &pixels).map(Some).map_err(|e| unreadable(e.to_string()))
    }

    /// The picture for a control layer from the picture: `area`'s part of the canvas without its
    /// control layers, in sRGB and flattened onto white, at its own size up to the family's most.
    fn extraction_picture(&self, area: IRect) -> Result<Picture, GenerationError> {
        let canvas = self.picture_for_generation(true)?;
        let Some(picture) = canvas.picture else {
            return Err(GenerationError::Canvas { reason: "the canvas isn't drawn yet".to_owned() });
        };
        let (x, y) = (u32::try_from(area.x0).unwrap_or(0), u32::try_from(area.y0).unwrap_or(0));
        let cropped = picture
            .cropped(x, y, area.width(), area.height())
            .map_err(|e| GenerationError::Canvas { reason: e.to_string() })?;
        Ok(scaled_to_cap(cropped.flattened([255, 255, 255])))
    }

    /// Shows `overlay` (an area and its premultiplied pixels) over the canvas, or nothing.
    fn set_overlay(&self, overlay: Option<(IRect, &[u8])>) {
        set_overlay(&self.shared, overlay);
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
    fn control_layers_are_listed_and_hidden_ones_are_left_out_of_runs() {
        let Some(s) = session(32) else { return };
        let id = s.layers()[0].id;
        assert!(s.set_layer_control(id, Some(LayerControl::LineArt)));
        assert_eq!(s.layers()[0].control, Some(LayerControl::LineArt));
        let lines = Picture::new(32, 32, vec![255; 32 * 32 * 4]).unwrap().lines_alpha();
        assert!(replace_control_pixels(&s.shared, LayerId(id), IRect::from_size(32, 32), lines, None).is_some());
        assert!(s.layer_options(id).iter().any(|r| r.option == crate::LayerOption::ControlLayer));
        let generation = Generation::new(settings());
        let listed = generation.control_layers(&s);
        assert_eq!(listed.len(), 1);
        assert_eq!((listed[0].id, listed[0].kind, listed[0].visible), (id, LayerControl::LineArt, true));
        assert!(listed[0].thumbnail.width > 0 && listed[0].thumbnail.height > 0);
        // hidden, it is ignored by a run; shown, it is followed at Balanced by default
        assert!(s.set_layer_visible(id, false));
        assert!(!generation.control_layers(&s)[0].visible);
        assert!(run_controls(&s, &[], IRect::from_size(32, 32)).unwrap().is_empty());
        assert!(s.set_layer_visible(id, true));
        let defaults = run_controls(&s, &[], IRect::from_size(32, 32)).unwrap();
        assert_eq!((defaults.len(), defaults[0].strength, defaults[0].end_percent), (1, 0.6, 0.6));
        // a setting replaces the default
        let close = [ControlSetting { layer: id, step: ControlStep::Close }];
        let close = run_controls(&s, &close, IRect::from_size(32, 32)).unwrap();
        assert_eq!((close[0].strength, close[0].end_percent), (0.7, 0.8));
        let strict = [ControlSetting { layer: id, step: ControlStep::Strict }];
        let strict = run_controls(&s, &strict, IRect::from_size(32, 32)).unwrap();
        assert_eq!((strict[0].strength, strict[0].end_percent), (1.0, 1.0));
        // turned off, it is not listed or used
        assert!(s.set_layer_control(id, None));
        assert!(generation.control_layers(&s).is_empty());
    }

    #[test]
    fn tracing_replaces_a_control_layer_s_pixels_as_one_undo_step() {
        let Some(s) = session(64) else { return };
        let id = s.add_control_layer(LayerControl::LineArt);
        assert!(id != 0);
        let layer = LayerId(id);
        // white lines on black come back as black lines, their luminance as alpha
        let lines = Picture::new(4, 4, vec![255; 64]).unwrap().lines_alpha();
        assert_eq!(&lines[..4], &[0, 0, 0, 255]);
        let area = IRect::from_size(4, 4);
        assert_eq!(replace_control_pixels(&s.shared, layer, area, lines.clone(), None), Some(id));
        assert_eq!(at(&composite(&s), 64, 0, 0), [0, 0, 0, 255], "black lines on the canvas");
        // within a selection: only where it's selected; the rest keeps the layer's own
        let clear = vec![0u8; 64];
        let mut mask = vec![0u8; 16];
        mask[0] = 255;
        assert_eq!(replace_control_pixels(&s.shared, layer, area, clear, Some(&mask)), Some(id));
        let after = composite(&s);
        assert_ne!(at(&after, 64, 0, 0), [0, 0, 0, 255], "selected: replaced");
        assert_eq!(at(&after, 64, 1, 0), [0, 0, 0, 255], "not selected: kept");
        // one undo brings the lines back
        s.undo();
        assert_eq!(at(&composite(&s), 64, 0, 0), [0, 0, 0, 255]);
        // only a control layer is traced into
        assert!(s.set_layer_control(id, None));
        assert_eq!(replace_control_pixels(&s.shared, layer, area, lines, None), None);
    }

    #[test]
    fn a_new_control_layer_is_empty_above_the_current_one_and_one_undo_step() {
        let Some(s) = session(64) else { return };
        let lower = s.shared.lock().doc.current();
        let id = s.add_control_layer(LayerControl::Scribble);
        {
            let st = s.shared.lock();
            let members = st.doc.members(None);
            let at = members.iter().position(|m| *m == lower).map(|i| i + 1);
            assert_eq!(members.iter().position(|m| m.0 == id), at, "just above the current one");
            assert_eq!(st.doc.current().0, id, "and current");
            let layer = st.doc.layer(LayerId(id)).unwrap();
            assert_eq!(layer.name, "Scribble");
            assert_eq!(
                layer.control,
                Some(procraft_engine::document::Control { kind: procraft_engine::document::ControlKind::Scribble })
            );
        }
        // empty, it isn't sent with a run
        assert!(run_controls(&s, &[], IRect::from_size(64, 64)).unwrap().is_empty());
        s.undo();
        assert!(s.shared.lock().doc.layer(LayerId(id)).is_none(), "one undo takes it away");
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
        // Anima's own limit (3 MP) caps it below the 6 MP setting
        assert_eq!((quality.width, quality.height), (2048, 1456));
        assert_eq!(quality.passes, 2);
        assert_eq!((quality.from_width, quality.from_height), (2048, 1456));
        assert!(quality.capped);
        assert_eq!(quality.largest_megapixels, 3.0);
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
        assert!(u64::from(capped.width) * u64::from(capped.height) <= 3_000_000);
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
            extract: None,
        };
        let picture = Picture::new(width, height, [255u8, 0, 0, 255].repeat((width * height) as usize)).unwrap();
        let result = procraft_diffusion::RunResult { seed: 1, picture, generated: (width, height) };
        let kept = keep(1, &result, &run, None).unwrap();
        assert_eq!((kept.area.x0, kept.area.y0, kept.area.x1, kept.area.y1), (26, 26, 174, 174));
        let pixels = kept.pixels.as_deref().map(Vec::as_slice).unwrap_or_default();
        assert_eq!(pixels.first_chunk::<4>(), Some(&[255, 0, 0, 255]));
        let outside = (width as usize / 2) * 4;
        assert_eq!(pixels.get(outside..outside + 4), Some([0, 0, 0, 0].as_slice()));

        let Some(s) = session(200) else { return };
        let id = s.add_generated(kept.area, pixels, &kept.name).unwrap();
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
            pixels: Some(Arc::new([255u8, 0, 0, 255].repeat(16 * 16))),
            thumbnail: ThumbnailImage { width: 1, height: 1, rgba: vec![255, 0, 0, 255] },
            scaled_from: (0, 0),
            saved: false,
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

    /// A fresh folder in the temporary directory for one test.
    fn temp_folder(name: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("procraft-generation-{}-{name}-{n}", std::process::id()))
    }

    fn settings() -> GenerationSettings {
        GenerationSettings { address: String::new(), nsfw: false, largest_megapixels: Some(2.0) }
    }

    /// A result with every kind of alpha and premultiplied colour in it, for exact round trips.
    fn kept_result(id: u64, side: u32) -> Kept {
        let pixels: Vec<u8> = (0..side * side)
            .flat_map(|i| {
                let alpha = ((i * 29 + 11) % 256) as u8;
                let colour = |k: u32| if alpha == 0 { 0 } else { ((i * k + 3) % u32::from(alpha).max(1)) as u8 };
                [colour(17), colour(7), colour(5), alpha]
            })
            .collect();
        let picture = Picture::new(side, side, pixels.clone()).unwrap();
        Kept {
            id,
            seed: 1_000 + id,
            name: format!("Generated · result {id}"),
            area: IRect::from_size(side, side),
            pixels: Some(Arc::new(pixels)),
            thumbnail: thumbnail(&picture).unwrap(),
            scaled_from: (side / 2, side / 2),
            saved: false,
        }
    }

    /// Waits for the folder's read to finish (it is done on a thread of its own).
    fn wait_loaded(generation: &Generation) {
        for _ in 0..500 {
            let _ = generation.state();
            if generation.artwork.lock().loaded {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the results' folder was not read");
    }

    /// Waits for the background saver to write an index of `count` results.
    fn wait_index(dir: &Path, count: usize) {
        for _ in 0..500 {
            if let Ok(bytes) = fs::read(dir.join(RESULTS_INDEX))
                && let Ok(index) = serde_json::from_slice::<ResultsIndex>(&bytes)
                && index.results.len() == count
            {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the results index was not written");
    }

    fn wait_results(generation: &Generation, count: usize) -> GenerationState {
        for _ in 0..500 {
            let state = generation.state();
            if state.results.len() == count {
                return state;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the results were not read back");
    }

    /// Waits for the saver to have written every result and the artwork to let its pixels go.
    fn wait_saved(generation: &Generation) {
        for _ in 0..500 {
            let _ = generation.state();
            let all_saved = {
                let artwork = generation.artwork.lock();
                !artwork.results.is_empty() && artwork.results.iter().all(|k| k.saved && k.pixels.is_none())
            };
            if all_saved {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the results were not written and let go");
    }

    #[test]
    fn results_survive_leaving_and_a_new_generation_over_the_folder() {
        let folder = temp_folder("survive");
        let s = CanvasSession::new(16, 16);
        let generation = Generation::new(settings());
        generation.set_folder(&s, folder.to_string_lossy().into_owned());
        wait_loaded(&generation);
        let kept = kept_result(7, 8);
        let pixels = kept.pixels.as_deref().map(Vec::as_slice).unwrap_or_default().to_vec();
        let scaled_from = kept.scaled_from;
        let name = kept.name.clone();
        generation.artwork_of(&s).results.push(kept);
        generation.leave(&s);
        assert!(generation.state().results.is_empty(), "leaving lets the pixels go");
        wait_index(&folder.join(RESULTS_DIR), 1);

        let s = CanvasSession::new(16, 16);
        let generation = Generation::new(settings());
        generation.set_folder(&s, folder.to_string_lossy().into_owned());
        let state = wait_results(&generation, 1);
        assert_eq!(state.results.first().map(|r| r.id), Some(7));
        assert_eq!(state.results.first().map(|r| r.seed), Some(1_007));
        {
            let artwork = generation.artwork.lock();
            let loaded = artwork.results.first().expect("the result read back");
            assert!(loaded.pixels.is_none(), "opening reads only the thumbnail");
            assert_eq!(loaded.scaled_from, scaled_from);
            assert_eq!(loaded.name, name);
            assert_eq!(artwork.next, 8, "new results continue after the saved ids");
        }
        generation.show_result(&s, Some(7));
        {
            let artwork = generation.artwork.lock();
            let shown = artwork.results.first().expect("the result read back");
            assert_eq!(
                shown.pixels.as_deref().map(Vec::as_slice),
                Some(pixels.as_slice()),
                "the pixels round-trip exactly"
            );
        }
        drop(generation);
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn saved_results_hold_only_thumbnails_until_shown_or_applied() {
        let folder = temp_folder("memory");
        let Some(s) = session(16) else { return };
        let generation = Generation::new(settings());
        generation.set_folder(&s, folder.to_string_lossy().into_owned());
        wait_loaded(&generation);
        let mut saved = Vec::new();
        {
            let mut artwork = generation.artwork_of(&s);
            for id in 1..=3 {
                let kept = kept_result(id, 8);
                let pixels = kept.pixels.as_deref().map(Vec::as_slice).unwrap_or_default().to_vec();
                saved.push((pixels, kept.thumbnail.clone()));
                artwork.results.push(kept);
            }
        }
        generation.leave(&s);
        wait_index(&folder.join(RESULTS_DIR), 3);
        drop(generation);

        let generation = Generation::new(settings());
        generation.set_folder(&s, folder.to_string_lossy().into_owned());
        wait_results(&generation, 3);
        {
            let artwork = generation.artwork.lock();
            for kept in &artwork.results {
                assert!(kept.pixels.is_none(), "opening reads only the thumbnails");
            }
            for (kept, (_, thumbnail)) in artwork.results.iter().zip(&saved) {
                assert_eq!(&kept.thumbnail, thumbnail, "the thumbnails round-trip");
            }
        }
        generation.show_result(&s, Some(2));
        {
            let artwork = generation.artwork.lock();
            assert_eq!(artwork.shown, Some(2));
            let held: Vec<u64> = artwork.results.iter().filter(|k| k.pixels.is_some()).map(|k| k.id).collect();
            assert_eq!(held, vec![2], "only the shown result keeps its pixels");
            let shown = artwork.results.iter().find(|k| k.id == 2).expect("the shown result");
            assert_eq!(shown.pixels.as_deref().map(Vec::as_slice), Some(saved[1].0.as_slice()), "read back exactly");
        }
        generation.show_result(&s, Some(3));
        {
            let artwork = generation.artwork.lock();
            let held: Vec<u64> = artwork.results.iter().filter(|k| k.pixels.is_some()).map(|k| k.id).collect();
            assert_eq!(held, vec![3], "showing another lets the first go");
        }
        let layer = generation.apply_result(&s, 1).expect("applying a result kept on disk");
        assert!(s.shared.lock().doc.layer(LayerId(layer)).is_some());
        assert!(generation.artwork.lock().results.iter().all(|k| k.pixels.is_none()), "apply lets it go");
        drop(generation);
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn only_the_newest_twenty_four_results_are_kept() {
        let folder = temp_folder("limit");
        let s = CanvasSession::new(8, 8);
        let generation = Generation::new(settings());
        generation.set_folder(&s, folder.to_string_lossy().into_owned());
        wait_loaded(&generation);
        {
            let mut artwork = generation.artwork_of(&s);
            for id in 1..=25 {
                artwork.results.push(kept_result(id, 8));
            }
        }
        let state = generation.state();
        assert_eq!(state.results.len(), KEPT_RESULTS);
        assert_eq!(state.results.first().map(|r| r.id), Some(2), "the oldest went");
        assert_eq!(state.results.last().map(|r| r.id), Some(25));
        wait_saved(&generation);
        drop(generation);

        let dir = folder.join(RESULTS_DIR);
        assert!(!dir.join("1.png").exists(), "the oldest is deleted");
        assert!(!dir.join("1.thumb.png").exists(), "its thumbnail too");
        for id in 2..=25 {
            assert!(dir.join(format!("{id}.png")).is_file(), "result {id}");
            assert!(dir.join(format!("{id}.thumb.png")).is_file(), "result {id}'s thumbnail");
        }
        let index: ResultsIndex = serde_json::from_slice(&fs::read(dir.join(RESULTS_INDEX)).unwrap()).unwrap();
        assert_eq!(index.results.len(), KEPT_RESULTS);
        assert_eq!(index.results.first().map(|r| r.id), Some(2));
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn discarding_results_deletes_the_folder_s() {
        let folder = temp_folder("discard");
        let s = CanvasSession::new(8, 8);
        let generation = Generation::new(settings());
        generation.set_folder(&s, folder.to_string_lossy().into_owned());
        wait_loaded(&generation);
        generation.artwork_of(&s).results.push(kept_result(3, 8));
        generation.save_results(&folder, &generation.artwork_of(&s));
        let dir = folder.join(RESULTS_DIR);
        wait_index(&dir, 1);
        assert!(dir.join("3.png").is_file());
        assert!(dir.join("3.thumb.png").is_file());
        generation.discard_results(&s);
        drop(generation);
        assert!(!dir.exists(), "the results and their index go");
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn deleting_a_result_takes_it_from_the_bar_and_the_folder() {
        let folder = temp_folder("delete");
        let s = CanvasSession::new(8, 8);
        let generation = Generation::new(settings());
        generation.set_folder(&s, folder.to_string_lossy().into_owned());
        wait_loaded(&generation);
        generation.artwork_of(&s).results.push(kept_result(3, 8));
        generation.artwork_of(&s).results.push(kept_result(4, 8));
        generation.save_results(&folder, &generation.artwork_of(&s));
        let dir = folder.join(RESULTS_DIR);
        wait_index(&dir, 2);
        generation.show_result(&s, Some(3));
        generation.delete_result(&s, 3);
        let state = generation.state();
        assert_eq!(state.results.iter().map(|r| r.id).collect::<Vec<_>>(), vec![4]);
        assert_eq!(state.shown, None, "the one shown is hidden");
        wait_index(&dir, 1);
        drop(generation);
        assert!(!dir.join("3.png").exists() && !dir.join("3.thumb.png").exists(), "its files go");
        assert!(dir.join("4.png").is_file(), "the others stay");
        let _ = fs::remove_dir_all(&folder);
    }

    #[test]
    fn an_index_survives_a_write_cut_short_before_its_rename() {
        let folder = temp_folder("interrupted");
        let dir = folder.join(RESULTS_DIR);
        fs::create_dir_all(&dir).unwrap();
        let side = 8;
        let kept = kept_result(7, side);
        let pixels = kept.pixels.as_deref().map(Vec::as_slice).unwrap_or_default();
        let index = ResultsIndex {
            version: RESULTS_VERSION,
            results: vec![SavedResult {
                id: 7,
                seed: kept.seed,
                name: kept.name.clone(),
                area: [0, 0, side as i32, side as i32],
                scaled_from: [side / 2, side / 2],
                thumbnail: [kept.thumbnail.width, kept.thumbnail.height],
            }],
        };
        fs::write(dir.join(RESULTS_INDEX), serde_json::to_vec(&index).unwrap()).unwrap();
        crate::project::write_png(&dir.join("7.png"), side, side, pixels, &[]).unwrap();
        crate::project::write_png(
            &dir.join("7.thumb.png"),
            kept.thumbnail.width,
            kept.thumbnail.height,
            &kept.thumbnail.rgba,
            &[],
        )
        .unwrap();
        // a save cut short after writing its new index beside the old one, before the rename
        fs::write(dir.join("index.1.tmp"), b"{ not an index").unwrap();
        // and a picture it wrote before it stopped: the old index still rules, its id is not reused
        crate::project::write_png(&dir.join("9.png"), side, side, pixels, &[]).unwrap();
        let found = load_results(&folder);
        assert_eq!(found.results.len(), 1);
        assert_eq!(found.results.first().map(|k| k.id), Some(7));
        assert_eq!(found.results.first().map(|k| k.thumbnail.clone()), Some(kept.thumbnail.clone()));
        assert_eq!(found.results.first().map(|k| k.pixels.is_none()), Some(true), "only the thumbnail is read");
        assert_eq!(found.next, 10, "past the picture the cut-short save wrote");
        let _ = fs::remove_dir_all(&folder);
    }
}
