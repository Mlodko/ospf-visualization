use std::{collections::HashSet, net::Ipv4Addr};

use ipnetwork::IpNetwork;
use itertools::Itertools;
use petgraph::{Undirected, csr::NodeIndex};
use rand::{SeedableRng, seq::IteratorRandom};

use crate::{edge::Edge, node::{Node, RouterNode, RouterType}};

pub struct GraphConfig {
    /// Number of core routers in the network, i.e. a complete graph representing the OSPF backbone area
    pub core_routers_count: usize,
    /// Number of distribution routers in the network, i.e. routers connected to the core routers
    pub distribution_routers_count: usize,
    /// Number of access routers in the network, i.e. routers connected to the distribution routers
    pub access_routers_count: usize,
    /// Number of random links every distribution and access router starts with
    pub attachment_factor: usize,
    /// Probability of creating a triangle
    pub clustering_factor: f32,
    /// Minimum number of edges for turning a router into a multiaccess network
    pub multiaccess_network_edge_count: usize,
    /// Probability of turning a router with >= multiaccess_network_edge_count edges into a multiaccess network
    pub multiaccess_network_factor: f32,
    /// Function to generate a cost for an edge
    pub cost_distribution: fn(f32) -> f32,
}

impl GraphConfig {
    pub fn total_router_count(&self) -> usize {
        self.core_routers_count + self.distribution_routers_count + self.access_routers_count
    }
    
    pub fn new_from_percentages(
        total_router_count: usize,
        core_percentage: f32,
        distribution_percentage: f32,
        access_percentage: f32,
        attachment_factor: usize,
        clustering_factor: f32,
        multiaccess_network_edge_count: usize,
        multiaccess_network_factor: f32,
        cost_distribution: fn(f32) -> f32,
    ) -> Self {
        let core_routers_count = (total_router_count as f32 * core_percentage) as usize;
        let distribution_routers_count = (total_router_count as f32 * distribution_percentage) as usize;
        let access_routers_count = (total_router_count as f32 * access_percentage) as usize;

        Self {
            core_routers_count,
            distribution_routers_count,
            access_routers_count,
            attachment_factor,
            clustering_factor,
            multiaccess_network_edge_count,
            multiaccess_network_factor,
            cost_distribution,
        }
    }
    
    pub fn new(
        core_routers_count: usize,
        distribution_routers_count: usize,
        access_routers_count: usize,
        attachment_factor: usize,
        clustering_factor: f32,
        multiaccess_network_edge_count: usize,
        multiaccess_network_factor: f32,
        cost_distribution: fn(f32) -> f32,
    ) -> Self {
        Self {
            core_routers_count,
            distribution_routers_count,
            access_routers_count,
            attachment_factor,
            clustering_factor,
            multiaccess_network_edge_count,
            multiaccess_network_factor,
            cost_distribution,
        }
    }
}

#[derive(Default)]
pub struct TopoGraph {
    pub graph: petgraph::prelude::Graph<Node, Edge>
}

impl TopoGraph {
    pub fn generate(config: GraphConfig, seed: u64) -> Self {
        let mut graph = petgraph::Graph::<Node, Edge>::new();
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

        // Generate core, reserve 10.0.0.0/8 for core routers
        let core_router_indices: Vec<_> = std::iter::repeat_with(|| Node::new_router(RouterType::Core))
            .take(config.core_routers_count)
            .map(|r| {
                graph.add_node(r)
            })
            .collect();
        
        {
            let core_router_pairs = core_router_indices.iter().enumerate().combinations(2);
            
            for pair in core_router_pairs.into_iter() {
                let (i1, gidx1) = pair[0];
                let (i2, gidx2) = pair[1];
                
                let prefix = if let IpNetwork::V4(prefix) = core_pair_prefix(i1, i2, config.core_routers_count) {
                    prefix
                } else {
                    panic!("Unexpected IP version");
                };
                
                let ip1 = prefix.nth(1).unwrap();
                let ip2 = prefix.nth(2).unwrap();
                {
                    let r1 = if let Node::Router(r) = graph.node_weight_mut(*gidx1).unwrap() {
                        r
                    } else {
                        panic!("Unexpected node type");
                    };
                    r1.add_interface(ip1);
                }
                {
                    let r2 = if let Node::Router(r) = graph.node_weight_mut(*gidx2).unwrap() {
                        r
                    } else {
                        panic!("Unexpected node type");
                    };
                    r2.add_interface(ip2);
                }
                
                let edge = Edge {
                    metric: 10.0
                };
                
                graph.add_edge(*gidx1, *gidx2, edge);
            }
        }
        
        {
            let mut distribution_routers = std::iter::repeat_with(|| Node::new_router(RouterType::Distribution))
                .take(config.distribution_routers_count)
                .collect::<Vec<_>>();
            let mut core_router_pairs: HashSet<_> = core_router_indices.iter().combinations(2).collect();
            
            
            // Reserve 20.0.0.0/8 for distribution routers
            // Format: 20.{dist_router_idx}.1-2.0/24
            for (i, dist_node) in distribution_routers.iter_mut().enumerate() {
                let dist_router = dist_node.router_mut().unwrap();
                // Select random core router pair to connect to, pop it from the set
                let core_pair = core_router_pairs.iter().choose(&mut rng).expect("Could not find a core router pair").clone();
                core_router_pairs.remove(&core_pair);
                let ip_dist_1: Ipv4Addr = Ipv4Addr::new(20, i as u8, 1, 2);
                let ip_dist_2: Ipv4Addr = Ipv4Addr::new(20, i as u8, 2, 2);
                
                dist_router.add_interface(ip_dist_1);
                dist_router.add_interface(ip_dist_2);
                
                {
                    let ip_core_1 = Ipv4Addr::new(20, i as u8, 1, 1);
                    
                    let core_router_1 = graph.node_weight_mut(*core_pair[0]).unwrap().router_mut().unwrap();
                    core_router_1.add_interface(ip_core_1);
                }
                
                {
                    let ip_core_2 = Ipv4Addr::new(20, i as u8, 2, 1);
                    
                    let core_router_2 = graph.node_weight_mut(*core_pair[1]).unwrap().router_mut().unwrap();
                    core_router_2.add_interface(ip_core_2);
                }
                
                let dist_router_idx = graph.add_node(dist_node.clone());
                
                graph.add_edge(dist_router_idx, *core_pair[0], Edge::new(10.0));
                graph.add_edge(dist_router_idx, *core_pair[1], Edge::new(10.0));
            }
        }

        Self { graph }
    }
}

