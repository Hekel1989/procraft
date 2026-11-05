//! A ComfyUI graph in its API format, built node by node: `{"1": {"class_type": ..., "inputs":
//! {...}}, ...}`. An input is a value or another node's output, written `[node id, output index]`.

use serde_json::{Map, Value, json};

/// A graph being built; [`Graph::to_json`] gives what `POST /prompt` takes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graph {
    nodes: Vec<(String, Map<String, Value>)>,
}

/// A node added to a graph (its id is its place, from 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node(usize);

/// One of a node's outputs, to feed into another node's input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Out {
    node: Node,
    slot: u32,
}

impl Node {
    /// Its output `slot` (0 for a node with one).
    pub fn out(self, slot: u32) -> Out {
        Out { node: self, slot }
    }

    /// Its id in the graph.
    pub fn id(self) -> String {
        (self.0 + 1).to_string()
    }
}

impl From<Node> for Out {
    /// A node's first output.
    fn from(node: Node) -> Out {
        node.out(0)
    }
}

impl From<Out> for Value {
    fn from(out: Out) -> Value {
        json!([out.node.id(), out.slot])
    }
}

impl Graph {
    pub fn new() -> Graph {
        Graph::default()
    }

    /// Adds a node of `class` with `inputs` (values, or other nodes' [`Out`]s).
    pub fn add<'a>(&mut self, class: &str, inputs: impl IntoIterator<Item = (&'a str, Value)>) -> Node {
        let inputs = inputs.into_iter().map(|(name, value)| (name.to_owned(), value)).collect();
        self.nodes.push((class.to_owned(), inputs));
        Node(self.nodes.len() - 1)
    }

    /// Sets input `name` of the node with id `node` to `value` (nothing if there's no such node).
    pub fn set(&mut self, node: &str, name: &str, value: Value) {
        let index = node.parse::<usize>().ok().and_then(|n| n.checked_sub(1));
        if let Some((_, inputs)) = index.and_then(|i| self.nodes.get_mut(i)) {
            inputs.insert(name.to_owned(), value);
        }
    }

    /// How many nodes it has.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The ids of the nodes of `class`.
    pub fn nodes_of(&self, class: &str) -> Vec<String> {
        self.nodes.iter().enumerate().filter(|(_, (c, _))| c == class).map(|(i, _)| Node(i).id()).collect()
    }

    /// The graph as `POST /prompt`'s `prompt`.
    pub fn to_json(&self) -> Value {
        let nodes = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, (class, inputs))| (Node(i).id(), json!({ "class_type": class, "inputs": inputs })))
            .collect::<Map<_, _>>();
        Value::Object(nodes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_are_numbered_and_wired() {
        let mut g = Graph::new();
        let a = g.add("Loader", [("name", json!("model.safetensors"))]);
        let b = g.add("Use", [("model", a.out(0).into()), ("steps", json!(8))]);
        assert_eq!(b.id(), "2");
        assert_eq!(
            g.to_json(),
            json!({
                "1": { "class_type": "Loader", "inputs": { "name": "model.safetensors" } },
                "2": { "class_type": "Use", "inputs": { "model": ["1", 0], "steps": 8 } },
            })
        );
        assert_eq!(g.nodes_of("Use"), vec!["2"]);
        g.set("2", "steps", json!(4));
        g.set("9", "steps", json!(4));
        assert_eq!(g.to_json()["2"]["inputs"]["steps"], 4);
    }
}
