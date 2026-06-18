#[cfg(target_os = "windows")]
pub mod direct_route;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "windows")]
pub use windows::{device_manager, session_manager, virtual_driver};

#[cfg(not(target_os = "windows"))]
pub mod direct_route {
    pub struct DirectAudioRoute;

    impl DirectAudioRoute {
        pub fn start(_input_device_name: &str, _output_device_name: &str) -> Result<Self, String> {
            Err(
                "Le routage audio direct est disponible uniquement sur Windows pour l'instant."
                    .to_string(),
            )
        }

        pub fn start_system_audio(_output_device_name: &str) -> Result<Self, String> {
            Err(
                "Le loopback système est disponible uniquement sur Windows pour l'instant."
                    .to_string(),
            )
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub mod device_manager {
    use crate::app::core::types::device::DeviceInfo;

    pub fn list_audio_devices() -> Result<Vec<DeviceInfo>, String> {
        Ok(Vec::new())
    }
}

#[cfg(not(target_os = "windows"))]
pub mod session_manager {
    use crate::app::core::types::stream::StreamInfo;

    pub fn list_audio_sessions() -> Result<Vec<StreamInfo>, String> {
        Ok(Vec::new())
    }
}

#[cfg(not(target_os = "windows"))]
pub mod virtual_driver {
    use serde::Serialize;

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
        Ok(VirtualDriverStatus {
            service_name: "VAMAudio".to_string(),
            service_installed: false,
            service_running: false,
            service_state: None,
            driver_path: None,
            media_device_ok: false,
            input_endpoint_ok: false,
            output_endpoint_ok: false,
            apo_ok: false,
            devices: Vec::new(),
        })
    }

    pub fn get_transport_status() -> Result<Option<AudioTransportStatus>, String> {
        Ok(None)
    }
}
