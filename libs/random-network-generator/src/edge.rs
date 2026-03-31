use petgraph::csr::EdgeIndex;


pub struct Edge {
    pub metric: f32
}

impl Edge {
    pub fn new(metric: f32) -> Self {
        Edge { metric }
    }
}
