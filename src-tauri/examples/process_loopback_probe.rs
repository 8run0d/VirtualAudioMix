use std::{env, time::Duration};

use virtual_audio_mix_lib::app::platform::direct_route::capture_process_loopback_wav;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let process_id = args
        .next()
        .ok_or_else(|| "Usage: process_loopback_probe <pid> <output.wav> [seconds]".to_string())?
        .parse::<u32>()
        .map_err(|error| format!("PID invalide: {error}"))?;
    let output_path = args
        .next()
        .ok_or_else(|| "Usage: process_loopback_probe <pid> <output.wav> [seconds]".to_string())?;
    let seconds = args
        .next()
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|error| format!("Durée invalide: {error}"))
        })
        .transpose()?
        .unwrap_or(5);

    let bytes = capture_process_loopback_wav(
        process_id,
        &output_path,
        Duration::from_secs(seconds.clamp(1, 60)),
    )?;
    println!("Probe process loopback terminé: {bytes} octets écrits dans {output_path}");
    Ok(())
}
