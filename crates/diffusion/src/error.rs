//! What can go wrong between asking for a picture and getting it, each case worded for the person
//! who asked.

use crate::requirements::Missing;

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum Error {
    /// The server didn't answer (wrong address, not running, network down).
    #[error("can't reach {address}: {reason}")]
    Unreachable { address: String, reason: String },
    /// The connection dropped while a run was going.
    #[error("lost the server during the run: {reason}")]
    Lost { reason: String },
    /// The server answered, but not as ComfyUI does.
    #[error("the server's answer wasn't understood: {reason}")]
    Protocol { reason: String },
    /// The server is missing what the request needs.
    #[error("the server is missing {}", list(.0))]
    Missing(Vec<Missing>),
    /// The GPU ran out of memory running the graph.
    #[error("the server ran out of GPU memory")]
    OutOfMemory,
    /// ComfyUI ran the graph and failed: its own message.
    #[error("the server failed: {message}")]
    Server { message: String },
    /// A picture couldn't be read or written.
    #[error("picture: {reason}")]
    Picture { reason: String },
    /// The request can't be run as asked (an empty prompt where one is needed, say).
    #[error("{reason}")]
    Request { reason: String },
    /// The run was cancelled.
    #[error("cancelled")]
    Cancelled,
}

fn list(missing: &[Missing]) -> String {
    missing.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
}

impl Error {
    pub(crate) fn protocol(reason: impl std::fmt::Display) -> Error {
        Error::Protocol { reason: reason.to_string() }
    }

    pub(crate) fn picture(reason: impl std::fmt::Display) -> Error {
        Error::Picture { reason: reason.to_string() }
    }
}
