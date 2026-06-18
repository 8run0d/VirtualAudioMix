use std::time::{Duration, Instant};

use windows::{
    core::{Interface, Result, GUID},
    Win32::{
        Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator},
        System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
            COINIT_MULTITHREADED,
        },
    },
};

const ENDPOINT_VOLUME_REFRESH_INTERVAL: Duration = Duration::from_millis(50);

#[repr(transparent)]
#[derive(Clone, PartialEq, Eq)]
struct IAudioEndpointVolume(windows::core::IUnknown);

unsafe impl Interface for IAudioEndpointVolume {
    type Vtable = IAudioEndpointVolumeVtbl;
    const IID: GUID = GUID::from_u128(0x5cdf2c82_841e_4546_9722_0cf74078229a);
}

#[repr(C)]
struct IAudioEndpointVolumeVtbl {
    base__: windows::core::IUnknown_Vtbl,
    register_control_change_notify: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows::core::HRESULT,
    unregister_control_change_notify: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut core::ffi::c_void,
    ) -> windows::core::HRESULT,
    get_channel_count:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u32) -> windows::core::HRESULT,
    set_master_volume_level: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        f32,
        *const GUID,
    ) -> windows::core::HRESULT,
    set_master_volume_level_scalar: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        f32,
        *const GUID,
    ) -> windows::core::HRESULT,
    get_master_volume_level:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut f32) -> windows::core::HRESULT,
    get_master_volume_level_scalar:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut f32) -> windows::core::HRESULT,
    set_channel_volume_level: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        f32,
        *const GUID,
    ) -> windows::core::HRESULT,
    set_channel_volume_level_scalar: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        u32,
        f32,
        *const GUID,
    ) -> windows::core::HRESULT,
    get_channel_volume_level:
        unsafe extern "system" fn(*mut core::ffi::c_void, u32, *mut f32) -> windows::core::HRESULT,
    get_channel_volume_level_scalar:
        unsafe extern "system" fn(*mut core::ffi::c_void, u32, *mut f32) -> windows::core::HRESULT,
    set_mute: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows::core::BOOL,
        *const GUID,
    ) -> windows::core::HRESULT,
    get_mute: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows::core::BOOL,
    ) -> windows::core::HRESULT,
}

impl IAudioEndpointVolume {
    unsafe fn get_master_volume_level_scalar(&self) -> Result<f32> {
        let mut volume = 1.0_f32;
        (Interface::vtable(self).get_master_volume_level_scalar)(
            Interface::as_raw(self),
            &mut volume,
        )
        .map(|| volume)
    }

    unsafe fn get_mute(&self) -> Result<bool> {
        let mut muted = windows::core::BOOL(0);
        (Interface::vtable(self).get_mute)(Interface::as_raw(self), &mut muted)
            .map(|| muted.as_bool())
    }
}

pub struct EndpointVolumeCache {
    endpoint: Option<IAudioEndpointVolume>,
    current_gain: f32,
    last_refresh: Instant,
}

impl EndpointVolumeCache {
    pub fn new() -> Self {
        let init_result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if init_result.is_err() {
            let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        }
        Self {
            endpoint: default_render_endpoint_volume().ok(),
            current_gain: 1.0,
            last_refresh: Instant::now() - ENDPOINT_VOLUME_REFRESH_INTERVAL,
        }
    }

    pub fn current_gain(&mut self) -> f32 {
        if self.last_refresh.elapsed() < ENDPOINT_VOLUME_REFRESH_INTERVAL {
            return self.current_gain;
        }

        self.last_refresh = Instant::now();
        if self.endpoint.is_none() {
            self.endpoint = default_render_endpoint_volume().ok();
        }

        let Some(endpoint) = &self.endpoint else {
            self.current_gain = 1.0;
            return self.current_gain;
        };

        let next_gain = unsafe {
            let muted = endpoint.get_mute().unwrap_or(false);
            if muted {
                0.0
            } else {
                endpoint.get_master_volume_level_scalar().unwrap_or(1.0)
            }
        };

        self.current_gain = next_gain.clamp(0.0, 1.0);
        self.current_gain
    }
}

pub fn default_render_volume_gain() -> f32 {
    default_render_endpoint_volume()
        .and_then(|endpoint| unsafe { endpoint.get_master_volume_level_scalar() })
        .unwrap_or(1.0)
        .clamp(0.0, 1.0)
}

fn default_render_endpoint_volume() -> Result<IAudioEndpointVolume> {
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
    let endpoint = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole)? };
    unsafe { endpoint.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }
}
