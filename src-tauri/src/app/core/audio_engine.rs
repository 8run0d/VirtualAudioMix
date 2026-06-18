use super::graph::graph::AudioGraph;

#[derive(Debug, Default)]
pub struct AudioEngine {
    graph: AudioGraph,
}

impl AudioEngine {
    pub fn graph(&self) -> &AudioGraph {
        &self.graph
    }

    pub fn graph_mut(&mut self) -> &mut AudioGraph {
        &mut self.graph
    }
}
