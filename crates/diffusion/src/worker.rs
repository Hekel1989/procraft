//! The worker: one thread that keeps in touch with a server and runs requests on it, one at a time,
//! while the app carries on. The app hands it requests and reads snapshots of how they're going
//! (it polls, as it does an export's progress); nothing calls back into the app.
//!
//! A request for several pictures runs as one graph per picture, seeds one apart, so each comes
//! back as soon as it's made and each can be made again from its seed.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::{Condvar, Mutex};
use serde_json::json;

use crate::comfy::{Comfy, Event, Socket, SystemStats};
use crate::plan::{largest_for_vram, plan_for};
use crate::requirements::{Feature, Readiness, all_nodes};
use crate::{Address, Error, Family, Graph, Picture, Request, ServerModels, Style, styles_for};

/// Sampling previews are kept at most this big.
const PREVIEW_SIDE: u32 = 512;

/// A run's number, unique for the worker's life.
pub type RunId = u64;

/// How things stand with the server.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Connection {
    /// No address yet.
    #[default]
    NotSet,
    Connecting,
    Connected,
    /// The last try failed.
    Failed(Error),
}

/// The server as last found: whether it's reachable, what it is and has, and what that's enough for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerState {
    pub address: Option<Address>,
    pub connection: Connection,
    pub stats: Option<SystemStats>,
    pub models: Option<ServerModels>,
    /// Per family, the features it's ready for.
    pub readiness: Vec<(Family, Readiness)>,
}

impl ServerState {
    /// Whether `family` can do `feature` here.
    pub fn ready(&self, family: Family, feature: Feature) -> bool {
        self.connection == Connection::Connected && self.readiness.iter().any(|(f, r)| *f == family && r.ready(feature))
    }

    /// The styles the server can run.
    pub fn styles(&self) -> Vec<Style> {
        self.models.as_ref().map(styles_for).unwrap_or_default()
    }
}

/// A picture a run made, at the request's area and the size it was made at before the final
/// scale there.
#[derive(Clone, Debug, PartialEq)]
pub struct RunResult {
    pub seed: u64,
    pub picture: Picture,
    /// The plan's final pass size.
    pub generated: (u32, u32),
}

/// How a run is going.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunState {
    pub id: RunId,
    /// How many pictures it makes.
    pub count: u32,
    /// How far it is, 0 to 1, over all its pictures.
    pub progress: f32,
    /// The latest sampling preview, if the server sends them.
    pub preview: Option<Picture>,
    /// How many previews have come, to tell a new one from the last.
    pub previews: u32,
    /// The pictures made so far.
    pub results: Vec<RunResult>,
    /// Why it stopped early, if it did.
    pub error: Option<Error>,
    /// It's over (done, failed or cancelled).
    pub finished: bool,
}

#[derive(Default)]
struct Inner {
    server: ServerState,
    connect: bool,
    queue: VecDeque<(RunId, Request)>,
    runs: HashMap<RunId, RunState>,
    cancelled: HashSet<RunId>,
    next: RunId,
    stop: bool,
}

struct Shared {
    inner: Mutex<Inner>,
    wake: Condvar,
}

/// Runs requests on a server on a thread of its own; dropping it stops the thread.
pub struct Worker {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Default for Worker {
    fn default() -> Worker {
        Worker::new()
    }
}

impl Worker {
    pub fn new() -> Worker {
        let shared =
            Arc::new(Shared { inner: Mutex::new(Inner { next: 1, ..Inner::default() }), wake: Condvar::new() });
        let theirs = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("procraft-diffusion".to_owned())
            .spawn(move || work(&theirs))
            .map_err(|e| log::error!("no generation thread: {e}"))
            .ok();
        Worker { shared, thread }
    }

    /// Uses the server at `address` (none: no server), and connects to it.
    pub fn set_address(&self, address: Option<Address>) {
        let mut inner = self.shared.inner.lock();
        if inner.server.address == address {
            return;
        }
        inner.server = ServerState {
            connection: if address.is_some() { Connection::Connecting } else { Connection::NotSet },
            address,
            ..ServerState::default()
        };
        inner.connect = inner.server.address.is_some();
        self.shared.wake.notify_all();
    }

