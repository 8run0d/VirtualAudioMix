use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    thread,
    time::Duration,
};

pub mod app;

use app::core::audio_engine::AudioEngine;
use app::core::graph::graph::AudioGraph;
use app::core::types::device::DeviceInfo;
use app::core::types::stream::StreamInfo;
use app::platform::direct_route::{
    AudioNodeLevel, AudioRouteSpec, AudioRuntimeMetrics, DirectAudioRoute,
};
use app::platform::virtual_driver::{AudioTransportStatus, VirtualDriverStatus};
use serde::{Deserialize, Serialize};
use tauri::{
    image::Image,
    menu::{Menu, MenuBuilder, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WindowEvent,
};

const AUDIO_DEVICE_WATCH_INTERVAL: Duration = Duration::from_millis(1_500);
const TRAY_ICON_WATCH_INTERVAL: Duration = Duration::from_millis(5_000);
const WINDOWS_RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const WINDOWS_RUN_VALUE: &str = "VirtualAudioMix";
const WINDOWS_THEME_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const WINDOWS_APPS_USE_LIGHT_THEME_VALUE: &str = "AppsUseLightTheme";
const WINDOWS_APP_PREFS_KEY: &str = r"Software\Bruno Del piero\VirtualAudioMix";
const INSTALLER_START_WITH_WINDOWS_VALUE: &str = "InstallerStartWithWindows";
const INSTALLER_AUTO_START_AUDIO_VALUE: &str = "InstallerAutoStartAudio";
const INSTALLER_PROMPT_AUDIO_SETUP_VALUE: &str = "InstallerPromptAudioSetup";
const TRAY_PRESETS_MENU_ID: &str = "tray-presets";
const TRAY_PRESET_ID_PREFIX: &str = "tray-preset:";
const TRAY_EMPTY_PRESET_ID: &str = "tray-preset-empty";
const TRAY_AUDIO_TOGGLE_ID: &str = "tray-audio-toggle";
const TRAY_QUIT_ID: &str = "tray-quit";
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioDevicesChangedEvent {
    devices: Vec<DeviceInfo>,
}

struct AppState {
    engine: Mutex<AudioEngine>,
    direct_route: Mutex<Option<DirectAudioRoute>>,
    tray: Mutex<Option<TrayIcon>>,
    tray_presets: Mutex<Vec<TrayPreset>>,
    tray_audio_audible: AtomicBool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            engine: Mutex::new(AudioEngine::default()),
            direct_route: Mutex::new(None),
            tray: Mutex::new(None),
            tray_presets: Mutex::new(Vec::new()),
            tray_audio_audible: AtomicBool::new(false),
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrayPreset {
    id: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallerPreferences {
    start_with_windows: Option<bool>,
    auto_start_audio: Option<bool>,
    prompt_audio_setup: Option<bool>,
}

#[tauri::command]
fn list_audio_devices() -> Result<Vec<DeviceInfo>, String> {
    app::platform::device_manager::list_audio_devices()
}

fn spawn_audio_device_watcher(app_handle: AppHandle) {
    thread::Builder::new()
        .name("audio-device-watcher".to_string())
        .spawn(move || {
            let mut last_signature = String::new();
            loop {
                if let Ok(devices) = app::platform::device_manager::list_audio_devices() {
                    let signature = audio_device_signature(&devices);
                    if signature != last_signature {
                        last_signature = signature;
                        let _ = app_handle.emit(
                            "audio-devices-changed",
                            AudioDevicesChangedEvent { devices },
                        );
                    }
                }
                thread::sleep(AUDIO_DEVICE_WATCH_INTERVAL);
            }
        })
        .expect("failed to start audio device watcher");
}

fn spawn_tray_icon_watcher(app_handle: AppHandle) {
    thread::Builder::new()
        .name("tray-icon-watcher".to_string())
        .spawn(move || {
            let mut last_audible = None;
            loop {
                let Some(state) = app_handle.try_state::<AppState>() else {
                    thread::sleep(TRAY_ICON_WATCH_INTERVAL);
                    continue;
                };
                let route_active = state
                    .direct_route
                    .lock()
                    .map(|route| route.is_some())
                    .unwrap_or(false);
                let audible = route_active && default_render_volume_gain() > 0.001;
                if last_audible != Some(audible) {
                    update_tray_audio_audible(&app_handle, &state, audible);
                    last_audible = Some(audible);
                }
                thread::sleep(TRAY_ICON_WATCH_INTERVAL);
            }
        })
        .expect("failed to start tray icon watcher");
}

#[cfg(windows)]
fn default_render_volume_gain() -> f32 {
    app::platform::windows::endpoint_volume::default_render_volume_gain()
}

#[cfg(not(windows))]
fn default_render_volume_gain() -> f32 {
    1.0
}

fn audio_device_signature(devices: &[DeviceInfo]) -> String {
    let mut entries = devices
        .iter()
        .map(|device| {
            format!(
                "{:?}:{}:{}:{}",
                device.kind, device.name, device.channels, device.sample_rate
            )
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries.join("|")
}

fn install_tray(app_handle: &AppHandle) -> tauri::Result<TrayIcon> {
    let menu = build_tray_menu(app_handle, &[], false)?;
    let mut builder = TrayIconBuilder::with_id("vam-tray")
        .menu(&menu)
        .tooltip("VirtualAudioMix")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => show_main_window(tray.app_handle()),
            _ => {}
        })
        .on_menu_event(|app, event| handle_tray_menu_event(app, event.id().as_ref()));

    if let Some(icon) = tray_icon_image(false) {
        builder = builder.icon(icon);
    }

    builder.build(app_handle)
}

fn show_main_window(app_handle: &AppHandle) {
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn handle_tray_menu_event(app_handle: &AppHandle, id: &str) {
    if let Some(preset_id) = id.strip_prefix(TRAY_PRESET_ID_PREFIX) {
        if preset_id != "empty" {
            let _ = app_handle.emit("tray-preset-selected", preset_id.to_string());
        }
        return;
    }

    match id {
        TRAY_AUDIO_TOGGLE_ID => {
            let _ = app_handle.emit("tray-audio-toggle", ());
        }
        TRAY_QUIT_ID => {
            QUIT_REQUESTED.store(true, Ordering::SeqCst);
            app_handle.exit(0);
        }
        TRAY_EMPTY_PRESET_ID => {}
        _ => {}
    }
}

fn build_tray_menu(
    app_handle: &AppHandle,
    presets: &[TrayPreset],
    audio_audible: bool,
) -> tauri::Result<Menu<tauri::Wry>> {
    let mut preset_menu = SubmenuBuilder::with_id(app_handle, TRAY_PRESETS_MENU_ID, "Preset");
    if presets.is_empty() {
        preset_menu = preset_menu.text(TRAY_EMPTY_PRESET_ID, "Aucun preset");
    } else {
        for preset in presets {
            preset_menu = preset_menu.text(
                format!("{TRAY_PRESET_ID_PREFIX}{}", preset.id),
                sanitize_menu_label(&preset.name),
            );
        }
    }

    let preset_submenu = preset_menu.build()?;
    let engine_label = if audio_audible {
        "■ Arrêter le moteur son"
    } else {
        "▶ Démarrer le moteur son"
    };

    MenuBuilder::new(app_handle)
        .item(&preset_submenu)
        .text(TRAY_AUDIO_TOGGLE_ID, engine_label)
        .separator()
        .text(TRAY_QUIT_ID, "Quitter VAM")
        .build()
}

fn sanitize_menu_label(label: &str) -> String {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        "Preset sans nom".to_string()
    } else {
        trimmed.replace('&', "&&")
    }
}

fn refresh_tray_menu(app_handle: &AppHandle, state: &AppState) -> Result<(), String> {
    let presets = state
        .tray_presets
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    let audio_audible = state.tray_audio_audible.load(Ordering::SeqCst);
    let menu = build_tray_menu(app_handle, &presets, audio_audible).map_err(|error| error.to_string())?;
    if let Some(tray) = state
        .tray
        .lock()
        .map_err(|error| error.to_string())?
        .as_ref()
    {
        tray.set_menu(Some(menu)).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn update_tray_audio_audible(app_handle: &AppHandle, state: &AppState, audible: bool) {
    state.tray_audio_audible.store(audible, Ordering::SeqCst);
    if let Ok(tray) = state.tray.lock() {
        if let Some(tray) = tray.as_ref() {
            if let Some(icon) = tray_icon_image(audible) {
                let _ = tray.set_icon(Some(icon));
            }
        }
    }
    let _ = refresh_tray_menu(app_handle, state);
}

fn tray_icon_image(audio_audible: bool) -> Option<Image<'static>> {
    let dark_theme = cached_windows_uses_dark_apps_theme();
    let bytes = match (audio_audible, dark_theme) {
        (true, true) => include_bytes!("../../images/logo barre outils sonblc.png").as_slice(),
        (true, false) => include_bytes!("../../images/logo barre outils son noir.png").as_slice(),
        (false, true) => include_bytes!("../../images/logo barre outils pas-de-son blc.png").as_slice(),
        (false, false) => include_bytes!("../../images/logo barre outils pas-de-son noir.png").as_slice(),
    };
    Image::from_bytes(bytes).ok().map(Image::to_owned)
}

fn cached_windows_uses_dark_apps_theme() -> bool {
    static DARK_THEME: OnceLock<bool> = OnceLock::new();
    *DARK_THEME.get_or_init(windows_uses_dark_apps_theme)
}

#[cfg(windows)]
fn windows_uses_dark_apps_theme() -> bool {
    read_hkcu_dword(WINDOWS_THEME_KEY, WINDOWS_APPS_USE_LIGHT_THEME_VALUE)
        .map(|value| value == 0)
        .unwrap_or(true)
}

#[cfg(not(windows))]
fn windows_uses_dark_apps_theme() -> bool {
    true
}

#[cfg(windows)]
fn read_hkcu_dword(subkey: &str, value_name: &str) -> Option<u32> {
    use std::ffi::c_void;
    use windows::core::HSTRING;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegGetValueW, HKEY_CURRENT_USER, REG_VALUE_TYPE, RRF_RT_REG_DWORD,
    };

    let mut value = 0_u32;
    let mut value_type = REG_VALUE_TYPE::default();
    let mut byte_len = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(subkey),
            &HSTRING::from(value_name),
            RRF_RT_REG_DWORD,
            Some(&mut value_type),
            Some((&mut value as *mut u32).cast::<c_void>()),
            Some(&mut byte_len),
        )
    };

    (status == ERROR_SUCCESS).then_some(value)
}

#[cfg(not(windows))]
fn read_hkcu_dword(_subkey: &str, _value_name: &str) -> Option<u32> {
    None
}

#[cfg(windows)]
fn hkcu_value_exists(subkey: &str, value_name: &str) -> bool {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

    let mut byte_len = 0_u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(subkey),
            &HSTRING::from(value_name),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut byte_len),
        )
    };
    status == ERROR_SUCCESS
}

