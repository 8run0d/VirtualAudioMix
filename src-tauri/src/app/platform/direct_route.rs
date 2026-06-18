use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::{Seek, SeekFrom, Write},
    mem::{size_of, ManuallyDrop},
    ptr::null_mut,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, SampleFormat, SampleRate, Stream, StreamConfig,
};
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use serde::Deserialize;
use serde::Serialize;
use windows::{
    core::{implement, w, Interface, Ref, HRESULT},
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Media::{
            timeBeginPeriod, timeEndPeriod,
            Audio::{
                ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
                IActivateAudioInterfaceCompletionHandler,
                IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient,
                AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_LOOPBACK,
                AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0,
                AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
                PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
                VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX, WAVE_FORMAT_PCM,
            },
        },
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
            FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::{
            Com::{
                CoInitializeEx,
                StructuredStorage::{
                    PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
                },
                BLOB, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED,
            },
            Variant::VT_BLOB,
            IO::DeviceIoControl,
        },
    },
};

use crate::app::platform::windows::endpoint_volume::EndpointVolumeCache;

const TARGET_LATENCY_MS: usize = 30;
const MAX_LATENCY_MS: usize = 120;
const VAM_CAPTURE_TARGET_LATENCY_MS: usize = 25;
const VAM_CAPTURE_MAX_LATENCY_MS: usize = 80;
const DYNAMIC_LATENCY_STEP_MS: usize = 5;
const DYNAMIC_LATENCY_STABLE_DECREASE_MS: usize = 30_000;
const DYNAMIC_LATENCY_OVERFLOWS_TO_INCREASE: u32 = 3;
const OUTPUT_DYNAMIC_MAX_LATENCY_MS: usize = 180;
const VAM_CAPTURE_DYNAMIC_MAX_LATENCY_MS: usize = 120;
const MANUAL_LATENCY_MIN_MS: u64 = 10;
const MANUAL_LATENCY_MAX_MS: u64 = 200;
const VAM_RENDER_SAMPLE_RATE: u32 = 48_000;
const VAM_RENDER_CHANNELS: usize = 2;
const VAM_RENDER_BITS_PER_SAMPLE: u16 = 16;
const VAM_RENDER_BYTES_PER_FRAME: usize = VAM_RENDER_CHANNELS * 2;
const VAM_CAPTURE_SAMPLE_RATE: u32 = 48_000;
const VAM_CAPTURE_CHANNELS: usize = 2;
const VAM_CAPTURE_FRAMES_PER_WRITE: usize = 240;
const PROCESS_LOOPBACK_SAMPLE_RATE: u32 = 48_000;
const PROCESS_LOOPBACK_CHANNELS: usize = 2;
const PROCESS_LOOPBACK_BITS_PER_SAMPLE: u16 = 16;
const PROCESS_LOOPBACK_BUFFER_HNS: i64 = 0;
const MAX_ROUTABLE_CHANNELS: usize = 16;
const VISUALIZER_FFT_SIZE: usize = 512;
const VISUALIZER_FFT_BANDS: usize = 8;
const VISUALIZER_WAVEFORM_POINTS: usize = 48;
const VISUALIZER_FFT_BUDGET_US: u64 = 1_500;
const VISUALIZER_FFT_COOLDOWN_MS: u64 = 5_000;
const VISUALIZER_FFT_MIN_INTERVAL_MS: u64 = 30;
const IOCTL_VAMAUDIO_READ_RENDER: u32 = 0x0022_6000;
const IOCTL_VAMAUDIO_GET_STATUS: u32 = 0x0022_6004;
const IOCTL_VAMAUDIO_WRITE_CAPTURE: u32 = 0x0022_A008;
static VAM_CAPTURE_WRITER_LATE_TOTAL_US: AtomicU64 = AtomicU64::new(0);
static VAM_CAPTURE_WRITER_LATE_MAX_US: AtomicU64 = AtomicU64::new(0);
static VAM_CAPTURE_WRITER_LATE_SAMPLES: AtomicU64 = AtomicU64::new(0);
static DYNAMIC_LATENCY_TARGET_MS: AtomicU64 = AtomicU64::new(0);
static DYNAMIC_LATENCY_MAX_MS: AtomicU64 = AtomicU64::new(0);
static DYNAMIC_LATENCY_OVERFLOW_EVENTS: AtomicU64 = AtomicU64::new(0);
static DYNAMIC_LATENCY_ADJUSTMENTS: AtomicU64 = AtomicU64::new(0);
static DYNAMIC_LATENCY_ENABLED: AtomicBool = AtomicBool::new(true);
static MANUAL_LATENCY_TARGET_MS: AtomicU64 = AtomicU64::new(TARGET_LATENCY_MS as u64);
static AUDIO_NODE_LEVELS: OnceLock<Mutex<HashMap<String, AudioNodeMeter>>> = OnceLock::new();
static AUDIO_VISUALIZER: OnceLock<Mutex<FftVisualizer>> = OnceLock::new();
static AUDIO_VISUALIZER_FFT_ENABLED: AtomicBool = AtomicBool::new(true);
static AUDIO_VISUALIZER_FFT_LAST_US: AtomicU64 = AtomicU64::new(0);
static AUDIO_VISUALIZER_FFT_FALLBACKS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioRuntimeMetrics {
    pub vam_capture_writer_late_avg_us: u64,
    pub vam_capture_writer_late_max_us: u64,
    pub vam_capture_writer_late_samples: u64,
    pub dynamic_latency_target_ms: u64,
    pub dynamic_latency_max_ms: u64,
    pub dynamic_latency_overflow_events: u64,
    pub dynamic_latency_adjustments: u64,
    pub dynamic_latency_enabled: bool,
    pub manual_latency_target_ms: u64,
    pub manual_latency_min_ms: u64,
    pub manual_latency_max_ms: u64,
    pub audio_visualizer_fft_enabled: bool,
    pub audio_visualizer_fft_last_us: u64,
    pub audio_visualizer_fft_fallbacks: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioNodeLevel {
    pub node_id: String,
    pub level: f32,
    pub bands: [f32; VISUALIZER_FFT_BANDS],
    pub waveform: Vec<f32>,
}

#[derive(Debug, Clone, Copy)]
struct AudioNodeMeter {
    level: f32,
    bands: [f32; VISUALIZER_FFT_BANDS],
    waveform: [f32; VISUALIZER_WAVEFORM_POINTS],
}

struct FftVisualizer {
    fft: Arc<dyn Fft<f32>>,
    buffer: Vec<Complex<f32>>,
    disabled_until: Option<Instant>,
    last_analysis_by_node: HashMap<String, Instant>,
}

pub fn get_runtime_metrics() -> AudioRuntimeMetrics {
    let samples = VAM_CAPTURE_WRITER_LATE_SAMPLES.load(Ordering::Relaxed);
    let total = VAM_CAPTURE_WRITER_LATE_TOTAL_US.load(Ordering::Relaxed);
    AudioRuntimeMetrics {
        vam_capture_writer_late_avg_us: if samples > 0 { total / samples } else { 0 },
        vam_capture_writer_late_max_us: VAM_CAPTURE_WRITER_LATE_MAX_US.load(Ordering::Relaxed),
        vam_capture_writer_late_samples: samples,
        dynamic_latency_target_ms: DYNAMIC_LATENCY_TARGET_MS.load(Ordering::Relaxed),
        dynamic_latency_max_ms: DYNAMIC_LATENCY_MAX_MS.load(Ordering::Relaxed),
        dynamic_latency_overflow_events: DYNAMIC_LATENCY_OVERFLOW_EVENTS.load(Ordering::Relaxed),
        dynamic_latency_adjustments: DYNAMIC_LATENCY_ADJUSTMENTS.load(Ordering::Relaxed),
        dynamic_latency_enabled: dynamic_latency_enabled(),
        manual_latency_target_ms: manual_latency_target_ms(),
        manual_latency_min_ms: MANUAL_LATENCY_MIN_MS,
        manual_latency_max_ms: MANUAL_LATENCY_MAX_MS,
        audio_visualizer_fft_enabled: AUDIO_VISUALIZER_FFT_ENABLED.load(Ordering::Relaxed),
        audio_visualizer_fft_last_us: AUDIO_VISUALIZER_FFT_LAST_US.load(Ordering::Relaxed),
        audio_visualizer_fft_fallbacks: AUDIO_VISUALIZER_FFT_FALLBACKS.load(Ordering::Relaxed),
    }
}

pub fn get_audio_node_levels() -> Vec<AudioNodeLevel> {
    let Some(levels) = AUDIO_NODE_LEVELS.get() else {
        return Vec::new();
    };
    let Ok(mut levels) = levels.lock() else {
        return Vec::new();
    };

    let result: Vec<AudioNodeLevel> = levels
        .iter()
        .map(|(node_id, meter)| AudioNodeLevel {
            node_id: node_id.clone(),
            level: meter.level,
            bands: meter.bands,
            waveform: meter.waveform.to_vec(),
        })
        .collect();
    for meter in levels.values_mut() {
        meter.level *= 0.82;
        for band in &mut meter.bands {
            *band *= 0.78;
        }
        for point in &mut meter.waveform {
            *point *= 0.72;
        }
    }
    result
}

pub fn get_dynamic_latency_enabled() -> bool {
    dynamic_latency_enabled()
}

pub fn set_dynamic_latency_enabled(enabled: bool) {
    DYNAMIC_LATENCY_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn get_manual_latency_target_ms() -> u64 {
    manual_latency_target_ms()
}

pub fn set_manual_latency_target_ms(latency_ms: u64) -> u64 {
    let latency_ms = sanitize_manual_latency_ms(latency_ms);
    MANUAL_LATENCY_TARGET_MS.store(latency_ms, Ordering::Relaxed);
    latency_ms
}

pub fn capture_process_loopback_wav(
    process_id: u32,
    output_path: &str,
    duration: Duration,
) -> Result<u64, String> {
    initialize_com_for_audio();
    let audio_client = activate_process_loopback_client(process_id)?;
    let wave_format = process_loopback_wave_format();
    let mut wav_file = WavProbeWriter::create(output_path, wave_format)?;

    unsafe {
        audio_client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
                PROCESS_LOOPBACK_BUFFER_HNS,
                0,
                &wave_format as *const WAVEFORMATEX,
                None,
            )
            .map_err(|error| {
                format!("Initialisation probe process loopback impossible: {error}")
            })?;

        let capture_client: IAudioCaptureClient = audio_client.GetService().map_err(|error| {
            format!("Capture client probe process loopback indisponible: {error}")
        })?;

        audio_client
            .Start()
            .map_err(|error| format!("Démarrage probe process loopback impossible: {error}"))?;

        let started_at = Instant::now();
        let mut total_bytes = 0_u64;
        while started_at.elapsed() < duration {
            let mut packet_size = capture_client
                .GetNextPacketSize()
                .map_err(|error| format!("Lecture taille paquet probe impossible: {error}"))?;
            if packet_size == 0 {
                thread::sleep(Duration::from_millis(2));
                continue;
            }

            while packet_size > 0 {
                let mut data = null_mut::<u8>();
                let mut frames_available = 0_u32;
                let mut flags = 0_u32;
                capture_client
                    .GetBuffer(&mut data, &mut frames_available, &mut flags, None, None)
                    .map_err(|error| format!("Lecture buffer probe impossible: {error}"))?;

                let bytes = frames_available as usize * wave_format.nBlockAlign as usize;
                if data.is_null() || (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 {
                    wav_file.write_silence(bytes)?;
                } else {
                    let samples = std::slice::from_raw_parts(data, bytes);
                    wav_file.write_audio(samples)?;
                }
                total_bytes += bytes as u64;

                capture_client
                    .ReleaseBuffer(frames_available)
                    .map_err(|error| format!("Release buffer probe impossible: {error}"))?;

                packet_size = capture_client
                    .GetNextPacketSize()
                    .map_err(|error| format!("Lecture paquet suivant probe impossible: {error}"))?;
            }
        }

        let _ = audio_client.Stop();
        wav_file.finalize()?;
        Ok(total_bytes)
    }
}

fn reset_runtime_metrics() {
    VAM_CAPTURE_WRITER_LATE_TOTAL_US.store(0, Ordering::Relaxed);
    VAM_CAPTURE_WRITER_LATE_MAX_US.store(0, Ordering::Relaxed);
    VAM_CAPTURE_WRITER_LATE_SAMPLES.store(0, Ordering::Relaxed);
    DYNAMIC_LATENCY_TARGET_MS.store(0, Ordering::Relaxed);
    DYNAMIC_LATENCY_MAX_MS.store(0, Ordering::Relaxed);
    DYNAMIC_LATENCY_OVERFLOW_EVENTS.store(0, Ordering::Relaxed);
    DYNAMIC_LATENCY_ADJUSTMENTS.store(0, Ordering::Relaxed);
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioRouteSpec {
    pub source_kind: AudioRouteSourceKind,
    pub source_node_id: Option<String>,
    pub source_name: Option<String>,
    pub source_process_id: Option<u32>,
    pub source_channel: Option<RouteChannel>,
    pub target_kind: AudioRouteTargetKind,
    pub target_node_id: Option<String>,
    pub target_name: String,
    pub target_channel: Option<RouteChannel>,
    pub gain: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RouteChannel {
    All(String),
    Index(usize),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ChannelSelection {
    All,
    Index(usize),
}

impl ChannelSelection {
    fn from_route(value: &Option<RouteChannel>) -> Self {
        match value {
            Some(RouteChannel::Index(index)) => Self::Index(*index),
            Some(RouteChannel::All(value)) if value != "all" => {
                value.parse::<usize>().map(Self::Index).unwrap_or(Self::All)
            }
            _ => Self::All,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct AudioFrame {
    samples: [f32; MAX_ROUTABLE_CHANNELS],
    channels: usize,
}

impl AudioFrame {
    fn with_channels(channels: usize) -> Self {
        Self {
            samples: [0.0; MAX_ROUTABLE_CHANNELS],
            channels: channels.clamp(1, MAX_ROUTABLE_CHANNELS),
        }
    }

    fn mono(sample: f32) -> Self {
        let mut frame = Self::with_channels(2);
        frame.samples[0] = sample;
        frame.samples[1] = sample;
        frame
    }

    fn selected_sample(self, channel: usize) -> f32 {
        if channel < self.channels && channel < MAX_ROUTABLE_CHANNELS {
            self.samples[channel]
        } else {
            0.0
        }
    }

    fn mono_sum(self) -> f32 {
        if self.channels == 0 {
            return 0.0;
        }
        let channels = self.channels.min(MAX_ROUTABLE_CHANNELS);
        self.samples[..channels].iter().sum::<f32>() / channels as f32
    }
}

impl Default for AudioFrame {
    fn default() -> Self {
        Self::with_channels(2)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AudioRouteSourceKind {
    InputDevice,
    SystemAudio,
    Application,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AudioRouteTargetKind {
    OutputDevice,
    VirtualInput,
}

pub struct DirectAudioRoute {
    _input_streams: Vec<Stream>,
    _output_streams: Vec<Stream>,
    stop_reader: Arc<AtomicBool>,
    reader_threads: Vec<JoinHandle<()>>,
}

impl DirectAudioRoute {
    pub fn start(input_device_name: &str, output_device_name: &str) -> Result<Self, String> {
        Self::start_graph(vec![AudioRouteSpec {
            source_kind: AudioRouteSourceKind::InputDevice,
            source_node_id: None,
            source_name: Some(input_device_name.to_string()),
            source_process_id: None,
            source_channel: None,
            target_kind: AudioRouteTargetKind::OutputDevice,
            target_node_id: None,
            target_name: output_device_name.to_string(),
            target_channel: None,
            gain: 1.0,
        }])
    }

    pub fn start_system_audio(output_device_name: &str) -> Result<Self, String> {
        Self::start_graph(vec![AudioRouteSpec {
            source_kind: AudioRouteSourceKind::SystemAudio,
            source_node_id: None,
            source_name: None,
            source_process_id: None,
            source_channel: None,
            target_kind: AudioRouteTargetKind::OutputDevice,
            target_node_id: None,
            target_name: output_device_name.to_string(),
            target_channel: None,
            gain: 1.0,
        }])
    }

    pub fn start_graph(routes: Vec<AudioRouteSpec>) -> Result<Self, String> {
        if routes.is_empty() {
            return Err("Aucun lien audio valide à démarrer.".to_string());
        }

        reset_runtime_metrics();
        let host = cpal::default_host();
        let mut outputs = HashMap::<String, PendingOutput>::new();
        let mut virtual_inputs = HashMap::<String, PendingVirtualInput>::new();
        let mut source_targets = HashMap::<SourceKey, Vec<PendingFanoutTarget>>::new();
        let dynamic_latency_enabled = dynamic_latency_enabled();
        let manual_latency_ms = manual_latency_target_ms() as usize;
        let output_channel_requirements = output_channel_requirements(&routes);
        let input_channel_requirements = input_channel_requirements(&routes);
        for route in routes {
            let source_key = SourceKey::try_from_route(&route)?;
            let target_name = route.target_name.trim().to_string();
            if target_name.is_empty() {
                return Err("Nom de cible audio vide.".to_string());
            }

            let (target_sample_rate, target_frames, max_frames, mix_input) = match route.target_kind
            {
                AudioRouteTargetKind::OutputDevice => {
                    if !outputs.contains_key(&target_name) {
                        let output_device =
                            find_device(host.output_devices(), &target_name, "output")?;
                        let min_channels = output_channel_requirements
                            .get(&target_name)
                            .copied()
                            .unwrap_or(2);
                        let (stream_config, sample_format) =
                            choose_output_stream_config(&output_device, min_channels)?;
                        outputs.insert(
                            target_name.clone(),
                            PendingOutput {
                                device: output_device,
                                config: stream_config.clone(),
                                sample_format,
                                sample_rate: stream_config.sample_rate.0,
                                inputs: Vec::new(),
                            },
                        );
                    }

                    let output = outputs
                        .get_mut(&target_name)
                        .ok_or_else(|| format!("Sortie audio introuvable: {target_name}"))?;
                    let target_latency_ms = if dynamic_latency_enabled {
                        TARGET_LATENCY_MS
                    } else {
                        manual_latency_ms
                    };
                    let max_latency_ms = if dynamic_latency_enabled {
                        MAX_LATENCY_MS
                    } else {
                        manual_latency_ms * 4
                    };
                    let target_frames = ms_to_frames(output.sample_rate, target_latency_ms);
                    let max_frames =
                        ms_to_frames(output.sample_rate, max_latency_ms).max(target_frames);
                    let mix_input = MixInput {
                        buffer: Arc::new(Mutex::new(VecDeque::with_capacity(max_frames))),
                        gain: sanitize_gain(route.gain),
                        target_channel: ChannelSelection::from_route(&route.target_channel),
                    };
                    output.inputs.push(mix_input.clone());
                    (output.sample_rate, target_frames, max_frames, mix_input)
                }
                AudioRouteTargetKind::VirtualInput => {
                    if !virtual_inputs.contains_key(&target_name) {
                        virtual_inputs.insert(
                            target_name.clone(),
                            PendingVirtualInput { inputs: Vec::new() },
                        );
                    }
                    let target_latency_ms = if dynamic_latency_enabled {
                        VAM_CAPTURE_TARGET_LATENCY_MS
                    } else {
                        manual_latency_ms
                    };
                    let max_latency_ms = if dynamic_latency_enabled {
                        VAM_CAPTURE_MAX_LATENCY_MS
                    } else {
                        manual_latency_ms * 4
                    };
                    let target_frames = ms_to_frames(VAM_CAPTURE_SAMPLE_RATE, target_latency_ms);
                    let max_frames =
                        ms_to_frames(VAM_CAPTURE_SAMPLE_RATE, max_latency_ms).max(target_frames);
                    let mix_input = MixInput {
                        buffer: Arc::new(Mutex::new(VecDeque::with_capacity(max_frames))),
                        gain: sanitize_gain(route.gain),
                        target_channel: ChannelSelection::from_route(&route.target_channel),
                    };
                    let virtual_input = virtual_inputs
                        .get_mut(&target_name)
                        .ok_or_else(|| format!("Entrée virtuelle introuvable: {target_name}"))?;
                    virtual_input.inputs.push(mix_input.clone());
                    (
                        VAM_CAPTURE_SAMPLE_RATE,
                        target_frames,
                        max_frames,
                        mix_input,
                    )
                }
            };

            source_targets
                .entry(source_key)
                .or_default()
                .push(PendingFanoutTarget {
                    source_node_id: route.source_node_id.clone(),
                    target_node_id: route.target_node_id.clone(),
                    mix_input,
                    source_channel: ChannelSelection::from_route(&route.source_channel),
                    target_sample_rate,
                    latency_policy: DynamicLatencyPolicy::new(
                        dynamic_latency_enabled,
                        target_sample_rate,
                        target_frames,
                        max_frames,
                        match route.target_kind {
                            AudioRouteTargetKind::OutputDevice => OUTPUT_DYNAMIC_MAX_LATENCY_MS,
                            AudioRouteTargetKind::VirtualInput => {
                                VAM_CAPTURE_DYNAMIC_MAX_LATENCY_MS
                            }
                        },
                    ),
                });
        }

        if (outputs.is_empty() && virtual_inputs.is_empty()) || source_targets.is_empty() {
            return Err("Aucun lien audio valide à démarrer.".to_string());
        }

        let mut input_streams = Vec::new();
        let mut reader_threads = Vec::new();
        let stop_reader = Arc::new(AtomicBool::new(false));

        for (source_key, targets) in source_targets {
            match source_key {
                SourceKey::Input(device_name) => {
                    let input_device = find_device(host.input_devices(), &device_name, "input")?;
                    let preferred_sample_rate = targets
                        .iter()
                        .find(|target| target.target_sample_rate == VAM_CAPTURE_SAMPLE_RATE)
                        .or_else(|| targets.first())
                        .map(|target| target.target_sample_rate)
                        .unwrap_or(VAM_CAPTURE_SAMPLE_RATE);
                    let (stream_config, sample_format) = choose_input_stream_config(
                        &input_device,
                        preferred_sample_rate,
                        input_channel_requirements
                            .get(&device_name)
                            .copied()
                            .unwrap_or(1),
                    )?;
                    let fanout = AudioFanout::from_pending(targets, stream_config.sample_rate.0);
                    let input_stream =
                        build_input_stream(&input_device, &stream_config, sample_format, fanout)?;
                    input_stream.play().map_err(|error| error.to_string())?;
                    input_streams.push(input_stream);
                }
                SourceKey::System => {
                    let loopback_device = host.default_output_device().ok_or_else(|| {
                        "Périphérique de sortie Windows à capturer introuvable.".to_string()
                    })?;
                    let loopback_device_name =
                        loopback_device.name().map_err(|error| error.to_string())?;

                    if outputs.contains_key(&loopback_device_name) {
                        return Err(format!(
                            "Boucle audio refusée: la sortie système capturée et la sortie cible sont toutes les deux `{loopback_device_name}`. Mets `VAM Sortie` comme sortie Windows par défaut, puis route `Son système` vers une sortie physique."
                        ));
                    }

                    if is_virtual_driver_output_name(&loopback_device_name) {
                        let fanout = AudioFanout::from_pending(targets, VAM_RENDER_SAMPLE_RATE);
                        reader_threads.push(spawn_vam_transport_reader(
                            Arc::clone(&stop_reader),
                            fanout,
                        )?);
                    } else {
                        let (stream_config, sample_format) =
                            choose_output_stream_config(&loopback_device, 2)?;
                        let fanout =
                            AudioFanout::from_pending(targets, stream_config.sample_rate.0);
                        let input_stream = build_input_stream(
                            &loopback_device,
                            &stream_config,
                            sample_format,
                            fanout,
                        )?;
                        input_stream.play().map_err(|error| error.to_string())?;
                        input_streams.push(input_stream);
                    }
                }
                SourceKey::Process(process_id) => {
                    let fanout = AudioFanout::from_pending(targets, PROCESS_LOOPBACK_SAMPLE_RATE);
                    reader_threads.push(spawn_process_loopback_reader(
                        Arc::clone(&stop_reader),
                        process_id,
                        fanout,
                    )?);
                }
            }
        }

        let mut output_streams = Vec::new();
        for output in outputs.into_values() {
            let output_stream = build_output_stream(
                &output.device,
                &output.config,
                output.sample_format,
                output.inputs,
            )?;
            output_stream.play().map_err(|error| error.to_string())?;
            output_streams.push(output_stream);
        }

        for virtual_input in virtual_inputs.into_values() {
            reader_threads.push(spawn_vam_capture_writer(
                Arc::clone(&stop_reader),
                virtual_input.inputs,
            )?);
        }

        Ok(Self {
            _input_streams: input_streams,
            _output_streams: output_streams,
            stop_reader,
            reader_threads,
        })
    }
}

impl Drop for DirectAudioRoute {
    fn drop(&mut self) {
        self.stop_reader.store(true, Ordering::Release);
        for reader_thread in self.reader_threads.drain(..) {
            let _ = reader_thread.join();
        }
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
enum SourceKey {
    Input(String),
    System,
    Process(u32),
}

impl SourceKey {
    fn try_from_route(route: &AudioRouteSpec) -> Result<Self, String> {
        match route.source_kind {
            AudioRouteSourceKind::InputDevice => {
                let source_name = route
                    .source_name
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if source_name.is_empty() {
                    return Err("Nom d'entrée audio vide.".to_string());
                }
                Ok(Self::Input(source_name))
            }
            AudioRouteSourceKind::SystemAudio => Ok(Self::System),
            AudioRouteSourceKind::Application => {
                let process_id = route
                    .source_process_id
                    .ok_or_else(|| "PID de source processus manquant.".to_string())?;
                if process_id == 0 {
                    return Err("PID de source processus invalide.".to_string());
                }
                Ok(Self::Process(process_id))
            }
        }
    }
}

fn output_channel_requirements(routes: &[AudioRouteSpec]) -> HashMap<String, u16> {
    let mut requirements = HashMap::<String, u16>::new();
    for route in routes {
        if route.target_kind != AudioRouteTargetKind::OutputDevice {
            continue;
        }

        let target_name = route.target_name.trim();
        if target_name.is_empty() {
            continue;
        }

        let required_channels = required_stream_channels(&route.target_channel);
        requirements
            .entry(target_name.to_string())
            .and_modify(|channels| *channels = (*channels).max(required_channels))
            .or_insert(required_channels);
    }
    requirements
}

fn input_channel_requirements(routes: &[AudioRouteSpec]) -> HashMap<String, u16> {
    let mut requirements = HashMap::<String, u16>::new();
    for route in routes {
        if route.source_kind != AudioRouteSourceKind::InputDevice {
            continue;
        }

        let Some(source_name) = route.source_name.as_deref().map(str::trim) else {
            continue;
        };
        if source_name.is_empty() {
            continue;
        }

        let required_channels = required_stream_channels(&route.source_channel);
        requirements
            .entry(source_name.to_string())
            .and_modify(|channels| *channels = (*channels).max(required_channels))
            .or_insert(required_channels);
    }
    requirements
}

fn required_stream_channels(channel: &Option<RouteChannel>) -> u16 {
    match ChannelSelection::from_route(channel) {
        ChannelSelection::All => 2,
        ChannelSelection::Index(index) => (index + 1).max(2).min(u16::MAX as usize) as u16,
    }
}

struct PendingOutput {
    device: Device,
    config: StreamConfig,
    sample_format: SampleFormat,
    sample_rate: u32,
    inputs: Vec<MixInput>,
}

struct PendingVirtualInput {
    inputs: Vec<MixInput>,
}

#[derive(Clone)]
struct MixInput {
    buffer: Arc<Mutex<VecDeque<AudioFrame>>>,
    gain: f32,
    target_channel: ChannelSelection,
}

struct PendingFanoutTarget {
    source_node_id: Option<String>,
    target_node_id: Option<String>,
    mix_input: MixInput,
    source_channel: ChannelSelection,
    target_sample_rate: u32,
    latency_policy: DynamicLatencyPolicy,
}

struct AudioFanout {
    targets: Vec<FanoutTarget>,
}

fn build_channel_resamplers(
    input_sample_rate: u32,
    output_sample_rate: u32,
) -> Vec<InputResampler> {
    (0..MAX_ROUTABLE_CHANNELS)
        .map(|_| InputResampler::new(input_sample_rate, output_sample_rate))
        .collect()
}

struct FanoutTarget {
    source_node_id: Option<String>,
    target_node_id: Option<String>,
    mix_input: MixInput,
    source_channel: ChannelSelection,
    target_sample_rate: u32,
    latency_policy: DynamicLatencyPolicy,
    resamplers: Vec<InputResampler>,
}

impl AudioFanout {
    fn from_pending(targets: Vec<PendingFanoutTarget>, input_sample_rate: u32) -> Self {
        Self {
            targets: targets
                .into_iter()
                .map(|target| FanoutTarget {
                    resamplers: build_channel_resamplers(
                        input_sample_rate,
                        target.target_sample_rate,
                    ),
                    source_node_id: target.source_node_id,
                    target_node_id: target.target_node_id,
                    mix_input: target.mix_input,
                    source_channel: target.source_channel,
                    target_sample_rate: target.target_sample_rate,
                    latency_policy: target.latency_policy,
                })
                .collect(),
        }
    }

    fn reset_resamplers(&mut self, input_sample_rate: u32) {
        for target in &mut self.targets {
            target.resamplers =
                build_channel_resamplers(input_sample_rate, target.target_sample_rate);
        }
    }

    fn push_input<T: AudioSample>(&mut self, data: &[T], channels: usize) {
        let frames = data
            .chunks(channels.max(1))
            .map(|frame| audio_frame_from_samples(frame.iter().map(AudioSample::to_f32)))
            .collect::<Vec<_>>();
        self.push_frames(&frames);
    }

    fn push_frames(&mut self, frames: &[AudioFrame]) {
        let meter_samples = frames
            .iter()
            .map(|frame| frame.mono_sum())
            .collect::<Vec<_>>();
        for target in &mut self.targets {
            if let Some(node_id) = &target.source_node_id {
                let source_meter = signal_meter(Some(node_id), &meter_samples);
                record_node_meter(node_id, source_meter);
            }
            let output_frames = match target.source_channel {
                ChannelSelection::All => {
                    let channel_count = frames
                        .iter()
                        .map(|frame| frame.channels)
                        .max()
                        .unwrap_or(2)
                        .clamp(1, MAX_ROUTABLE_CHANNELS);
                    let channel_outputs = (0..channel_count)
                        .map(|channel| {
                            let samples = frames
                                .iter()
                                .map(|frame| frame.selected_sample(channel))
                                .collect::<Vec<_>>();
                            target.resamplers[channel].process(&samples)
                        })
                        .collect::<Vec<_>>();
                    let output_len = channel_outputs
                        .iter()
                        .map(Vec::len)
                        .min()
                        .unwrap_or_default();
                    (0..output_len)
                        .map(|frame_index| {
                            let mut frame = AudioFrame::with_channels(channel_count);
                            for channel in 0..channel_count {
                                frame.samples[channel] = channel_outputs[channel][frame_index];
                            }
                            frame
                        })
                        .collect::<Vec<_>>()
                }
                ChannelSelection::Index(channel) => {
                    let selected_samples = frames
                        .iter()
                        .map(|frame| frame.selected_sample(channel))
                        .collect::<Vec<_>>();
                    target
                        .resamplers
                        .first_mut()
                        .expect("channel resamplers must not be empty")
                        .process(&selected_samples)
                        .into_iter()
                        .map(AudioFrame::mono)
                        .collect::<Vec<_>>()
                }
            };
            let output_samples = output_frames
                .iter()
                .map(|frame| frame.mono_sum())
                .collect::<Vec<_>>();
            if let Some(node_id) = &target.target_node_id {
                record_node_meter(node_id, signal_meter(Some(node_id), &output_samples));
            }
            let samples_len = output_frames.len();
            let max_frames = target.latency_policy.max_frames();
            let target_frames = target.latency_policy.target_frames();
            let Ok(mut buffer) = target.mix_input.buffer.lock() else {
                continue;
            };

            for frame in output_frames {
                buffer.push_back(frame);
            }

            let overflowed = buffer.len() > max_frames;
            if overflowed {
                while buffer.len() > target_frames {
                    buffer.pop_front();
                }
            }
            drop(buffer);

            if overflowed {
                target.latency_policy.on_overflow();
            } else {
                target.latency_policy.on_stable(samples_len);
            }
        }
    }
}

#[derive(Clone)]
struct DynamicLatencyPolicy {
    enabled: bool,
    sample_rate: u32,
    base_target_frames: usize,
    base_max_frames: usize,
    target_frames: usize,
    max_frames: usize,
    target_ceiling_frames: usize,
    max_ceiling_frames: usize,
    step_frames: usize,
    stable_decrease_frames: usize,
    overflows_since_change: u32,
    stable_frames_since_change: usize,
}

impl DynamicLatencyPolicy {
    fn new(
        enabled: bool,
        sample_rate: u32,
        target_frames: usize,
        max_frames: usize,
        max_ceiling_ms: usize,
    ) -> Self {
        let max_ceiling_frames = ms_to_frames(sample_rate, max_ceiling_ms).max(max_frames);
        let policy = Self {
            enabled,
            sample_rate,
            base_target_frames: target_frames,
            base_max_frames: max_frames,
            target_frames,
            max_frames,
            target_ceiling_frames: max_frames,
            max_ceiling_frames,
            step_frames: ms_to_frames(sample_rate, DYNAMIC_LATENCY_STEP_MS).max(1),
            stable_decrease_frames: ms_to_frames(sample_rate, DYNAMIC_LATENCY_STABLE_DECREASE_MS)
                .max(1),
            overflows_since_change: 0,
            stable_frames_since_change: 0,
        };
        record_dynamic_latency_state(sample_rate, target_frames, max_frames);
        policy
    }

    fn target_frames(&self) -> usize {
        self.target_frames
    }

    fn max_frames(&self) -> usize {
        self.max_frames
    }

    fn on_overflow(&mut self) {
        record_dynamic_latency_overflow();
        if !self.enabled {
            return;
        }

        self.overflows_since_change += 1;
        self.stable_frames_since_change = 0;
        if self.overflows_since_change < DYNAMIC_LATENCY_OVERFLOWS_TO_INCREASE {
            return;
        }

        self.overflows_since_change = 0;
        let next_target = (self.target_frames + self.step_frames).min(self.target_ceiling_frames);
        let next_max = (self.max_frames + self.step_frames * 2)
            .min(self.max_ceiling_frames)
            .max(next_target);
        self.apply(next_target, next_max);
    }

    fn on_stable(&mut self, frames: usize) {
        if !self.enabled || self.target_frames == self.base_target_frames {
            return;
        }

        self.stable_frames_since_change = self.stable_frames_since_change.saturating_add(frames);
        if self.stable_frames_since_change < self.stable_decrease_frames {
            return;
        }

        self.stable_frames_since_change = 0;
        let next_target = self
            .target_frames
            .saturating_sub(self.step_frames)
            .max(self.base_target_frames);
        let next_max = self
            .max_frames
            .saturating_sub(self.step_frames * 2)
            .max(self.base_max_frames)
            .max(next_target);
        self.apply(next_target, next_max);
    }

    fn apply(&mut self, next_target: usize, next_max: usize) {
        if next_target == self.target_frames && next_max == self.max_frames {
            return;
        }

        self.target_frames = next_target;
        self.max_frames = next_max;
        DYNAMIC_LATENCY_ADJUSTMENTS.fetch_add(1, Ordering::Relaxed);
        record_dynamic_latency_state(self.sample_rate, self.target_frames, self.max_frames);
    }
}

fn dynamic_latency_enabled() -> bool {
    let env_allows_dynamic_latency = std::env::var("VAM_DYNAMIC_LATENCY")
        .map(|value| !matches!(value.trim(), "0" | "false" | "FALSE" | "off" | "OFF"))
        .unwrap_or(true);

    env_allows_dynamic_latency && DYNAMIC_LATENCY_ENABLED.load(Ordering::Relaxed)
}

fn manual_latency_target_ms() -> u64 {
    sanitize_manual_latency_ms(MANUAL_LATENCY_TARGET_MS.load(Ordering::Relaxed))
}

fn sanitize_manual_latency_ms(latency_ms: u64) -> u64 {
    latency_ms.clamp(MANUAL_LATENCY_MIN_MS, MANUAL_LATENCY_MAX_MS)
}

fn ms_to_frames(sample_rate: u32, milliseconds: usize) -> usize {
    (sample_rate as usize * milliseconds / 1_000).max(1)
}

fn frames_to_ms(sample_rate: u32, frames: usize) -> u64 {
    if sample_rate == 0 {
        return 0;
    }

    ((frames as u64) * 1_000) / sample_rate as u64
}

fn record_dynamic_latency_state(sample_rate: u32, target_frames: usize, max_frames: usize) {
    DYNAMIC_LATENCY_TARGET_MS.store(frames_to_ms(sample_rate, target_frames), Ordering::Relaxed);
    DYNAMIC_LATENCY_MAX_MS.store(frames_to_ms(sample_rate, max_frames), Ordering::Relaxed);
}

fn record_dynamic_latency_overflow() {
    DYNAMIC_LATENCY_OVERFLOW_EVENTS.fetch_add(1, Ordering::Relaxed);
}

fn record_node_meter(node_id: &str, meter: AudioNodeMeter) {
    if node_id.is_empty() {
        return;
    }

    let levels = AUDIO_NODE_LEVELS.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut levels) = levels.try_lock() else {
        return;
    };
    let normalized_level = (meter.level * 3.0).clamp(0.0, 1.0);
    let normalized_bands = meter.bands.map(|band| (band * 4.0).clamp(0.0, 1.0));
    let normalized_waveform = meter.waveform.map(|point| point.clamp(-1.0, 1.0));
    let current = levels.get(node_id).copied().unwrap_or(AudioNodeMeter {
        level: 0.0,
        bands: [0.0; VISUALIZER_FFT_BANDS],
        waveform: [0.0; VISUALIZER_WAVEFORM_POINTS],
    });
    let bands = std::array::from_fn(|index| current.bands[index].max(normalized_bands[index]));
    let waveform = std::array::from_fn(|index| {
        if normalized_waveform[index].abs() > current.waveform[index].abs() {
            normalized_waveform[index]
        } else {
            current.waveform[index]
        }
    });
    levels.insert(
        node_id.to_string(),
        AudioNodeMeter {
            level: current.level.max(normalized_level),
            bands,
            waveform,
        },
    );
}

fn signal_meter(node_id: Option<&str>, samples: &[f32]) -> AudioNodeMeter {
    let fallback = fallback_signal_meter(samples);
    let Some(node_id) = node_id else {
        return fallback;
    };
    if !AUDIO_VISUALIZER_FFT_ENABLED.load(Ordering::Relaxed) {
        let visualizer = AUDIO_VISUALIZER.get_or_init(|| Mutex::new(FftVisualizer::new()));
        let Ok(mut visualizer) = visualizer.try_lock() else {
            return fallback;
        };
        if !visualizer.can_retry_fft() {
            return fallback;
        }
        AUDIO_VISUALIZER_FFT_ENABLED.store(true, Ordering::Relaxed);
    }

    let visualizer = AUDIO_VISUALIZER.get_or_init(|| Mutex::new(FftVisualizer::new()));
    let Ok(mut visualizer) = visualizer.try_lock() else {
        return fallback;
    };
    if !visualizer.can_retry_fft() {
        AUDIO_VISUALIZER_FFT_ENABLED.store(false, Ordering::Relaxed);
        return fallback;
    }

    let started = Instant::now();
    let Some(bands) = visualizer.analyze(node_id, samples) else {
        return fallback;
    };
    let elapsed_us = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
    AUDIO_VISUALIZER_FFT_LAST_US.store(elapsed_us, Ordering::Relaxed);
    if elapsed_us > VISUALIZER_FFT_BUDGET_US {
        visualizer.disable_temporarily();
        AUDIO_VISUALIZER_FFT_ENABLED.store(false, Ordering::Relaxed);
        AUDIO_VISUALIZER_FFT_FALLBACKS.fetch_add(1, Ordering::Relaxed);
        return fallback;
    }

    AudioNodeMeter {
        level: fallback.level,
        bands,
        waveform: fallback.waveform,
    }
}

fn fallback_signal_meter(samples: &[f32]) -> AudioNodeMeter {
    if samples.is_empty() {
        return AudioNodeMeter {
            level: 0.0,
            bands: [0.0; VISUALIZER_FFT_BANDS],
            waveform: [0.0; VISUALIZER_WAVEFORM_POINTS],
        };
    }

    let mut full_square_sum = 0.0;
    let mut low_square_sum = 0.0;
    let mut low_mid_square_sum = 0.0;
    let mut high_mid_square_sum = 0.0;
    let mut high_square_sum = 0.0;
    let mut slow = 0.0;
    let mut fast = 0.0;
    let mut previous = samples[0];

    for &raw_sample in samples {
        let sample = raw_sample.clamp(-1.0, 1.0);
        slow += (sample - slow) * 0.055;
        fast += (sample - fast) * 0.28;

        let low = slow;
        let low_mid = fast - slow;
        let high_mid = sample - fast;
        let high = (sample - previous) * 0.55;
        previous = sample;

        full_square_sum += sample * sample;
        low_square_sum += low * low;
        low_mid_square_sum += low_mid * low_mid;
        high_mid_square_sum += high_mid * high_mid;
        high_square_sum += high * high;
    }

    let count = samples.len() as f32;
    let low = (low_square_sum / count).sqrt();
    let low_mid = (low_mid_square_sum / count).sqrt();
    let high_mid = (high_mid_square_sum / count).sqrt();
    let high = (high_square_sum / count).sqrt();
    AudioNodeMeter {
        level: (full_square_sum / count).sqrt().clamp(0.0, 1.0),
        bands: [
            low,
            (low * 0.65 + low_mid * 0.35),
            low_mid,
            (low_mid * 0.55 + high_mid * 0.45),
            high_mid,
            (high_mid * 0.45 + high * 0.55),
            high,
            high * 0.82,
        ],
        waveform: waveform_points(samples),
    }
}

fn waveform_points(samples: &[f32]) -> [f32; VISUALIZER_WAVEFORM_POINTS] {
    if samples.is_empty() {
        return [0.0; VISUALIZER_WAVEFORM_POINTS];
    }

    std::array::from_fn(|index| {
        let start = index * samples.len() / VISUALIZER_WAVEFORM_POINTS;
        let end = ((index + 1) * samples.len() / VISUALIZER_WAVEFORM_POINTS).max(start + 1);
        let window = &samples[start..end.min(samples.len())];
        let mut positive_peak = 0.0_f32;
        let mut negative_peak = 0.0_f32;
        let mut signed_sum = 0.0_f32;
        for &sample in window {
            let sample = sample.clamp(-1.0, 1.0);
            signed_sum += sample;
            positive_peak = positive_peak.max(sample);
            negative_peak = negative_peak.min(sample);
        }
        let signed_average = signed_sum / window.len() as f32;
        let dominant_peak = if positive_peak.abs() >= negative_peak.abs() {
            positive_peak
        } else {
            negative_peak
        };
        (dominant_peak * 0.72 + signed_average * 0.28).clamp(-1.0, 1.0)
    })
}

impl FftVisualizer {
    fn new() -> Self {
        let mut planner = FftPlanner::<f32>::new();
        Self {
            fft: planner.plan_fft_forward(VISUALIZER_FFT_SIZE),
            buffer: vec![Complex { re: 0.0, im: 0.0 }; VISUALIZER_FFT_SIZE],
            disabled_until: None,
            last_analysis_by_node: HashMap::new(),
        }
    }

    fn can_retry_fft(&mut self) -> bool {
        let Some(disabled_until) = self.disabled_until else {
            return true;
        };
        if Instant::now() >= disabled_until {
            self.disabled_until = None;
            return true;
        }
        false
    }

    fn disable_temporarily(&mut self) {
        self.disabled_until =
            Some(Instant::now() + Duration::from_millis(VISUALIZER_FFT_COOLDOWN_MS));
    }

    fn analyze(&mut self, node_id: &str, samples: &[f32]) -> Option<[f32; VISUALIZER_FFT_BANDS]> {
        if samples.is_empty() {
            return None;
        }

        let now = Instant::now();
        if self
            .last_analysis_by_node
            .get(node_id)
            .is_some_and(|last_analysis| {
                now.duration_since(*last_analysis)
                    < Duration::from_millis(VISUALIZER_FFT_MIN_INTERVAL_MS)
            })
        {
            return None;
        }
        self.last_analysis_by_node.insert(node_id.to_string(), now);

        for sample in &mut self.buffer {
            sample.re = 0.0;
            sample.im = 0.0;
        }

        let copy_len = samples.len().min(VISUALIZER_FFT_SIZE);
        let sample_start = samples.len() - copy_len;
        let buffer_start = VISUALIZER_FFT_SIZE - copy_len;
        for index in 0..copy_len {
            let normalized = index as f32 / (VISUALIZER_FFT_SIZE - 1) as f32;
            let window = 0.5 - 0.5 * (std::f32::consts::TAU * normalized).cos();
            self.buffer[buffer_start + index].re =
                samples[sample_start + index].clamp(-1.0, 1.0) * window;
        }

        self.fft.process(&mut self.buffer);
        Some(self.bands_from_fft())
    }

    fn bands_from_fft(&self) -> [f32; VISUALIZER_FFT_BANDS] {
        let ranges = [
            (1, 3),
            (3, 6),
            (6, 11),
            (11, 20),
            (20, 36),
            (36, 64),
            (64, 112),
            (112, 256),
        ];
        std::array::from_fn(|band_index| {
            let (start, end) = ranges[band_index];
            let mut magnitude_sum = 0.0;
            let mut count = 0usize;
            for bin in start..end.min(VISUALIZER_FFT_SIZE / 2) {
                let value = self.buffer[bin];
                magnitude_sum += (value.re * value.re + value.im * value.im).sqrt();
                count += 1;
            }
            if count == 0 {
                0.0
            } else {
                (magnitude_sum / count as f32 / VISUALIZER_FFT_SIZE as f32)
                    .sqrt()
                    .clamp(0.0, 1.0)
            }
        })
    }
}

fn sanitize_gain(gain: f32) -> f32 {
    if gain.is_finite() {
        gain.clamp(0.0, 4.0)
    } else {
        1.0
    }
}

fn audio_frame_from_samples(samples: impl Iterator<Item = f32>) -> AudioFrame {
    let mut frame = AudioFrame::with_channels(1);
    let mut count = 0usize;
    for sample in samples.take(MAX_ROUTABLE_CHANNELS) {
        frame.samples[count] = sample;
        count += 1;
    }
    match count {
        0 => AudioFrame::default(),
        1 => AudioFrame::mono(frame.samples[0]),
        _ => {
            frame.channels = count;
            frame
        }
    }
}

fn apply_frame_gain(frames: &mut [AudioFrame], gain: f32) {
    let gain = gain.clamp(0.0, 1.0);
    if (gain - 1.0).abs() <= f32::EPSILON {
        return;
    }

    for frame in frames {
        for sample in frame.samples.iter_mut().take(frame.channels) {
            *sample *= gain;
        }
    }
}

fn find_device<I>(
    devices: Result<I, cpal::DevicesError>,
    expected_name: &str,
    kind: &str,
) -> Result<Device, String>
where
    I: Iterator<Item = Device>,
{
    devices
        .map_err(|error| error.to_string())?
        .find(|device| {
            device
                .name()
                .map(|name| name == expected_name)
                .unwrap_or(false)
        })
        .ok_or_else(|| format!("Périphérique {kind} introuvable: {expected_name}"))
}

fn choose_input_stream_config(
    device: &Device,
    preferred_sample_rate: u32,
    min_channels: u16,
) -> Result<(StreamConfig, SampleFormat), String> {
    let preferred_formats = [SampleFormat::F32, SampleFormat::I16, SampleFormat::U16];
    if let Ok(config_ranges) = device.supported_input_configs() {
        let ranges = config_ranges.collect::<Vec<_>>();
        for sample_format in preferred_formats {
            if let Some(config) = ranges.iter().find(|config| {
                config.sample_format() == sample_format
                    && config.channels() >= min_channels
                    && config.min_sample_rate().0 <= preferred_sample_rate
                    && config.max_sample_rate().0 >= preferred_sample_rate
            }) {
                let stream_config: StreamConfig = config
                    .with_sample_rate(SampleRate(preferred_sample_rate))
                    .into();
                return Ok((stream_config, sample_format));
            }
        }
    }

    let default_config = device
        .default_input_config()
        .map_err(|error| error.to_string())?;
    let sample_format = default_config.sample_format();
    Ok((default_config.into(), sample_format))
}

fn choose_output_stream_config(
    device: &Device,
    min_channels: u16,
) -> Result<(StreamConfig, SampleFormat), String> {
    let default_config = device
        .default_output_config()
        .map_err(|error| error.to_string())?;
    let preferred_sample_rate = default_config.sample_rate().0;
    let preferred_formats = [
        default_config.sample_format(),
        SampleFormat::F32,
        SampleFormat::I16,
        SampleFormat::U16,
    ];

    if default_config.channels() >= min_channels {
        let sample_format = default_config.sample_format();
        return Ok((default_config.into(), sample_format));
    }

    if let Ok(config_ranges) = device.supported_output_configs() {
        let ranges = config_ranges.collect::<Vec<_>>();
        for sample_format in preferred_formats {
            if let Some(config) = ranges.iter().find(|config| {
                config.sample_format() == sample_format
                    && config.channels() >= min_channels
                    && config.min_sample_rate().0 <= preferred_sample_rate
                    && config.max_sample_rate().0 >= preferred_sample_rate
            }) {
                let stream_config: StreamConfig = config
                    .with_sample_rate(SampleRate(preferred_sample_rate))
                    .into();
                return Ok((stream_config, sample_format));
            }
        }

        for sample_format in preferred_formats {
            if let Some(config) = ranges.iter().find(|config| {
                config.sample_format() == sample_format && config.channels() >= min_channels
            }) {
                let stream_config: StreamConfig = config.with_max_sample_rate().into();
                return Ok((stream_config, sample_format));
            }
        }
    }

    let sample_format = default_config.sample_format();
    Ok((default_config.into(), sample_format))
}

fn build_input_stream(
    device: &Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    fanout: AudioFanout,
) -> Result<Stream, String> {
    let channels = config.channels as usize;
    let on_error = |error| eprintln!("Erreur stream input CPAL: {error}");

    match sample_format {
        SampleFormat::F32 => device
            .build_input_stream(
                config,
                {
                    let mut fanout = fanout;
                    move |data: &[f32], _| fanout.push_input(data, channels)
                },
                on_error,
                None,
            )
            .map_err(|error| error.to_string()),
        SampleFormat::I16 => device
            .build_input_stream(
                config,
                {
                    let mut fanout = fanout;
                    move |data: &[i16], _| fanout.push_input(data, channels)
                },
                on_error,
                None,
            )
            .map_err(|error| error.to_string()),
        SampleFormat::U16 => device
            .build_input_stream(
                config,
                {
                    let mut fanout = fanout;
                    move |data: &[u16], _| fanout.push_input(data, channels)
                },
                on_error,
                None,
            )
            .map_err(|error| error.to_string()),
        format => Err(format!("Format input non supporté pour le MVP: {format:?}")),
    }
}

fn build_output_stream(
    device: &Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    inputs: Vec<MixInput>,
) -> Result<Stream, String> {
    let channels = config.channels as usize;
    let on_error = |error| eprintln!("Erreur stream output CPAL: {error}");

    match sample_format {
        SampleFormat::F32 => device
            .build_output_stream(
                config,
                move |data: &mut [f32], _| fill_output(data, channels, &inputs),
                on_error,
                None,
            )
            .map_err(|error| error.to_string()),
        SampleFormat::I16 => device
            .build_output_stream(
                config,
                move |data: &mut [i16], _| fill_output(data, channels, &inputs),
                on_error,
                None,
            )
            .map_err(|error| error.to_string()),
        SampleFormat::U16 => device
            .build_output_stream(
                config,
                move |data: &mut [u16], _| fill_output(data, channels, &inputs),
                on_error,
                None,
            )
            .map_err(|error| error.to_string()),
        format => Err(format!(
            "Format output non supporté pour le MVP: {format:?}"
        )),
    }
}

fn spawn_vam_transport_reader(
    stop_reader: Arc<AtomicBool>,
    mut fanout: AudioFanout,
) -> Result<JoinHandle<()>, String> {
    let handle = open_vam_transport()?;
    let reader_thread = thread::Builder::new()
        .name("bad-render-transport-reader".to_string())
        .spawn(move || {
            let _handle = handle;
            let mut read_buffer = vec![0_u8; 16 * 1024];
            let mut frames = Vec::with_capacity(read_buffer.len() / VAM_RENDER_BYTES_PER_FRAME);
            let mut format = VamTransportFormat::default();
            let mut endpoint_volume = EndpointVolumeCache::new();

            while !stop_reader.load(Ordering::Acquire) {
                match read_vam_render_bytes(_handle.0, &mut read_buffer) {
                    Ok(0) => thread::sleep(Duration::from_millis(2)),
                    Ok(bytes_read) => {
                        if let Ok(status) = read_vam_transport_status(_handle.0) {
                            let next_format = VamTransportFormat::from_status(status);
                            if next_format != format {
                                format = next_format;
                                fanout.reset_resamplers(format.sample_rate);
                                frames = Vec::with_capacity(
                                    read_buffer.len() / format.bytes_per_frame.max(1),
                                );
                            }
                        }

                        frames.clear();
                        decode_vam_render_bytes(&read_buffer[..bytes_read], format, &mut frames);
                        apply_frame_gain(&mut frames, endpoint_volume.current_gain());
                        fanout.push_frames(&frames);
                    }
                    Err(error) => {
                        eprintln!("Erreur transport BAD: {error}");
                        thread::sleep(Duration::from_millis(20));
                    }
                }
            }
        })
        .map_err(|error| error.to_string())?;

    Ok(reader_thread)
}

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct ProcessLoopbackActivationHandler {
    sender: Mutex<Option<mpsc::Sender<Result<IAudioClient, String>>>>,
}

#[allow(non_snake_case)]
impl IActivateAudioInterfaceCompletionHandler_Impl for ProcessLoopbackActivationHandler_Impl {
    fn ActivateCompleted(
        &self,
        activateoperation: Ref<IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        let result = unsafe {
            let operation = activateoperation.ok()?;
            let mut activate_result = HRESULT(0);
            let mut activated = None;
            operation.GetActivateResult(&mut activate_result, &mut activated)?;
            if activate_result.is_ok() {
                activated
                    .ok_or_else(|| "Activation process loopback sans interface.".to_string())
                    .and_then(|interface| {
                        interface
                            .cast::<IAudioClient>()
                            .map_err(|error| error.to_string())
                    })
            } else {
                Err(format!(
                    "Activation process loopback refusée: HRESULT 0x{:08X}",
                    activate_result.0 as u32
                ))
            }
        };

        if let Ok(mut sender) = self.sender.lock() {
            if let Some(sender) = sender.take() {
                let _ = sender.send(result);
            }
        }

        Ok(())
    }
}

fn spawn_process_loopback_reader(
    stop_reader: Arc<AtomicBool>,
    process_id: u32,
    mut fanout: AudioFanout,
) -> Result<JoinHandle<()>, String> {
    let reader_thread = thread::Builder::new()
        .name(format!("process-loopback-{process_id}"))
        .spawn(move || {
            let _timer_period = WindowsTimerPeriod::new(1);
            if let Err(error) = run_process_loopback_reader(process_id, stop_reader, &mut fanout) {
                eprintln!("Erreur capture processus {process_id}: {error}");
            }
        })
        .map_err(|error| error.to_string())?;

    Ok(reader_thread)
}

fn run_process_loopback_reader(
    process_id: u32,
    stop_reader: Arc<AtomicBool>,
    fanout: &mut AudioFanout,
) -> Result<(), String> {
    initialize_com_for_audio();
    let audio_client = activate_process_loopback_client(process_id)?;
    let wave_format = process_loopback_wave_format();
    fanout.reset_resamplers(PROCESS_LOOPBACK_SAMPLE_RATE);

    unsafe {
        audio_client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
                PROCESS_LOOPBACK_BUFFER_HNS,
                0,
                &wave_format as *const WAVEFORMATEX,
                None,
            )
            .map_err(|error| format!("Initialisation process loopback impossible: {error}"))?;

        let capture_client: IAudioCaptureClient = audio_client
            .GetService()
            .map_err(|error| format!("Capture client process loopback indisponible: {error}"))?;

        audio_client
            .Start()
            .map_err(|error| format!("Démarrage process loopback impossible: {error}"))?;

        let mut frames = Vec::new();
        while !stop_reader.load(Ordering::Acquire) {
            let mut packet_size = capture_client.GetNextPacketSize().map_err(|error| {
                format!("Lecture taille paquet process loopback impossible: {error}")
            })?;

            if packet_size == 0 {
                thread::sleep(Duration::from_millis(2));
                continue;
            }

            while packet_size > 0 {
                let mut data = null_mut::<u8>();
                let mut frames_available = 0_u32;
                let mut flags = 0_u32;
                capture_client
                    .GetBuffer(&mut data, &mut frames_available, &mut flags, None, None)
                    .map_err(|error| {
                        format!("Lecture buffer process loopback impossible: {error}")
                    })?;

                frames.clear();
                decode_process_loopback_buffer(data, frames_available, flags, &mut frames);
                capture_client
                    .ReleaseBuffer(frames_available)
                    .map_err(|error| {
                        format!("Release buffer process loopback impossible: {error}")
                    })?;

                if !frames.is_empty() {
                    fanout.push_frames(&frames);
                }

                packet_size = capture_client.GetNextPacketSize().map_err(|error| {
                    format!("Lecture paquet suivant process loopback impossible: {error}")
                })?;
            }
        }

        let _ = audio_client.Stop();
    }

    Ok(())
}

fn initialize_com_for_audio() {
    let init_result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if init_result.is_err() {
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    }
}

fn activate_process_loopback_client(process_id: u32) -> Result<IAudioClient, String> {
    initialize_com_for_audio();
    let params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: process_id,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };
    let propvariant = ManuallyDrop::new(PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_BLOB,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    blob: BLOB {
                        cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                        pBlobData: &params as *const AUDIOCLIENT_ACTIVATION_PARAMS as *mut u8,
                    },
                },
            }),
        },
    });

    let (sender, receiver) = mpsc::channel();
    let completion_handler: IActivateAudioInterfaceCompletionHandler =
        ProcessLoopbackActivationHandler {
            sender: Mutex::new(Some(sender)),
        }
        .into();

    let _operation = unsafe {
        ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&*propvariant),
            &completion_handler,
        )
        .map_err(|error| {
            format!("ActivateAudioInterfaceAsync process loopback impossible: {error}")
        })?
    };

    receiver
        .recv_timeout(Duration::from_secs(3))
        .map_err(|_| "Timeout activation process loopback.".to_string())?
}

fn process_loopback_wave_format() -> WAVEFORMATEX {
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: PROCESS_LOOPBACK_CHANNELS as u16,
        nSamplesPerSec: PROCESS_LOOPBACK_SAMPLE_RATE,
        nAvgBytesPerSec: PROCESS_LOOPBACK_SAMPLE_RATE
            * PROCESS_LOOPBACK_CHANNELS as u32
            * (PROCESS_LOOPBACK_BITS_PER_SAMPLE as u32 / 8),
        nBlockAlign: (PROCESS_LOOPBACK_CHANNELS * (PROCESS_LOOPBACK_BITS_PER_SAMPLE as usize / 8))
            as u16,
        wBitsPerSample: PROCESS_LOOPBACK_BITS_PER_SAMPLE,
        cbSize: 0,
    }
}

fn decode_process_loopback_buffer(
    data: *const u8,
    frames_available: u32,
    flags: u32,
    frames: &mut Vec<AudioFrame>,
) {
    let frame_count = frames_available as usize;
    if frame_count == 0 {
        return;
    }

    frames.reserve(frame_count);
    if data.is_null() || (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 {
        frames
            .extend((0..frame_count).map(|_| AudioFrame::with_channels(PROCESS_LOOPBACK_CHANNELS)));
        return;
    }

    let sample_count = frame_count * PROCESS_LOOPBACK_CHANNELS;
    let samples = unsafe { std::slice::from_raw_parts(data.cast::<i16>(), sample_count) };
    for sample_frame in samples.chunks_exact(PROCESS_LOOPBACK_CHANNELS) {
        let mut frame = AudioFrame::with_channels(PROCESS_LOOPBACK_CHANNELS);
        frame.samples[0] = sample_frame[0] as f32 / i16::MAX as f32;
        frame.samples[1] = sample_frame[1] as f32 / i16::MAX as f32;
        frames.push(frame);
    }
}

struct WavProbeWriter {
    file: File,
    data_bytes: u32,
}

impl WavProbeWriter {
    fn create(output_path: &str, format: WAVEFORMATEX) -> Result<Self, String> {
        let mut file = File::create(output_path)
            .map_err(|error| format!("Création WAV probe impossible: {error}"))?;
        write_wav_header(&mut file, format, 0)?;
        Ok(Self {
            file,
            data_bytes: 0,
        })
    }

    fn write_audio(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.file
            .write_all(bytes)
            .map_err(|error| format!("Écriture WAV probe impossible: {error}"))?;
        self.data_bytes = self.data_bytes.saturating_add(bytes.len() as u32);
        Ok(())
    }

    fn write_silence(&mut self, byte_count: usize) -> Result<(), String> {
        const SILENCE_CHUNK: [u8; 4096] = [0; 4096];
        let mut remaining = byte_count;
        while remaining > 0 {
            let chunk = remaining.min(SILENCE_CHUNK.len());
            self.write_audio(&SILENCE_CHUNK[..chunk])?;
            remaining -= chunk;
        }
        Ok(())
    }

    fn finalize(mut self) -> Result<(), String> {
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|error| format!("Seek WAV probe impossible: {error}"))?;
        write_wav_header(
            &mut self.file,
            process_loopback_wave_format(),
            self.data_bytes,
        )?;
        self.file
            .flush()
            .map_err(|error| format!("Flush WAV probe impossible: {error}"))
    }
}

fn write_wav_header(file: &mut File, format: WAVEFORMATEX, data_bytes: u32) -> Result<(), String> {
    let riff_size = 36_u32.saturating_add(data_bytes);
    file.write_all(b"RIFF")
        .and_then(|_| file.write_all(&riff_size.to_le_bytes()))
        .and_then(|_| file.write_all(b"WAVE"))
        .and_then(|_| file.write_all(b"fmt "))
        .and_then(|_| file.write_all(&16_u32.to_le_bytes()))
        .and_then(|_| file.write_all(&format.wFormatTag.to_le_bytes()))
        .and_then(|_| file.write_all(&format.nChannels.to_le_bytes()))
        .and_then(|_| file.write_all(&format.nSamplesPerSec.to_le_bytes()))
        .and_then(|_| file.write_all(&format.nAvgBytesPerSec.to_le_bytes()))
        .and_then(|_| file.write_all(&format.nBlockAlign.to_le_bytes()))
        .and_then(|_| file.write_all(&format.wBitsPerSample.to_le_bytes()))
        .and_then(|_| file.write_all(b"data"))
        .and_then(|_| file.write_all(&data_bytes.to_le_bytes()))
        .map_err(|error| format!("Header WAV probe impossible: {error}"))
}

fn spawn_vam_capture_writer(
    stop_reader: Arc<AtomicBool>,
    inputs: Vec<MixInput>,
) -> Result<JoinHandle<()>, String> {
    let handle = open_vam_transport_write()?;
    let writer_thread = thread::Builder::new()
        .name("vam-capture-transport-writer".to_string())
        .spawn(move || {
            let _timer_period = WindowsTimerPeriod::new(1);
            let _handle = handle;
            let mut samples = vec![0.0_f32; VAM_CAPTURE_FRAMES_PER_WRITE * VAM_CAPTURE_CHANNELS];
            let mut bytes = Vec::with_capacity(samples.len() * size_of::<i16>());
            let write_interval = Duration::from_secs_f64(
                VAM_CAPTURE_FRAMES_PER_WRITE as f64 / VAM_CAPTURE_SAMPLE_RATE as f64,
            );
            let mut next_write_at = Instant::now();

            while !stop_reader.load(Ordering::Acquire) {
                fill_vam_capture_samples(&mut samples, &inputs);
                bytes.clear();
                for sample in &samples {
                    bytes.extend_from_slice(&i16::from_f32(*sample).to_le_bytes());
                }

                if let Err(error) = write_vam_capture_bytes(_handle.0, &bytes) {
                    eprintln!("Erreur écriture VAM Entrée: {error}");
                    thread::sleep(Duration::from_millis(20));
                } else {
                    next_write_at += write_interval;
                    let now = Instant::now();
                    if next_write_at > now {
                        thread::sleep(next_write_at - now);
                    } else {
                        let late = now.duration_since(next_write_at);
                        record_vam_capture_writer_late(late);
                        if late > Duration::from_millis(50) {
                            next_write_at = now;
                        }
                    }
                }
            }
        })
        .map_err(|error| error.to_string())?;

    Ok(writer_thread)
}

fn open_vam_transport() -> Result<OwnedHandle, String> {
    let handle = unsafe {
        CreateFileW(
            w!("\\\\.\\VAMAudioTransport"),
            FILE_GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|error| {
        format!(
            "Transport driver VAM introuvable (`\\\\.\\VAMAudioTransport`): {error}. Réinstalle le driver Bubux Audio Driver signé puis relance l'application."
        )
    })?;

    Ok(OwnedHandle(handle))
}

fn open_vam_transport_write() -> Result<OwnedHandle, String> {
    let handle = unsafe {
        CreateFileW(
            w!("\\\\.\\VAMAudioTransport"),
            FILE_GENERIC_WRITE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|error| {
        format!(
            "Transport driver VAM introuvable (`\\\\.\\VAMAudioTransport`): {error}. Réinstalle le driver Bubux Audio Driver signé puis relance l'application."
        )
    })?;

    Ok(OwnedHandle(handle))
}

fn read_vam_render_bytes(handle: HANDLE, buffer: &mut [u8]) -> Result<usize, String> {
    let mut bytes_returned = 0_u32;
    unsafe {
        DeviceIoControl(
            handle,
            IOCTL_VAMAUDIO_READ_RENDER,
            None,
            0,
            Some(buffer.as_mut_ptr().cast()),
            buffer.len() as u32,
            Some(&mut bytes_returned),
            None,
        )
    }
    .map_err(|error| error.to_string())?;

    Ok(bytes_returned as usize)
}

fn write_vam_capture_bytes(handle: HANDLE, buffer: &[u8]) -> Result<usize, String> {
    let mut bytes_returned = 0_u32;
    unsafe {
        DeviceIoControl(
            handle,
            IOCTL_VAMAUDIO_WRITE_CAPTURE,
            Some(buffer.as_ptr().cast()),
            buffer.len() as u32,
            None,
            0,
            Some(&mut bytes_returned),
            None,
        )
    }
    .map_err(|error| error.to_string())?;

    Ok(bytes_returned as usize)
}

fn read_vam_transport_status(handle: HANDLE) -> Result<VamTransportStatus, String> {
    let mut status = VamTransportStatus::default();
    let mut bytes_returned = 0_u32;
    unsafe {
        DeviceIoControl(
            handle,
            IOCTL_VAMAUDIO_GET_STATUS,
            None,
            0,
            Some((&mut status as *mut VamTransportStatus).cast()),
            size_of::<VamTransportStatus>() as u32,
            Some(&mut bytes_returned),
            None,
        )
    }
    .map_err(|error| error.to_string())?;

    if bytes_returned as usize != size_of::<VamTransportStatus>() {
        return Err("Statut transport BAD incomplet.".to_string());
    }

    Ok(status)
}

fn record_vam_capture_writer_late(late: Duration) {
    let late_us = late.as_micros().min(u64::MAX as u128) as u64;
    VAM_CAPTURE_WRITER_LATE_TOTAL_US.fetch_add(late_us, Ordering::Relaxed);
    VAM_CAPTURE_WRITER_LATE_SAMPLES.fetch_add(1, Ordering::Relaxed);

    let mut current_max = VAM_CAPTURE_WRITER_LATE_MAX_US.load(Ordering::Relaxed);
    while late_us > current_max {
        match VAM_CAPTURE_WRITER_LATE_MAX_US.compare_exchange_weak(
            current_max,
            late_us,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(value) => current_max = value,
        }
    }
}

fn fill_vam_capture_samples(samples: &mut [f32], inputs: &[MixInput]) {
    let mut locked_inputs = Vec::with_capacity(inputs.len());
    for input in inputs {
        if let Ok(buffer) = input.buffer.lock() {
            locked_inputs.push((input.gain, input.target_channel, buffer));
        }
    }

    for frame in samples.chunks_mut(VAM_CAPTURE_CHANNELS) {
        let mut output_frame = AudioFrame::with_channels(VAM_CAPTURE_CHANNELS);
        for (gain, target_channel, buffer) in &mut locked_inputs {
            mix_frame(
                &mut output_frame,
                buffer.pop_front().unwrap_or_default(),
                *gain,
                *target_channel,
            );
        }

        for (channel_index, channel) in frame.iter_mut().enumerate() {
            *channel = output_frame.selected_sample(channel_index).clamp(-1.0, 1.0);
        }
    }
}

fn decode_vam_render_bytes(data: &[u8], format: VamTransportFormat, frames: &mut Vec<AudioFrame>) {
    match format.bits_per_sample {
        16 => {
            for frame in data.chunks_exact(format.bytes_per_frame) {
                frames.push(audio_frame_from_samples(
                    frame.chunks_exact(2).take(format.channels).map(|sample| {
                        i16::from_le_bytes([sample[0], sample[1]]) as f32 / i16::MAX as f32
                    }),
                ));
            }
        }
        32 => {
            for frame in data.chunks_exact(format.bytes_per_frame) {
                frames.push(audio_frame_from_samples(
                    frame.chunks_exact(4).take(format.channels).map(|sample| {
                        i32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]) as f32
                            / i32::MAX as f32
                    }),
                ));
            }
        }
        _ => {}
    }
}

struct OwnedHandle(HANDLE);

unsafe impl Send for OwnedHandle {}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

struct WindowsTimerPeriod {
    period_ms: u32,
    active: bool,
}

impl WindowsTimerPeriod {
    fn new(period_ms: u32) -> Self {
        let active = unsafe { timeBeginPeriod(period_ms) } == 0;
        Self { period_ms, active }
    }
}

impl Drop for WindowsTimerPeriod {
    fn drop(&mut self) {
        if self.active {
            let _ = unsafe { timeEndPeriod(self.period_ms) };
        }
    }
}

fn is_virtual_driver_output_name(name: &str) -> bool {
    name.contains("VAM Sortie")
        || name.contains("Bubux Audio Driver")
        || name.contains("VirtualAudioMix Sortie")
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct VamTransportStatus {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VamTransportFormat {
    sample_rate: u32,
    channels: usize,
    bits_per_sample: u16,
    bytes_per_frame: usize,
}

impl Default for VamTransportFormat {
    fn default() -> Self {
        Self {
            sample_rate: VAM_RENDER_SAMPLE_RATE,
            channels: VAM_RENDER_CHANNELS,
            bits_per_sample: VAM_RENDER_BITS_PER_SAMPLE,
            bytes_per_frame: VAM_RENDER_BYTES_PER_FRAME,
        }
    }
}

impl VamTransportFormat {
    fn from_status(status: VamTransportStatus) -> Self {
        let fallback = Self::default();
        let sample_rate = if status.sample_rate > 0 {
            status.sample_rate
        } else {
            fallback.sample_rate
        };
        let channels = if status.channels > 0 {
            status.channels as usize
        } else {
            fallback.channels
        };
        let bits_per_sample = match status.bits_per_sample {
            16 | 32 => status.bits_per_sample,
            _ => fallback.bits_per_sample,
        };
        let bytes_per_frame = if status.block_align > 0 {
            status.block_align as usize
        } else {
            channels * (bits_per_sample as usize / 8)
        };

        Self {
            sample_rate,
            channels,
            bits_per_sample,
            bytes_per_frame,
        }
    }
}

#[derive(Debug, Clone)]
struct InputResampler {
    ratio: f64,
    position: f64,
    previous_sample: Option<f32>,
}

impl InputResampler {
    fn new(input_sample_rate: u32, output_sample_rate: u32) -> Self {
        Self {
            ratio: input_sample_rate as f64 / output_sample_rate as f64,
            position: 0.0,
            previous_sample: None,
        }
    }

    fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }

        if (self.ratio - 1.0).abs() < f64::EPSILON {
            self.previous_sample = input.last().copied();
            return input.to_vec();
        }

        let mut source = Vec::with_capacity(input.len() + 1);
        if let Some(previous_sample) = self.previous_sample {
            source.push(previous_sample);
        }
        source.extend_from_slice(input);

        if source.len() < 2 {
            self.previous_sample = source.last().copied();
            return Vec::new();
        }

        let mut output = Vec::with_capacity(input.len());
        while self.position + 1.0 < source.len() as f64 {
            let index = self.position.floor() as usize;
            let fraction = (self.position - index as f64) as f32;
            let sample = source[index] + (source[index + 1] - source[index]) * fraction;
            output.push(sample);
            self.position += self.ratio;
        }

        self.position -= (source.len() - 1) as f64;
        while self.position < 0.0 {
            self.position += self.ratio;
        }
        self.previous_sample = source.last().copied();
        output
    }
}

fn fill_output<T: AudioSample>(data: &mut [T], channels: usize, inputs: &[MixInput]) {
    let mut locked_inputs = Vec::with_capacity(inputs.len());
    for input in inputs {
        if let Ok(buffer) = input.buffer.lock() {
            locked_inputs.push((input.gain, input.target_channel, buffer));
        }
    }

    for frame in data.chunks_mut(channels.max(1)) {
        let mut output_frame = AudioFrame::with_channels(channels.max(1));
        for (gain, target_channel, buffer) in &mut locked_inputs {
            mix_frame(
                &mut output_frame,
                buffer.pop_front().unwrap_or_default(),
                *gain,
                *target_channel,
            );
        }

        for (channel_index, output) in frame.iter_mut().enumerate() {
            *output = T::from_f32(output_frame.selected_sample(channel_index));
        }
    }
}

fn mix_frame(
    output: &mut AudioFrame,
    input: AudioFrame,
    gain: f32,
    target_channel: ChannelSelection,
) {
    match target_channel {
        ChannelSelection::All => {
            let channels = output
                .channels
                .min(input.channels)
                .min(MAX_ROUTABLE_CHANNELS);
            for channel in 0..channels {
                output.samples[channel] = (output.samples[channel]
                    + input.selected_sample(channel) * gain)
                    .clamp(-1.0, 1.0);
            }
        }
        ChannelSelection::Index(channel) => {
            if channel < output.channels && channel < MAX_ROUTABLE_CHANNELS {
                output.samples[channel] =
                    (output.samples[channel] + input.mono_sum() * gain).clamp(-1.0, 1.0);
            }
        }
    }
}

trait AudioSample: Copy {
    fn to_f32(&self) -> f32;
    fn from_f32(sample: f32) -> Self;
}

impl AudioSample for f32 {
    fn to_f32(&self) -> f32 {
        *self
    }

    fn from_f32(sample: f32) -> Self {
        sample.clamp(-1.0, 1.0)
    }
}

impl AudioSample for i16 {
    fn to_f32(&self) -> f32 {
        *self as f32 / i16::MAX as f32
    }

    fn from_f32(sample: f32) -> Self {
        (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
    }
}

impl AudioSample for u16 {
    fn to_f32(&self) -> f32 {
        (*self as f32 / u16::MAX as f32) * 2.0 - 1.0
    }

    fn from_f32(sample: f32) -> Self {
        (((sample.clamp(-1.0, 1.0) + 1.0) * 0.5) * u16::MAX as f32) as u16
    }
}