    /// Connects again (after a failure, or to see new models).
    pub fn connect(&self) {
        let mut inner = self.shared.inner.lock();
        if inner.server.address.is_some() {
            inner.server.connection = Connection::Connecting;
            inner.connect = true;
            self.shared.wake.notify_all();
        }
    }

    pub fn server(&self) -> ServerState {
        self.shared.inner.lock().server.clone()
    }

    /// Queues `request`; its run's id.
    pub fn submit(&self, request: Request) -> RunId {
        let mut inner = self.shared.inner.lock();
        let id = inner.next;
        inner.next += 1;
        inner.runs.insert(id, RunState { id, count: request.count.max(1), ..RunState::default() });
        inner.queue.push_back((id, request));
        self.shared.wake.notify_all();
        id
    }

    /// Stops run `id`, queued or running; the pictures it made so far are kept.
    pub fn cancel(&self, id: RunId) {
        let mut inner = self.shared.inner.lock();
        if let Some(at) = inner.queue.iter().position(|(r, _)| *r == id) {
            inner.queue.remove(at);
            finish(&mut inner, id, Some(Error::Cancelled));
        } else if inner.runs.get(&id).is_some_and(|r| !r.finished) {
            inner.cancelled.insert(id);
        }
    }

    /// How run `id` is going.
    pub fn run(&self, id: RunId) -> Option<RunState> {
        self.shared.inner.lock().runs.get(&id).cloned()
    }

