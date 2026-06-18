use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::edge::{AudioEdge, EdgeId};
use super::node::{AudioNode, NodeId};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioGraph {
    pub nodes: HashMap<NodeId, AudioNode>,
    pub edges: HashMap<EdgeId, AudioEdge>,
}

impl AudioGraph {
    pub fn add_node(&mut self, node: AudioNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn add_edge(&mut self, edge: AudioEdge) {
        self.edges.insert(edge.id.clone(), edge);
    }

    pub fn remove_node(&mut self, node_id: &str) {
        self.nodes.remove(node_id);
        self.edges
            .retain(|_, edge| edge.from_node != node_id && edge.to_node != node_id);
    }
}
