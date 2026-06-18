use virtual_audio_mix_lib::app::platform::virtual_driver;

fn main() -> Result<(), String> {
    let status = virtual_driver::get_transport_status()?;
    println!("{status:#?}");
    Ok(())
}
