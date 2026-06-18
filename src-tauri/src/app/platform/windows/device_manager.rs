use std::collections::BTreeSet;

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleFormat, SupportedStreamConfigRange};

use crate::app::core::types::device::{DeviceInfo, DeviceKind};

pub fn list_audio_devices() -> Result<Vec<DeviceInfo>, String> {
    let host = cpal::default_host();
    let mut devices = Vec::new();

    for (index, device) in host
        .input_devices()
        .map_err(|error| error.to_string())?
        .enumerate()
    {
        devices.push(read_device(index, device, DeviceKind::Input));
    }

    let offset = devices.len();
    for (index, device) in host
        .output_devices()
        .map_err(|error| error.to_string())?
        .enumerate()
    {
        devices.push(read_device(offset + index, device, DeviceKind::Output));
    }

    Ok(devices)
}

fn read_device(index: usize, device: cpal::Device, kind: DeviceKind) -> DeviceInfo {
    let config = device
        .default_input_config()
        .or_else(|_| device.default_output_config())
        .ok();
    let input_configs = device
        .supported_input_configs()
        .map(|configs| configs.collect::<Vec<_>>())
        .unwrap_or_default();
    let output_configs = device
        .supported_output_configs()
        .map(|configs| configs.collect::<Vec<_>>())
        .unwrap_or_default();
    let max_input_channels = max_channels(&input_configs);
    let max_output_channels = max_channels(&output_configs);
    let channels = config
        .as_ref()
        .map(|config| config.channels())
        .unwrap_or_else(|| {
            match kind {
                DeviceKind::Input => max_input_channels,
                DeviceKind::Output | DeviceKind::Loopback => max_output_channels,
            }
            .max(1)
        });
    let supported_sample_rates = supported_sample_rates(&input_configs, &output_configs);
    let supported_sample_formats = supported_sample_formats(&input_configs, &output_configs);
    let channel_names = channel_names(channels, &kind);

    DeviceInfo {
        id: format!("device-{index}"),
        name: device
            .name()
            .unwrap_or_else(|_| "Unknown audio device".to_string()),
        kind,
        channels,
        sample_rate: config
            .as_ref()
            .map(|config| config.sample_rate().0)
            .unwrap_or(48_000),
        channel_names,
        max_input_channels,
        max_output_channels,
        supported_sample_rates,
        supported_sample_formats,
    }
}

fn max_channels(configs: &[SupportedStreamConfigRange]) -> u16 {
    configs
        .iter()
        .map(SupportedStreamConfigRange::channels)
        .max()
        .unwrap_or(0)
}

fn supported_sample_rates(
    input_configs: &[SupportedStreamConfigRange],
    output_configs: &[SupportedStreamConfigRange],
) -> Vec<u32> {
    let mut rates = BTreeSet::new();
    for config in input_configs.iter().chain(output_configs.iter()) {
        rates.insert(config.min_sample_rate().0);
        rates.insert(config.max_sample_rate().0);
    }
    rates.into_iter().collect()
}

fn supported_sample_formats(
    input_configs: &[SupportedStreamConfigRange],
    output_configs: &[SupportedStreamConfigRange],
) -> Vec<String> {
    let mut formats = BTreeSet::new();
    for config in input_configs.iter().chain(output_configs.iter()) {
        formats.insert(sample_format_label(config.sample_format()).to_string());
    }
    formats.into_iter().collect()
}

fn sample_format_label(format: SampleFormat) -> &'static str {
    match format {
        SampleFormat::F32 => "f32",
        SampleFormat::I16 => "i16",
        SampleFormat::U16 => "u16",
        SampleFormat::I8 => "i8",
        SampleFormat::I32 => "i32",
        SampleFormat::I64 => "i64",
        SampleFormat::U8 => "u8",
        SampleFormat::U32 => "u32",
        SampleFormat::U64 => "u64",
        _ => "unknown",
    }
}

fn channel_names(channels: u16, kind: &DeviceKind) -> Vec<String> {
    if matches!(kind, DeviceKind::Input) {
        return match channels {
            0 => Vec::new(),
            1 => vec!["Mono".to_string()],
            channel_count => (1..=channel_count)
                .map(|channel| format!("Canal {channel}"))
                .collect(),
        };
    }

    match channels {
        0 => Vec::new(),
        1 => vec!["Mono".to_string()],
        2 => vec!["L".to_string(), "R".to_string()],
        6 => ["L", "R", "C", "LFE", "Ls", "Rs"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        8 => ["L", "R", "C", "LFE", "Ls", "Rs", "Lb", "Rb"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        channel_count => (1..=channel_count)
            .map(|channel| format!("Canal {channel}"))
            .collect(),
    }
}
