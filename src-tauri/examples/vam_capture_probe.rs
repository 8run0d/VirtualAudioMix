use std::{
    f32::consts::TAU,
    mem::size_of,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleFormat, SampleRate, Stream, StreamConfig, SupportedStreamConfig,
};
use windows::{
    core::w,
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE, FILE_SHARE_READ,
            FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::IO::DeviceIoControl,
    },
};

const IOCTL_VAMAUDIO_WRITE_CAPTURE: u32 = 0x0022_A008;
const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: usize = 2;
const FRAMES_PER_WRITE: usize = 240;
const TEST_FREQUENCY_HZ: f32 = 440.0;

fn main() -> Result<(), String> {
    let host = cpal::default_host();
    let input_device = host
        .input_devices()
        .map_err(|error| error.to_string())?
        .find(|device| {
            device
                .name()
                .map(|name| {
                    name.contains("VAM Entrée")
                        || name.contains("VAM Entree")
                        || name.contains("VAM IN")
                        || name.contains("Bubux Audio Driver")
                })
                .unwrap_or(false)
        })
        .ok_or_else(|| "Entrée VAM introuvable via CPAL.".to_string())?;

    let input_name = input_device.name().map_err(|error| error.to_string())?;
    let supported_config = choose_input_config(&input_device)?;
    let stream_config: StreamConfig = supported_config.clone().into();
    println!(
        "Capture: {input_name} | {:?} | {} Hz | {} ch",
        supported_config.sample_format(),
        stream_config.sample_rate.0,
        stream_config.channels
    );

    let stats = Arc::new(Mutex::new(Stats::default()));
    let input_stream = build_input_stream(
        &input_device,
        &stream_config,
        supported_config.sample_format(),
        Arc::clone(&stats),
    )?;
    input_stream.play().map_err(|error| error.to_string())?;

    let stop = Arc::new(AtomicBool::new(false));
    let writer_stop = Arc::clone(&stop);
    let writer = thread::Builder::new()
        .name("vam-capture-probe-writer".to_string())
        .spawn(move || write_probe_signal(writer_stop))
        .map_err(|error| error.to_string())?;

    thread::sleep(Duration::from_secs(3));
    stop.store(true, Ordering::Release);
    writer
        .join()
        .map_err(|_| "Writer probe paniqué.".to_string())??;
    drop(input_stream);

    let stats = stats
        .lock()
        .map_err(|_| "Stats verrouillées.".to_string())?;
    println!(
        "Résultat: frames={} rms={:.6} peak={:.6}",
        stats.frames, stats.rms, stats.peak
    );

    if stats.peak > 0.01 {
        println!("OK: VAM Entrée expose le signal écrit dans le driver.");
    } else {
        println!("ECHEC: VAM Entrée reste silencieuse malgré l'écriture driver.");
    }

    Ok(())
}

fn choose_input_config(device: &cpal::Device) -> Result<SupportedStreamConfig, String> {
    let configs = device
        .supported_input_configs()
        .map_err(|error| format!("Formats input VAM illisibles: {error}"))?
        .collect::<Vec<_>>();

    if configs.is_empty() {
        return Err("Aucun format input VAM annoncé.".to_string());
    }

    println!("Formats VAM Entrée annoncés:");
    for config in &configs {
        println!(
            "- {:?} | {} ch | {}..{} Hz",
            config.sample_format(),
            config.channels(),
            config.min_sample_rate().0,
            config.max_sample_rate().0
        );
    }

    configs
        .iter()
        .find(|config| {
            config.sample_format() == SampleFormat::I16
                && config.channels() == CHANNELS as u16
                && config.min_sample_rate().0 <= SAMPLE_RATE
                && config.max_sample_rate().0 >= SAMPLE_RATE
        })
        .or_else(|| {
            configs.iter().find(|config| {
                config.min_sample_rate().0 <= SAMPLE_RATE
                    && config.max_sample_rate().0 >= SAMPLE_RATE
            })
        })
        .map(|config| config.with_sample_rate(SampleRate(SAMPLE_RATE)))
        .ok_or_else(|| "Aucun format input VAM compatible 48 kHz.".to_string())
}

