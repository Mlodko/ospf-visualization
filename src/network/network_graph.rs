
use petgraph::stable_graph::{StableGraph, NodeIndex};

use crate::network::{node::Node, edge::Edge};

pub struct NetworkGraph {
    graph: StableGraph<Node, Edge>
}