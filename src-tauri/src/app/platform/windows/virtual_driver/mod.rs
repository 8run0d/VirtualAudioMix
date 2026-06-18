use std::{mem::size_of, os::windows::process::CommandExt, process::Command};

use serde::Serialize;
use serde_json::Value;
use windows::{
    core::w,
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_SHARE_READ,
            FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::{IO::DeviceIoControl, Threading::CREATE_NO_WINDOW},
    },
};

const SERVICE_NAME: &str = "VAMAudio";
const MEDIA_INSTANCE_ID: &str = "ROOT\\MEDIA\\0000";
const IOCTL_VAMAUDIO_GET_STATUS: u32 = 0x0022_6004;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualDriverDevice {
    pub class: String,
    pub status: String,
    pub friendly_name: String,
    pub instance_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualDriverStatus {
    pub service_name: String,
    pub service_installed: bool,
    pub service_running: bool,
    pub service_state: Option<String>,
    pub driver_path: Option<String>,
    pub media_device_ok: bool,
    pub input_endpoint_ok: bool,
    pub output_endpoint_ok: bool,
    pub apo_ok: bool,
    pub devices: Vec<VirtualDriverDevice>,
}

#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
struct RawAudioTransportStatus {
    buffer_size: u32,
    available_bytes: u32,
    total_bytes_written: u64,
    total_bytes_read: u64,
    overflow_bytes: u64,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    block_align: u16,
    capture_buffer_size: u32,
    capture_available_bytes: u32,
    capture_total_bytes_written: u64,
    capture_total_bytes_read: u64,
    capture_overflow_bytes: u64,
    capture_underrun_bytes: u64,
    capture_active_readers: u32,
    capture_max_reader_available_bytes: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTransportStatus {
    pub render_buffer_size: u32,
    pub render_available_bytes: u32,
    pub render_total_bytes_written: u64,
    pub render_total_bytes_read: u64,
    pub render_overflow_bytes: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub block_align: u16,
    pub capture_buffer_size: u32,
    pub capture_available_bytes: u32,
    pub capture_total_bytes_written: u64,
    pub capture_total_bytes_read: u64,
    pub capture_overflow_bytes: u64,
    pub capture_underrun_bytes: u64,
    pub capture_active_readers: u32,
    pub capture_max_reader_available_bytes: u32,
}

pub fn get_status() -> Result<VirtualDriverStatus, String> {
    let service = read_service()?;
    let devices = read_devices()?;

    Ok(VirtualDriverStatus {
        service_name: SERVICE_NAME.to_string(),
        service_installed: service.is_some(),
        service_running: service
            .as_ref()
            .and_then(|value| value.get("Started"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        service_state: service
            .as_ref()
            .and_then(|value| value.get("State"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        driver_path: service
            .as_ref()
            .and_then(|value| value.get("PathName"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        media_device_ok: devices.iter().any(|device| {
            device.instance_id.eq_ignore_ascii_case(MEDIA_INSTANCE_ID) && device.status == "OK"
        }),
        input_endpoint_ok: devices.iter().any(|device| {
            device.class == "AudioEndpoint"
                && device.status == "OK"
                && device.friendly_name.contains("VAM Entr")
        }),
        output_endpoint_ok: devices.iter().any(|device| {
            device.class == "AudioEndpoint"
                && device.status == "OK"
                && device.friendly_name.contains("VAM Sortie")
        }),
        apo_ok: devices.iter().any(|device| {
            device.class == "AudioProcessingObject"
                && device.status == "OK"
                && device.friendly_name.contains("Audio Proxy APO")
        }),
        devices,
    })
}

pub fn get_transport_status() -> Result<Option<AudioTransportStatus>, String> {
    let handle = match unsafe {
        CreateFileW(
            w!("\\\\.\\VAMAudioTransport"),
            FILE_GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    } {
        Ok(handle) => OwnedHandle(handle),
        Err(_) => return Ok(None),
    };

    let mut raw = RawAudioTransportStatus::default();
    let mut bytes_returned = 0_u32;
    unsafe {
        DeviceIoControl(
            handle.0,
            IOCTL_VAMAUDIO_GET_STATUS,
            None,
            0,
            Some((&mut raw as *mut RawAudioTransportStatus).cast()),
            size_of::<RawAudioTransportStatus>() as u32,
            Some(&mut bytes_returned),
            None,
        )
    }
    .map_err(|error| error.to_string())?;

    Ok(Some(AudioTransportStatus {
        render_buffer_size: raw.buffer_size,
        render_available_bytes: raw.available_bytes,
        render_total_bytes_written: raw.total_bytes_written,
        render_total_bytes_read: raw.total_bytes_read,
        render_overflow_bytes: raw.overflow_bytes,
        sample_rate: raw.sample_rate,
        channels: raw.channels,
        bits_per_sample: raw.bits_per_sample,
        block_align: raw.block_align,
        capture_buffer_size: raw.capture_buffer_size,
        capture_available_bytes: raw.capture_available_bytes,
        capture_total_bytes_written: raw.capture_total_bytes_written,
        capture_total_bytes_read: raw.capture_total_bytes_read,
        capture_overflow_bytes: raw.capture_overflow_bytes,
        capture_underrun_bytes: raw.capture_underrun_bytes,
        capture_active_readers: raw.capture_active_readers,
        capture_max_reader_available_bytes: raw.capture_max_reader_available_bytes,
    }))
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn read_service() -> Result<Option<Value>, String> {
    let script = format!(
        "Get-CimInstance Win32_SystemDriver | \
         Where-Object {{ $_.Name -eq '{}' }} | \
         Select-Object Name,State,Started,PathName | \
         ConvertTo-Json -Compress",
        SERVICE_NAME
    );
    let value = run_powershell_json(&script)?;
    Ok(match value {
        Value::Array(values) => values.into_iter().next(),
        Value::Null => None,
        value => Some(value),
    })
}

fn read_devices() -> Result<Vec<VirtualDriverDevice>, String> {
    let script = "Get-PnpDevice | \
        Where-Object { $_.FriendlyName -match 'Bubux|BAD|VirtualAudioMix|VAM|SYSVAD|Audio Proxy' -or $_.InstanceId -match 'BAD|VAM|SYSVAD' } | \
        Select-Object Status,Class,FriendlyName,InstanceId | \
        ConvertTo-Json -Compress";

    let value = run_powershell_json(script)?;
    Ok(json_items(value)
        .into_iter()
        .map(|value| VirtualDriverDevice {
            class: json_string(&value, "Class"),
            status: json_string(&value, "Status"),
            friendly_name: json_string(&value, "FriendlyName"),
            instance_id: json_string(&value, "InstanceId"),
        })
        .collect())
}

fn run_powershell_json(script: &str) -> Result<Value, String> {
    let script = format!(
        "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; \
         $ErrorActionPreference='Stop'; {}",
        script
    );
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .creation_flags(CREATE_NO_WINDOW.0)
        .output()
        .map_err(|error| error.to_string())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("PowerShell exited with {}", output.status)
        } else {
            stderr
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        return Ok(Value::Null);
    }

    serde_json::from_str(&stdout).map_err(|error| error.to_string())
}

fn json_items(value: Value) -> Vec<Value> {
    match value {
        Value::Array(values) => values,
        Value::Null => Vec::new(),
        value => vec![value],
    }
}

fn json_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}