fn nth_core_p2p_prefix(n: u32) -> IpNetwork {
    // 10.0.0.0
    let base = 0x0A_00_00_00 as u32;
    
    let addr_u32 = base + (n << 2);
    let addr = Ipv4Addr::from(addr_u32);
    IpNetwork::new(std::net::IpAddr::V4(addr), 30).expect("Invalid /30 prefix")
}

/// Deterministic index for an undirected router pair (i, j) with i != j, i, j < n.
/// Maps the pair to a unique 0-based edge index among all C(n, 2) pairs.
/// This is lexicographic over i < j.
fn undirected_pair_index(i: usize, j: usize, n: usize) -> u32 {
    assert!(i < n && j < n && i != j, "pair indices out of range");
    let (a, b) = if i < j { (i as u64, j as u64) } else { (j as u64, i as u64) };
    let n = n as u64;

    let offset = a * (2 * n - a - 1) / 2;
    let idx = offset + (b - a - 1);
    // Safe as long as C(n,2) <= 4_194_304; i.e., n <= 2896
    idx as u32
}

/// Convenience: compute the /30 for a core pair directly.
fn core_pair_prefix(i: usize, j: usize, core_count: usize) -> IpNetwork {
    let ix = undirected_pair_index(i, j, core_count);
    nth_core_p2p_prefix(ix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    
    #[test]
    fn nth_prefix_monotonicity() {
        for n in 0..10_000u32 {
            let net = nth_core_p2p_prefix(n);
            assert_eq!(net.prefix(), 30);
            assert!(matches!(net, IpNetwork::V4(_)));
            // Monotonic base address increments by 4
            if n > 0 {
                let prev = nth_core_p2p_prefix(n - 1);
                let a = if let IpNetwork::V4(addr) = net {
                    addr.network().to_bits()
                } else {
                    panic!("Unexpected network type")
                };
                let b = if let IpNetwork::V4(addr) = prev {
                    addr.network().to_bits()
                } else {
                    panic!("Unexpected network type")
                };
                assert_eq!(a - b, 4);
            }
        }
    }

    #[test]
    fn test_core_generation() {
        let config = GraphConfig::new(
            20,
            0,
            0,
            0,
            0.0,
            0,
            0.0,
            |_| 0.0,
        );
        
        let graph = TopoGraph::generate(config, 0);
        
        let nodes: Vec<_> = graph.graph.node_weights().collect();
        
        assert!(nodes.len() == 20);
        
        assert!(nodes.iter().all(|node| matches!(node, Node::Router(_))));
        
        let routers: Vec<_> = nodes.iter().filter_map(|node| node.router()).collect();
        
        assert!(routers.iter().all(|router| router.interfaces.len() >= 19));
    }
    
    #[test]
    fn pair_indexing_is_unique_and_symmetric() {
        let n = 1000usize;
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        let total = n * (n - 1) / 2;
        let mut counter = 0;
        let mut handle = std::io::stdout().lock();
        for i in 0..n {
            for j in (i + 1)..n {
                counter += 1;
                write!(handle, "{} / {}\r", counter, total).unwrap();
                handle.flush().unwrap();
                let a = undirected_pair_index(i, j, n);
                let b = undirected_pair_index(j, i, n);
                assert_eq!(a, b);
                assert!(seen.insert(a));
            }
        }
        assert_eq!(seen.len(), n * (n - 1) / 2);
    }
}