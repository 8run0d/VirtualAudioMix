#[derive(Debug, Clone)]
pub struct AudioBuffer {
    channels: usize,
    frames: Vec<f32>,
}

impl AudioBuffer {
    pub fn new(channels: usize, frame_count: usize) -> Self {
        Self {
            channels,
            frames: vec![0.0; channels * frame_count],
        }
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn samples(&self) -> &[f32] {
        &self.frames
    }

    pub fn samples_mut(&mut self) -> &mut [f32] {
        &mut self.frames
    }

    pub fn clear(&mut self) {
        self.frames.fill(0.0);
    }
}
