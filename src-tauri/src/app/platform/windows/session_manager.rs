use std::collections::BTreeMap;

use windows::{
    core::Interface,
    Win32::{
        Media::Audio::{
            eConsole, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
            MMDeviceEnumerator,
        },
        System::Com::{
            CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
            COINIT_MULTITHREADED,
        },
    },
};

use crate::app::core::types::stream::StreamInfo;
use crate::app::platform::windows::process_mapper;

pub fn list_audio_sessions() -> Result<Vec<StreamInfo>, String> {
    let init_result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if init_result.is_err() {
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    }

    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .map_err(|error| error.to_string())?;
    let endpoint = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
        .map_err(|error| error.to_string())?;
    let session_manager = unsafe { endpoint.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) }
        .map_err(|error| error.to_string())?;
    let sessions =
        unsafe { session_manager.GetSessionEnumerator() }.map_err(|error| error.to_string())?;
    let count = unsafe { sessions.GetCount() }.map_err(|error| error.to_string())?;
    let mut by_process = BTreeMap::<u32, StreamInfo>::new();
    let current_process_id = std::process::id();

    for index in 0..count {
        let Ok(session) = (unsafe { sessions.GetSession(index) }) else {
            continue;
        };
        let Ok(session2) = session.cast::<IAudioSessionControl2>() else {
            continue;
        };
        if is_system_sound_session(&session2) {
            continue;
        }

        let Ok(process_id) = (unsafe { session2.GetProcessId() }) else {
            continue;
        };
        if process_id == 0 || process_id == current_process_id {
            continue;
        }

        let label = process_mapper::process_label(process_id);
        by_process.entry(process_id).or_insert_with(|| StreamInfo {
            id: format!("process-{process_id}"),
            label,
            process_id: Some(process_id),
            level: 0.0,
        });
    }

    Ok(by_process.into_values().collect())
}

fn is_system_sound_session(session: &IAudioSessionControl2) -> bool {
    unsafe { session.IsSystemSoundsSession().0 == 0 }
}
