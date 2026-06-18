use serde::{Deserialize, Serialize};

pub type NodeId = String;
pub type PortId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NodeKind {
    InputDevice,
    OutputDevice,
    Application,
    SystemAudio,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioPort {
    pub id: PortId,
    pub channels: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioNode {
    pub id: NodeId,
    pub kind: NodeKind,
    pub label: String,
    pub inputs: Vec<AudioPort>,
    pub outputs: Vec<AudioPort>,
    pub level: f32,
}
