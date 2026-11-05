//! Image generation for ProCraft, run on a ComfyUI server: the app describes what it wants as a
//! [`Request`] (a prompt, a style, how strongly to change the picture it sends), a [`Family`] of
//! models turns it into a ComfyUI [`Graph`], and the [`Worker`] runs it on the server off the
//! caller's thread and keeps what comes back.
//!
//! Nothing here knows about the canvas or draws anything: pictures come in and go out as straight
//! RGBA ([`Picture`]). `procraft-core` takes them from the canvas and puts the results back.
//!
//! The server is reached over plain HTTP and a WebSocket ([`comfy`]), with blocking calls on the
//! worker's own thread. Pictures go to the server inside the graph and come back over the
//! WebSocket, so nothing is left in the server's folders. The add-ons and model files the server
//! needs, and which features each makes work, are in [`requirements`]; the companion that sets up
//! servers uses the same list.

pub mod comfy;
mod error;
mod family;
mod graph;
mod picture;
pub mod plan;
mod request;
pub mod requirements;
mod styles;
mod worker;

pub use comfy::{Address, Comfy, ServerModels, SystemStats};
pub use error::Error;
pub use family::Family;
pub use graph::{Graph, Node, Out};
pub use picture::Picture;
pub use plan::{Plan, Resolution, Size, largest_for_vram, plan, plan_for};
pub use request::Request;
pub use requirements::{Feature, Missing, Readiness};
pub use styles::{Style, styles_for};
pub use worker::{Connection, RunId, RunResult, RunState, ServerState, Worker};