#[cfg(windows)]
fn set_hkcu_string_value(subkey: &str, value_name: &str, value: &str) -> Result<(), String> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ,
    };

    let mut key = HKEY::default();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(subkey),
            None,
            KEY_SET_VALUE,
            &mut key,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("Ouverture registre impossible: code {}", status.0));
    }

    let mut wide = value.encode_utf16().collect::<Vec<_>>();
    wide.push(0);
    let bytes = unsafe {
        std::slice::from_raw_parts(
            wide.as_ptr().cast::<u8>(),
            wide.len() * std::mem::size_of::<u16>(),
        )
    };
    let status = unsafe { RegSetValueExW(key, &HSTRING::from(value_name), None, REG_SZ, Some(bytes)) };
    unsafe {
        let _ = RegCloseKey(key);
    }

    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("Écriture registre impossible: code {}", status.0))
    }
}

#[cfg(windows)]
fn delete_hkcu_value(subkey: &str, value_name: &str) -> Result<(), String> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE,
    };

    let mut key = HKEY::default();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(subkey),
            None,
            KEY_SET_VALUE,
            &mut key,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("Ouverture registre impossible: code {}", status.0));
    }

    let status = unsafe { RegDeleteValueW(key, &HSTRING::from(value_name)) };
    unsafe {
        let _ = RegCloseKey(key);
    }

    if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(format!("Suppression registre impossible: code {}", status.0))
    }
}

