use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceKind {
    Input,
    Output,
    Loopback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    pub channels: u16,
    pub sample_rate: u32,
    pub channel_names: Vec<String>,
    pub max_input_channels: u16,
    pub max_output_channels: u16,
    pub supported_sample_rates: Vec<u32>,
    pub supported_sample_formats: Vec<String>,
}
