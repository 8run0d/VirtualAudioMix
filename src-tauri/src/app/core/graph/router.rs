use super::edge::AudioEdge;
use super::graph::AudioGraph;

#[derive(Debug, Default)]
pub struct GraphRouter;

impl GraphRouter {
    pub fn active_edges<'a>(&self, graph: &'a AudioGraph) -> impl Iterator<Item = &'a AudioEdge> {
        graph.edges.values().filter(|edge| edge.gain > 0.0)
    }
}