#[cfg(not(windows))]
fn hkcu_value_exists(_subkey: &str, _value_name: &str) -> bool {
    false
}

#[cfg(not(windows))]
fn set_hkcu_string_value(_subkey: &str, _value_name: &str, _value: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
fn delete_hkcu_value(_subkey: &str, _value_name: &str) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
fn list_audio_sessions() -> Result<Vec<StreamInfo>, String> {
    app::platform::session_manager::list_audio_sessions()
}

#[tauri::command]
fn get_graph_snapshot(state: tauri::State<'_, AppState>) -> Result<AudioGraph, String> {
    state
        .engine
        .lock()
        .map(|engine| engine.graph().clone())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_virtual_driver_status() -> Result<VirtualDriverStatus, String> {
    app::platform::virtual_driver::get_status()
}

#[tauri::command]
fn get_audio_transport_status() -> Result<Option<AudioTransportStatus>, String> {
    app::platform::virtual_driver::get_transport_status()
}

#[tauri::command]
fn get_audio_runtime_metrics() -> AudioRuntimeMetrics {
    app::platform::direct_route::get_runtime_metrics()
}

#[tauri::command]
fn get_audio_node_levels() -> Vec<AudioNodeLevel> {
    app::platform::direct_route::get_audio_node_levels()
}

#[tauri::command]
fn get_dynamic_latency_enabled() -> bool {
    app::platform::direct_route::get_dynamic_latency_enabled()
}

#[tauri::command]
fn set_dynamic_latency_enabled(enabled: bool) {
    app::platform::direct_route::set_dynamic_latency_enabled(enabled);
}

#[tauri::command]
fn get_manual_latency_target_ms() -> u64 {
    app::platform::direct_route::get_manual_latency_target_ms()
}

#[tauri::command]
fn set_manual_latency_target_ms(latency_ms: u64) -> u64 {
    app::platform::direct_route::set_manual_latency_target_ms(latency_ms)
}

#[tauri::command]
fn get_start_with_windows() -> bool {
    hkcu_value_exists(WINDOWS_RUN_KEY, WINDOWS_RUN_VALUE)
}

#[tauri::command]
fn set_start_with_windows(enabled: bool) -> Result<(), String> {
    if enabled {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let value = format!("\"{}\"", exe.display());
        return set_hkcu_string_value(WINDOWS_RUN_KEY, WINDOWS_RUN_VALUE, &value);
    }

    delete_hkcu_value(WINDOWS_RUN_KEY, WINDOWS_RUN_VALUE)
}

#[tauri::command]
fn get_installer_preferences() -> InstallerPreferences {
    InstallerPreferences {
        start_with_windows: read_hkcu_dword(WINDOWS_APP_PREFS_KEY, INSTALLER_START_WITH_WINDOWS_VALUE)
            .map(|value| value != 0),
        auto_start_audio: read_hkcu_dword(WINDOWS_APP_PREFS_KEY, INSTALLER_AUTO_START_AUDIO_VALUE)
            .map(|value| value != 0),
        prompt_audio_setup: read_hkcu_dword(WINDOWS_APP_PREFS_KEY, INSTALLER_PROMPT_AUDIO_SETUP_VALUE)
            .map(|value| value != 0),
    }
}

#[tauri::command]
fn update_tray_presets(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
    presets: Vec<TrayPreset>,
) -> Result<(), String> {
    *state
        .tray_presets
        .lock()
        .map_err(|error| error.to_string())? = presets;
    refresh_tray_menu(&app_handle, &state)
}

#[tauri::command]
fn start_direct_audio_route(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
    input_device_name: String,
    output_device_name: String,
) -> Result<(), String> {
    let route = DirectAudioRoute::start(&input_device_name, &output_device_name)?;
    let mut direct_route = state
        .direct_route
        .lock()
        .map_err(|error| error.to_string())?;
    *direct_route = Some(route);
    update_tray_audio_audible(&app_handle, &state, default_render_volume_gain() > 0.001);
    Ok(())
}

#[tauri::command]
fn start_system_audio_route(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
    output_device_name: String,
) -> Result<(), String> {
    let route = DirectAudioRoute::start_system_audio(&output_device_name)?;
    let mut direct_route = state
        .direct_route
        .lock()
        .map_err(|error| error.to_string())?;
    *direct_route = Some(route);
    update_tray_audio_audible(&app_handle, &state, default_render_volume_gain() > 0.001);
    Ok(())
}

#[tauri::command]
fn start_audio_graph_route(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
    routes: Vec<AudioRouteSpec>,
) -> Result<(), String> {
    let route = DirectAudioRoute::start_graph(routes)?;
    let mut direct_route = state
        .direct_route
        .lock()
        .map_err(|error| error.to_string())?;
    *direct_route = Some(route);
    update_tray_audio_audible(&app_handle, &state, default_render_volume_gain() > 0.001);
    Ok(())
}

#[tauri::command]
fn stop_direct_audio_route(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut direct_route = state
        .direct_route
        .lock()
        .map_err(|error| error.to_string())?;
    *direct_route = None;
    update_tray_audio_audible(&app_handle, &state, false);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .on_window_event(|window, event| {
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    if !QUIT_REQUESTED.load(Ordering::SeqCst) {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
                WindowEvent::Resized(_) => {
                    if window.is_minimized().unwrap_or(false) {
                        let _ = window.hide();
                    }
                }
                _ => {}
            }
        })
        .setup(|app| {
            spawn_audio_device_watcher(app.handle().clone());
            spawn_tray_icon_watcher(app.handle().clone());
            let tray = install_tray(app.handle())?;
            let state = app.state::<AppState>();
            *state.tray.lock().map_err(|error| error.to_string())? = Some(tray);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_audio_devices,
            list_audio_sessions,
            get_graph_snapshot,
            get_virtual_driver_status,
            get_audio_transport_status,
            get_audio_runtime_metrics,
            get_audio_node_levels,
            get_dynamic_latency_enabled,
            set_dynamic_latency_enabled,
            get_manual_latency_target_ms,
            set_manual_latency_target_ms,
            get_start_with_windows,
            set_start_with_windows,
            get_installer_preferences,
            update_tray_presets,
            start_direct_audio_route,
            start_system_audio_route,
            start_audio_graph_route,
            stop_direct_audio_route
        ])
        .run(tauri::generate_context!())
        .expect("failed to run VirtualAudioMix");
}
