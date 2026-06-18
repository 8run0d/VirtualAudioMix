use serde::{Deserialize, Serialize};

use super::node::{NodeId, PortId};

pub type EdgeId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioEdge {
    pub id: EdgeId,
    pub from_node: NodeId,
    pub from_port: PortId,
    pub to_node: NodeId,
    pub to_port: PortId,
    pub gain: f32,
    pub level: f32,
}
