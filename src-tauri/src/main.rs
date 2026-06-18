#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    virtual_audio_mix_lib::run();
}