    /// Forgets run `id` and its pictures (cancelling it if it's still going).
    pub fn forget(&self, id: RunId) {
        self.cancel(id);
        self.shared.inner.lock().runs.remove(&id);
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.shared.inner.lock().stop = true;
        self.shared.wake.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn finish(inner: &mut Inner, id: RunId, error: Option<Error>) {
    if let Some(run) = inner.runs.get_mut(&id) {
        run.finished = true;
        run.preview = None;
        run.error = error;
        if run.error.is_none() {
            run.progress = 1.0;
        }
    }
    inner.cancelled.remove(&id);
}

/// What the thread does next.
enum Job {
    Connect(Address),
    Run(RunId, Box<Request>),
}

/// The thread: connects when asked, runs what's queued, sleeps otherwise.
fn work(shared: &Shared) {
    let client_id = format!(
        "procraft-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
    );
    // the server's API and WebSocket, kept between runs (none until connected, or after it drops)
    let mut comfy: Option<Comfy> = None;
    let mut socket: Option<Socket> = None;
    let mut recent = Recent::default();
    loop {
        let job = {
            let mut inner = shared.inner.lock();
            loop {
                if inner.stop {
                    return;
                }
                if inner.connect {
                    inner.connect = false;
                    if let Some(address) = inner.server.address.clone() {
                        break Job::Connect(address);
                    }
                }
                if let Some((id, request)) = inner.queue.pop_front() {
                    break Job::Run(id, Box::new(request));
                }
                shared.wake.wait(&mut inner);
            }
        };
        match job {
            Job::Connect(address) => {
                socket = None;
                let api = Comfy::new(address.clone());
                let found = connect(&api);
                let mut inner = shared.inner.lock();
                // the address may have changed meanwhile: then this answer is stale
                if inner.server.address.as_ref() != Some(&address) {
                    continue;
                }
                match found {
                    Ok((stats, models)) => {
                        let readiness = Family::ALL.iter().map(|&f| (f, Readiness::check(f, &models))).collect();
                        inner.server.connection = Connection::Connected;
                        inner.server.stats = Some(stats);
                        inner.server.models = Some(models);
                        inner.server.readiness = readiness;
                        comfy = Some(api);
                    }
                    Err(e) => {
                        inner.server.connection = Connection::Failed(e);
                        comfy = None;
                    }
                }
            }
            Job::Run(id, request) => {
                let models = shared.inner.lock().server.models.clone();
                let outcome = match (&comfy, models) {
                    (Some(api), Some(models)) => {
                        let on = Running { shared, api, client_id: &client_id, id };
                        run(&on, &mut socket, &mut recent, &request, &models)
                    }
                    _ => {
                        Err(Error::Unreachable { address: request_address(shared), reason: "not connected".to_owned() })
                    }
                };
                if matches!(outcome, Err(Error::Lost { .. } | Error::Unreachable { .. })) {
                    socket = None;
                    let mut inner = shared.inner.lock();
                    if let Err(e) = &outcome {
                        inner.server.connection = Connection::Failed(e.clone());
                    }
                }
                finish(&mut shared.inner.lock(), id, outcome.err());
            }
        }
    }
}

fn request_address(shared: &Shared) -> String {
    shared.inner.lock().server.address.as_ref().map(ToString::to_string).unwrap_or_default()
}

/// Finds out what the server is and has.
fn connect(api: &Comfy) -> Result<(SystemStats, ServerModels), Error> {
    let stats = api.system_stats()?;
    let models = api.models(&all_nodes())?;
    Ok((stats, models))
}

/// How many of the latest pictures the worker remembers, for runs the server answers from its
/// cache (an identical graph: ComfyUI doesn't run it again, so sends no picture).
const REMEMBERED: usize = 8;

/// The latest pictures received, by their graph.
#[derive(Default)]
struct Recent(VecDeque<(u64, Vec<u8>)>);

impl Recent {
    fn key(graph: &Graph) -> u64 {
        let mut hasher = std::hash::DefaultHasher::new();
        graph.to_json().to_string().hash(&mut hasher);
        hasher.finish()
    }

    fn find(&self, graph: &Graph) -> Option<Vec<u8>> {
        let key = Recent::key(graph);
        self.0.iter().find(|(k, _)| *k == key).map(|(_, bytes)| bytes.clone())
    }

    fn remember(&mut self, graph: &Graph, bytes: &[u8]) {
        let key = Recent::key(graph);
        self.0.retain(|(k, _)| *k != key);
        self.0.push_front((key, bytes.to_vec()));
        self.0.truncate(REMEMBERED);
    }
}

/// The server and the run, as the steps of a run need them.
struct Running<'a> {
    shared: &'a Shared,
    api: &'a Comfy,
    client_id: &'a str,
    id: RunId,
}

/// Runs `request`: one graph per picture, each followed over the WebSocket until it's done.
fn run(
    on: &Running<'_>,
    socket: &mut Option<Socket>,
    recent: &mut Recent,
    request: &Request,
    models: &ServerModels,
) -> Result<(), Error> {
    let family = request.style.family;
    let feature = if request.selection.is_some() {
        Feature::Selections
    } else if request.is_new() {
        Feature::Generate
    } else {
        Feature::Refine
    };
    let missing = Readiness::check(family, models).missing(feature).to_vec();
    if !missing.is_empty() {
        return Err(Error::Missing(missing));
    }
    let (width, height) = request.size;
    let vram = on.shared.inner.lock().server.stats.as_ref().and_then(|s| s.gpu.as_ref()).map_or(0, |(_, bytes)| *bytes);
    let largest = request.largest.unwrap_or_else(|| largest_for_vram(vram));
    // Selections are one pass at the target: a second pass would have to repeat the whole inpaint
    // structure. The refine plan is exactly one pass at the target, never below the model's size.
    let one_pass = !request.is_new() || request.selection.is_some();
    let plan = plan_for(width, height, family.native_pixels(), request.resolution, largest, one_pass);
    let weights = pass_weights(&plan, request.style.steps, family);
    let count = request.count.max(1);
    for i in 0..count {
        if on.shared.inner.lock().cancelled.contains(&on.id) {
            return Err(Error::Cancelled);
        }
        let seed = request.seed.wrapping_add(u64::from(i));
        let graph = family.graph(request, plan, seed, models)?;
        let part = |done: f64| (f64::from(i) + done) / f64::from(count);
        let bytes = match follow(on, socket, &graph, part, &weights)? {
            Some(bytes) => bytes,
            // an identical graph ran before: the picture it made, else the same graph sending
            // as JPEG (only the sending node runs again)
            None => match recent.find(&graph) {
                Some(bytes) => bytes,
                None => {
                    let mut again = graph.clone();
                    for node in again.nodes_of("ETN_SendImageWebSocket") {
                        again.set(&node, "format", json!("JPEG"));
                    }
                    follow(on, socket, &again, part, &weights)?
                        .ok_or_else(|| Error::protocol("the server finished without sending a picture"))?
                }
            },
        };
        recent.remember(&graph, &bytes);
        let picture = Picture::decode(&bytes)?.resized(width, height)?;
        if let Some(run) = on.shared.inner.lock().runs.get_mut(&on.id) {
            let generated = (plan.size.width, plan.size.height);
            run.results.push(crate::RunResult { seed, picture, generated });
            run.progress = part(1.0) as f32;
            run.preview = None;
        }
    }
    Ok(())
}

/// How much of a graph's progress each sampler covers. ComfyUI runs a denoised sampler for
/// `steps × denoise` steps, rounded up.
fn pass_weights(plan: &crate::Plan, steps: u32, family: Family) -> Vec<f64> {
    if plan.second.is_none() {
        return vec![1.0];
    }
    let first = f64::from(steps.max(1));
    let second = (first * family.second_pass_denoise()).ceil().max(1.0);
    vec![first / (first + second), second / (first + second)]
}

/// Queues `graph` and follows it to its end: the picture its sending node sent (PNG or JPEG
/// file), none if it sent none. `part` turns how far the graph is into how far the run is, and
/// `weights` how much each of its samplers is worth.
fn follow(
    on: &Running<'_>,
    socket: &mut Option<Socket>,
    graph: &Graph,
    part: impl Fn(f64) -> f64,
    weights: &[f64],
) -> Result<Option<Vec<u8>>, Error> {
    if socket.is_none() {
        *socket = Some(Socket::connect(on.api.address(), on.client_id)?);
    }
    let Some(ws) = socket.as_mut() else { return Ok(None) };
    let sender = graph.nodes_of("ETN_SendImageWebSocket");
    let samplers = graph.nodes_of("KSampler");
    let prompt_id = on.api.queue(graph, on.client_id)?;
    let mut running: Option<String> = None;
    let mut pass = 0usize;
    let mut made = None;
    loop {
        if on.shared.inner.lock().cancelled.contains(&on.id) {
            // stop it whether it's queued or running (the interrupt only stops ours: the server
            // runs one graph at a time and ours is next or now)
            let _ = on.api.unqueue(&prompt_id);
            let _ = on.api.interrupt();
            return Err(Error::Cancelled);
        }
        let Some(event) = ws.next_event()? else { continue };
        match event {
            Event::Executing { prompt_id: p, node } if p == prompt_id => match node {
                Some(node) => {
                    if let Some(at) = samplers.iter().position(|s| *s == node) {
                        pass = at;
                    }
                    running = Some(node);
                }
                None => return Ok(made),
            },
            Event::Progress { prompt_id: p, value, max } if p == prompt_id && max > 0 => {
                let before: f64 = weights.iter().take(pass).sum();
                let share = weights.get(pass).copied().unwrap_or(0.0);
                let done = (before + share * f64::from(value) / f64::from(max)).clamp(0.0, 1.0);
                if let Some(run) = on.shared.inner.lock().runs.get_mut(&on.id) {
                    run.progress = part(done) as f32;
                }
            }
            // what the sending node sends is the picture; anything else, a sampling preview
            Event::Picture { bytes, .. } if running.as_ref().is_some_and(|n| sender.contains(n)) => made = Some(bytes),
            Event::Picture { bytes, .. } => {
                if let Ok(preview) = Picture::decode(&bytes).and_then(|p| p.thumbnail(PREVIEW_SIDE))
                    && let Some(run) = on.shared.inner.lock().runs.get_mut(&on.id)
                {
                    run.preview = Some(preview);
                    run.previews += 1;
                }
            }
            Event::Succeeded { prompt_id: p } if p == prompt_id => return Ok(made),
            Event::Failed { prompt_id: p, kind, message } if p == prompt_id => {
                let oom = kind.contains("OutOfMemory") || message.to_lowercase().contains("out of memory");
                return Err(if oom { Error::OutOfMemory } else { Error::Server { message } });
            }
            Event::Interrupted { prompt_id: p } if p == prompt_id => return Err(Error::Cancelled),
            _ => {}
        }
    }
}
