//! Talking to a ComfyUI server: its HTTP API for questions and for queueing graphs, and its
//! WebSocket for what happens while they run (progress, previews, the pictures made).
//!
//! All calls block; the worker makes them on its own thread. Pictures come back over the WebSocket
//! as binary frames: a 4-byte event (1, a picture), a 4-byte format (1 JPEG, 2 PNG), the file.
//! Sampling previews arrive the same way (JPEG); the pictures a graph makes are the PNGs sent while
//! its `ETN_SendImageWebSocket` node runs.

use std::io::{ErrorKind, Read};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde_json::{Value, json};
use tungstenite::{Message, WebSocket};

use crate::{Error, Graph};

/// ComfyUI's own port, assumed when an address has none.
pub const DEFAULT_PORT: u16 = 8188;
/// How long a question to the server may take before it counts as unreachable.
const ASK_TIMEOUT: Duration = Duration::from_secs(10);
/// Queueing sends the pictures inside the graph: give a large one time over Wi-Fi.
const QUEUE_TIMEOUT: Duration = Duration::from_secs(60);
/// How long [`Socket::next_event`] waits for an event before saying there was none.
const EVENT_WAIT: Duration = Duration::from_millis(200);

/// Where a server is: a host and a port.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Address {
    pub host: String,
    pub port: u16,
}

impl Address {
    /// The address in `text` as people type it: `192.168.1.10:8189`, `http://pc.local:8188/`,
    /// `localhost` (port 8188). None if there's no host or the port isn't a number.
    pub fn parse(text: &str) -> Option<Address> {
        let text = text.trim();
        let text = text.strip_prefix("http://").unwrap_or(text);
        let text = text.split('/').next().unwrap_or_default();
        let (host, port) = match text.rsplit_once(':') {
            Some((host, port)) => (host, port.parse().ok()?),
            None => (text, DEFAULT_PORT),
        };
        (!host.is_empty() && !host.contains(char::is_whitespace)).then(|| Address { host: host.to_owned(), port })
    }

    fn http(&self, path: &str) -> String {
        format!("http://{}:{}{path}", self.host, self.port)
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

/// What a server says about itself.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemStats {
    pub comfyui_version: String,
    /// The first GPU's name and memory (bytes), if it has one.
    pub gpu: Option<(String, u64)>,
}

/// The model files a server offers, by the folder they're in, as its loaders list them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerModels {
    /// `models/diffusion_models` (`UNETLoader`).
    pub diffusion_models: Vec<String>,
    /// `models/text_encoders` (`CLIPLoader`).
    pub text_encoders: Vec<String>,
    /// `models/vae` (`VAELoader`).
    pub vaes: Vec<String>,
    /// `models/controlnet` as the tooling add-on's control loader lists it (empty without it).
    pub controls: Vec<String>,
    /// The nodes asked about that the server has.
    pub nodes: Vec<String>,
}

impl ServerModels {
    pub fn has_node(&self, node: &str) -> bool {
        self.nodes.iter().any(|n| n == node)
    }
}

/// A server's HTTP API.
#[derive(Clone, Debug)]
pub struct Comfy {
    address: Address,
    agent: ureq::Agent,
    queue_agent: ureq::Agent,
}

impl Comfy {
    pub fn new(address: Address) -> Comfy {
        let agent = |timeout| {
            ureq::Agent::config_builder().timeout_global(Some(timeout)).http_status_as_error(false).build().new_agent()
        };
        Comfy { address, agent: agent(ASK_TIMEOUT), queue_agent: agent(QUEUE_TIMEOUT) }
    }

    pub fn address(&self) -> &Address {
        &self.address
    }

    /// The server's version and GPU; [`Error::Unreachable`] if it doesn't answer as ComfyUI.
    pub fn system_stats(&self) -> Result<SystemStats, Error> {
        let stats = self.get("/system_stats")?;
        let version = stats.pointer("/system/comfyui_version").and_then(Value::as_str);
        let Some(version) = version else {
            return Err(self.unreachable("it doesn't answer as ComfyUI"));
        };
        let gpu = stats
            .pointer("/devices/0")
            .and_then(|d| Some((d.get("name")?.as_str()?.to_owned(), d.get("vram_total")?.as_u64().unwrap_or(0))));
        Ok(SystemStats { comfyui_version: version.to_owned(), gpu })
    }

    /// What the server knows of node `class`; none if it hasn't got it.
    pub fn node_info(&self, class: &str) -> Result<Option<Value>, Error> {
        let info = self.get(&format!("/object_info/{class}"))?;
        Ok(info.get(class).cloned())
    }

    /// The model files the server offers, and which of `nodes` it has.
    pub fn models(&self, nodes: &[&str]) -> Result<ServerModels, Error> {
        let choices = |class: &str, input: &str| -> Result<Vec<String>, Error> {
            Ok(self.node_info(class)?.map(|info| input_choices(&info, input)).unwrap_or_default())
        };
        let mut present = Vec::new();
        for &node in nodes {
            if self.node_info(node)?.is_some() {
                present.push(node.to_owned());
            }
        }
        Ok(ServerModels {
            diffusion_models: choices("UNETLoader", "unet_name")?,
            text_encoders: choices("CLIPLoader", "clip_name")?,
            vaes: choices("VAELoader", "vae_name")?,
            controls: choices("ETN_control_load", "weights")?,
            nodes: present,
        })
    }

