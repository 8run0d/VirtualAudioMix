#[derive(Debug, Clone, Copy)]
pub struct ResampleRequest {
    pub source_rate: u32,
    pub target_rate: u32,
}

impl ResampleRequest {
    pub fn is_required(self) -> bool {
        self.source_rate != self.target_rate
    }
}