fn write_probe_signal(stop: Arc<AtomicBool>) -> Result<(), String> {
    let handle = open_vam_transport_write()?;
    let _handle = handle;
    let mut phase = 0.0_f32;
    let phase_step = TAU * TEST_FREQUENCY_HZ / SAMPLE_RATE as f32;
    let mut samples = vec![0.0_f32; FRAMES_PER_WRITE * CHANNELS];
    let mut bytes = Vec::with_capacity(samples.len() * size_of::<i16>());

    while !stop.load(Ordering::Acquire) {
        for frame in samples.chunks_mut(CHANNELS) {
            let sample = (phase.sin() * 0.25).clamp(-1.0, 1.0);
            phase = (phase + phase_step) % TAU;
            for channel in frame {
                *channel = sample;
            }
        }

        bytes.clear();
        for sample in &samples {
            let sample = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        write_vam_capture_bytes(_handle.0, &bytes)?;
        thread::sleep(Duration::from_millis(5));
    }

    Ok(())
}

fn build_input_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    stats: Arc<Mutex<Stats>>,
) -> Result<Stream, String> {
    let channels = config.channels as usize;
    let started_at = Instant::now();
    let on_error = |error| eprintln!("Erreur stream input probe: {error}");

    match sample_format {
        SampleFormat::F32 => device
            .build_input_stream(
                config,
                move |data: &[f32], _| update_stats(&stats, data, channels, started_at),
                on_error,
                None,
            )
            .map_err(|error| error.to_string()),
        SampleFormat::I16 => device
            .build_input_stream(
                config,
                move |data: &[i16], _| update_stats(&stats, data, channels, started_at),
                on_error,
                None,
            )
            .map_err(|error| error.to_string()),
        SampleFormat::U16 => device
            .build_input_stream(
                config,
                move |data: &[u16], _| update_stats(&stats, data, channels, started_at),
                on_error,
                None,
            )
            .map_err(|error| error.to_string()),
        format => Err(format!("Format input probe non supporté: {format:?}")),
    }
}

fn update_stats<T: ProbeSample>(
    stats: &Arc<Mutex<Stats>>,
    data: &[T],
    channels: usize,
    started_at: Instant,
) {
    let elapsed = started_at.elapsed().as_secs_f32().max(0.001);
    let mut sum_squares = 0.0_f64;
    let mut peak = 0.0_f32;
    let mut frames = 0_u64;

    for frame in data.chunks(channels.max(1)) {
        let sample = frame.iter().map(ProbeSample::to_f32).sum::<f32>() / frame.len() as f32;
        sum_squares += (sample as f64) * (sample as f64);
        peak = peak.max(sample.abs());
        frames += 1;
    }

    if let Ok(mut stats) = stats.lock() {
        stats.frames += frames;
        stats.peak = stats.peak.max(peak);
        stats.sum_squares += sum_squares;
        stats.rms = (stats.sum_squares / stats.frames.max(1) as f64).sqrt() as f32;

        if stats.last_report.elapsed() >= Duration::from_millis(500) {
            stats.last_report = Instant::now();
            println!(
                "Probe {:.1}s: frames={} rms={:.6} peak={:.6}",
                elapsed, stats.frames, stats.rms, stats.peak
            );
        }
    }
}

trait ProbeSample {
    fn to_f32(&self) -> f32;
}

impl ProbeSample for f32 {
    fn to_f32(&self) -> f32 {
        *self
    }
}

impl ProbeSample for i16 {
    fn to_f32(&self) -> f32 {
        *self as f32 / i16::MAX as f32
    }
}

impl ProbeSample for u16 {
    fn to_f32(&self) -> f32 {
        (*self as f32 - 32768.0) / 32768.0
    }
}

#[derive(Debug)]
struct Stats {
    frames: u64,
    sum_squares: f64,
    rms: f32,
    peak: f32,
    last_report: Instant,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            frames: 0,
            sum_squares: 0.0,
            rms: 0.0,
            peak: 0.0,
            last_report: Instant::now(),
        }
    }
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
    .map_err(|error| format!("Transport BAD introuvable: {error}"))?;

    Ok(OwnedHandle(handle))
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

struct OwnedHandle(HANDLE);

unsafe impl Send for OwnedHandle {}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}
