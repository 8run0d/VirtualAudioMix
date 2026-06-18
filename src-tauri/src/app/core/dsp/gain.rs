pub fn apply_gain(samples: &mut [f32], gain: f32) {
    for sample in samples {
        *sample *= gain;
    }
}