    /// Queues `graph` for the WebSocket client `client_id`; returns the run's prompt id.
    pub fn queue(&self, graph: &Graph, client_id: &str) -> Result<String, Error> {
        let body = json!({ "prompt": graph.to_json(), "client_id": client_id });
        let answer = self.send(&self.queue_agent, "/prompt", &body)?;
        if let Some(id) = answer.get("prompt_id").and_then(Value::as_str) {
            return Ok(id.to_owned());
        }
        Err(Error::Server { message: queue_error(&answer) })
    }

    /// Stops the graph running now (whoever queued it).
    pub fn interrupt(&self) -> Result<(), Error> {
        self.send(&self.agent, "/interrupt", &json!({})).map(drop)
    }

    /// Takes the queued run `prompt_id` off the queue (no matter if it has started or finished).
    pub fn unqueue(&self, prompt_id: &str) -> Result<(), Error> {
        self.send(&self.agent, "/queue", &json!({ "delete": [prompt_id] })).map(drop)
    }

    fn get(&self, path: &str) -> Result<Value, Error> {
        let answer = self.agent.get(self.address.http(path)).call().map_err(|e| self.unreachable(e))?;
        read_json(answer)
    }

    fn send(&self, agent: &ureq::Agent, path: &str, body: &Value) -> Result<Value, Error> {
        let answer = agent
            .post(self.address.http(path))
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .map_err(|e| self.unreachable(e))?;
        read_json(answer)
    }

    fn unreachable(&self, reason: impl std::fmt::Display) -> Error {
        Error::Unreachable { address: self.address.to_string(), reason: reason.to_string() }
    }
}

/// The answer's body as JSON (an empty body as null).
fn read_json(mut answer: ureq::http::Response<ureq::Body>) -> Result<Value, Error> {
    let mut text = String::new();
    answer.body_mut().as_reader().read_to_string(&mut text).map_err(Error::protocol)?;
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|e| Error::protocol(format!("{e} (status {})", answer.status())))
}

/// The choices a loader's input offers: `[[a, b, ...]]` in older servers, `["COMBO", {"options":
/// [...]}]` in newer ones.
fn input_choices(info: &Value, input: &str) -> Vec<String> {
    let Some(spec) = info.pointer(&format!("/input/required/{input}")) else { return Vec::new() };
    let list = match spec.get(0) {
        Some(Value::Array(list)) => Some(list),
        Some(Value::String(kind)) if kind == "COMBO" => spec.pointer("/1/options").and_then(Value::as_array),
        _ => None,
    };
    list.map(|l| l.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

/// Why the server wouldn't queue a graph, from its answer: its message, and each node's errors.
fn queue_error(answer: &Value) -> String {
    let mut message =
        answer.pointer("/error/message").and_then(Value::as_str).unwrap_or("it wouldn't take the graph").to_owned();
    if let Some(nodes) = answer.get("node_errors").and_then(Value::as_object) {
        for (node, errors) in nodes {
            let class = errors.get("class_type").and_then(Value::as_str).unwrap_or("?");
            for e in errors.get("errors").and_then(Value::as_array).into_iter().flatten() {
                let detail = e.get("details").and_then(Value::as_str).unwrap_or_default();
                let what = e.get("message").and_then(Value::as_str).unwrap_or_default();
                message.push_str(&format!("; node {node} ({class}): {what} {detail}"));
            }
        }
    }
    message
}

/// What the server tells a client while graphs run.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// The run `prompt_id` started.
    Started { prompt_id: String },
    /// Node `node` of the run is running; none when the run is done.
    Executing { prompt_id: String, node: Option<String> },
    /// The running node (a sampler) is `value` of `max` steps in.
    Progress { prompt_id: String, value: u32, max: u32 },
    /// The run finished.
    Succeeded { prompt_id: String },
    /// The run failed: the exception's type and message.
    Failed { prompt_id: String, kind: String, message: String },
    /// The run was stopped.
    Interrupted { prompt_id: String },
    /// A picture: a sampling preview (JPEG) or a picture the graph sends (PNG).
    Picture { png: bool, bytes: Vec<u8> },
}

/// A server's WebSocket, for one client id.
pub struct Socket {
    socket: WebSocket<TcpStream>,
}

impl Socket {
    /// Opens the WebSocket for `client_id`.
    pub fn connect(address: &Address, client_id: &str) -> Result<Socket, Error> {
        let unreachable = |reason: String| Error::Unreachable { address: address.to_string(), reason };
        let at = (address.host.as_str(), address.port)
            .to_socket_addrs()
            .map_err(|e| unreachable(e.to_string()))?
            .next()
            .ok_or_else(|| unreachable("no such host".to_owned()))?;
        let stream = TcpStream::connect_timeout(&at, ASK_TIMEOUT).map_err(|e| unreachable(e.to_string()))?;
        stream.set_read_timeout(Some(ASK_TIMEOUT)).map_err(|e| unreachable(e.to_string()))?;
        let url = format!("ws://{}:{}/ws?clientId={client_id}", address.host, address.port);
        let (socket, _) = tungstenite::client(url.as_str(), stream).map_err(|e| unreachable(e.to_string()))?;
        socket.get_ref().set_read_timeout(Some(EVENT_WAIT)).map_err(|e| unreachable(e.to_string()))?;
        Ok(Socket { socket })
    }

    /// The next event, waiting a moment for one; none if none came (or it was one this client
    /// doesn't follow).
    pub fn next_event(&mut self) -> Result<Option<Event>, Error> {
        match self.socket.read() {
            Ok(Message::Text(text)) => Ok(serde_json::from_str(text.as_str()).ok().and_then(|v| event(&v))),
            Ok(Message::Binary(bytes)) => Ok(picture_event(&bytes)),
            Ok(_) => Ok(None),
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                Ok(None)
            }
            Err(e) => Err(Error::Lost { reason: e.to_string() }),
        }
    }
}

