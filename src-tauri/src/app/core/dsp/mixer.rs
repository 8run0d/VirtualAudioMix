pub fn mix_into(destination: &mut [f32], source: &[f32], gain: f32) {
    for (output, input) in destination.iter_mut().zip(source.iter()) {
        *output += input * gain;
    }
}

pub fn peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .fold(0.0, |peak, sample| peak.max(sample.abs()))
}