/// The event in a text message, if it's one followed here.
fn event(message: &Value) -> Option<Event> {
    let data = message.get("data")?;
    let prompt_id = data.get("prompt_id").and_then(Value::as_str).unwrap_or_default().to_owned();
    let text = |key: &str| data.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();
    let number = |key: &str| data.get(key).and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok());
    Some(match message.get("type")?.as_str()? {
        "execution_start" => Event::Started { prompt_id },
        "executing" => {
            Event::Executing { prompt_id, node: data.get("node").and_then(Value::as_str).map(str::to_owned) }
        }
        "progress" => Event::Progress { prompt_id, value: number("value")?, max: number("max")? },
        "execution_success" => Event::Succeeded { prompt_id },
        "execution_error" => {
            Event::Failed { prompt_id, kind: text("exception_type"), message: text("exception_message") }
        }
        "execution_interrupted" => Event::Interrupted { prompt_id },
        _ => return None,
    })
}

/// The picture in a binary message: event 1 (a picture: format, file) or 4 (a picture with its
/// details first: their length, the details, the file).
fn picture_event(bytes: &[u8]) -> Option<Event> {
    let word = |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    match word(0)? {
        1 => Some(Event::Picture { png: word(4)? == 2, bytes: bytes.get(8..)?.to_vec() }),
        4 => {
            let details = word(4)? as usize;
            let png = bytes.get(8..8 + details).is_some_and(|d| d.windows(9).any(|w| w == b"image/png"));
            Some(Event::Picture { png, bytes: bytes.get(8 + details..)?.to_vec() })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_read_as_typed() {
        let at = |host: &str, port| Some(Address { host: host.to_owned(), port });
        assert_eq!(Address::parse("192.168.1.10:8189"), at("192.168.1.10", 8189));
        assert_eq!(Address::parse(" http://pc.local:8188/ "), at("pc.local", 8188));
        assert_eq!(Address::parse("localhost"), at("localhost", DEFAULT_PORT));
        assert_eq!(Address::parse("pc:port"), None);
        assert_eq!(Address::parse(""), None);
    }

    #[test]
    fn choices_are_read_in_both_forms() {
        let old = json!({ "input": { "required": { "unet_name": [["a.safetensors", "b.safetensors"]] } } });
        let new = json!({ "input": { "required": { "weights": ["COMBO", { "options": ["c.safetensors"] }] } } });
        assert_eq!(input_choices(&old, "unet_name"), vec!["a.safetensors", "b.safetensors"]);
        assert_eq!(input_choices(&new, "weights"), vec!["c.safetensors"]);
        assert!(input_choices(&new, "missing").is_empty());
    }

    #[test]
    fn events_are_read() {
        let progress = json!({ "type": "progress", "data": { "value": 3, "max": 30, "prompt_id": "p" } });
        assert_eq!(event(&progress), Some(Event::Progress { prompt_id: "p".into(), value: 3, max: 30 }));
        let done = json!({ "type": "executing", "data": { "node": null, "prompt_id": "p" } });
        assert_eq!(event(&done), Some(Event::Executing { prompt_id: "p".into(), node: None }));
        assert_eq!(event(&json!({ "type": "status", "data": {} })), None);

        let mut frame = vec![0, 0, 0, 1, 0, 0, 0, 2];
        frame.extend_from_slice(b"PNG!");
        assert_eq!(picture_event(&frame), Some(Event::Picture { png: true, bytes: b"PNG!".to_vec() }));
        assert_eq!(picture_event(&[0, 0, 0, 3, 1]), None);
    }

    #[test]
    fn queue_errors_name_the_nodes() {
        let answer = json!({
            "error": { "message": "Prompt outputs failed validation" },
            "node_errors": { "4": { "class_type": "UNETLoader", "errors": [
                { "message": "Value not in list", "details": "unet_name: 'x' not in [...]" } ] } }
        });
        assert_eq!(
            queue_error(&answer),
            "Prompt outputs failed validation; node 4 (UNETLoader): Value not in list unet_name: 'x' not in [...]"
        );
    }
}
