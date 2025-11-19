use std::collections::HashMap;

use eframe::egui::Color32;
use egui::Pos2;
use egui_graphs::Graph;
use petgraph::{Directed, csr::DefaultIx, graph::NodeIndex, prelude::StableGraph};
use rand::Rng;
use uuid::Uuid;

use crate::{
    gui::node_shape::MyNodeShape,
    network::{
        edge::{Edge, EdgeMetric},
        node::{Node, NodeInfo},
        router::RouterId,
    },
};

const IF_SKIP_FUNCTIONALLY_P2P_NETWORKS: bool = false;

/// A protocol-agnostic graph wrapper used by the GUI.
///
/// Builds a graph from Nodes and wires edges based on attached_routers.
/// node_id_to_index_map maps stable UUIDs to graph indices to allow safe lookups.

#[allow(dead_code)]
pub struct NetworkGraph {
    pub graph: Graph<Node, crate::network::edge::Edge, Directed, DefaultIx, MyNodeShape>,
    pub node_id_to_index_map: HashMap<Uuid, NodeIndex>,
}

impl NetworkGraph {
    /// Build a new NetworkGraph from a list of protocol-agnostic nodes.
    /// This method avoids panics by validating lookups and ignores incomplete references.
    pub fn build_new(nodes: Vec<Node>) -> Self {
        let (mut graph, node_id_to_index_map) = {
            let mut graph = StableGraph::new();
            let mut node_id_to_index_map = HashMap::new();
            for node in nodes {
                let id = node.id;
                let index = graph.add_node(node);
                node_id_to_index_map.insert(id, index);
            }
            (graph, node_id_to_index_map)
        };

        // First pass (immutable): collect edges to add so we don't hold an immutable borrow
        // while trying to mutably add edges later.
        let mut edges_to_add: Vec<(NodeIndex, uuid::Uuid, uuid::Uuid)> = Vec::new();
        let mut node_indices_to_remove = Vec::new();
        for net_index in graph.node_indices() {
            if let NodeInfo::Network(network) = &graph[net_index].info {
                let net_id = graph[net_index].id;
                
                // If only 2 routers are attached, connect them directly and remove the network node
                if network.attached_routers.len() == 2 && IF_SKIP_FUNCTIONALLY_P2P_NETWORKS {
                    let router1_id = network.attached_routers[0].to_uuidv5();
                    let router2_id = network.attached_routers[1].to_uuidv5();
                    if let (Some(&router1_index), Some(&router2_index)) = (
                        node_id_to_index_map.get(&router1_id),
                        node_id_to_index_map.get(&router2_id),
                    ) {
                        edges_to_add.push((router1_index, router1_id, router2_id));
                        edges_to_add.push((router2_index, router2_id, router1_id));
                        node_indices_to_remove.push(net_index);
                    }
                    continue;
                }

                // Only make an edge from the router to the network
                for router_node_id in network.attached_routers.iter().map(RouterId::to_uuidv5) {
                    if let Some(&router_index) = node_id_to_index_map.get(&router_node_id) {
                        edges_to_add.push((router_index, router_node_id, net_id));
                    }
                }
            }
        }

        for node_index in node_indices_to_remove {
            let _ = graph.remove_node(node_index);
        }

        // Second pass (mutable): add collected edges, checking that destination nodes exist.
        for (src_index, src_id, dest_id) in edges_to_add {
            if let Some(router_index) = node_id_to_index_map.get(&dest_id) {
                let edge = Edge {
                    source_id: src_id,
                    destination_id: dest_id,
                    metric: EdgeMetric::Ospf,
                };
                graph.add_edge(src_index, *router_index, edge);
            }
        }

        let mut graph: egui_graphs::Graph<
            Node,
            crate::network::edge::Edge,
            Directed,
            DefaultIx,
            MyNodeShape,
            _,
        > = egui_graphs::to_graph(&graph);

        // Node formatting

        let node_indices: Vec<NodeIndex> = graph.nodes_iter().map(|(index, _)| index).collect();

        let mut rng = rand::rng();
        for index in node_indices {
            let node: &mut egui_graphs::Node<
                Node,
                crate::network::edge::Edge,
                Directed,
                DefaultIx,
                MyNodeShape,
            > = if let Some(node) = graph.node_mut(index) {
                node
            } else {
                continue;
            };
            let position = Pos2::new(rng.random_range(0.0..40.0), rng.random_range(0.0..40.0));
            node.set_location(position);
            let payload = node.payload();
            let label = if let Some(label) = &payload.label {
                label.clone()
            } else {
                // Default label - network IP for Network and Router ID for router
                match &payload.info {
                    NodeInfo::Network(_) => "Network".to_string(),
                    NodeInfo::Router(_) => "Router".to_string()
                }
            };

            // Set color based on node type
            let router_color = Color32::BLUE;
            let network_color = Color32::GREEN;
            let inter_area_color = Color32::LIGHT_GREEN;
            let node_color = if payload.is_inter_area() {
                inter_area_color
            } else {
                match &payload.info {
                NodeInfo::Network(_) => network_color,
                NodeInfo::Router(_) => router_color,
                }
            };
            node.set_label(label);
            node.set_color(node_color);
        }

        Self {
            graph,
            node_id_to_index_map,
        }
    }
    
    // Reconcile the existing graph in place to match the provided nodes (by UUID).
        // - Updates/keeps positions for existing nodes
        // - Adds new nodes with a seeded position
        // - Removes vanished nodes
        // - Rebuilds edges from current nodes (router -> network)
        pub fn reconcile(&mut self, desired_nodes: Vec<Node>) {
            let mut rng = rand::rng();
    
            // 1) Desired set and quick lookup
            let mut desired_map: HashMap<Uuid, Node> = HashMap::with_capacity(desired_nodes.len());
            for n in desired_nodes {
                desired_map.insert(n.id, n);
            }
    
            // 2) Remove nodes that no longer exist
            //    Collect first to avoid borrow issues during mutation.
            let mut to_remove: Vec<Uuid> = Vec::new();
            for (id, idx) in self.node_id_to_index_map.iter() {
                if !desired_map.contains_key(id) {
                    to_remove.push(*id);
                }
            }
            for id in to_remove {
                if let Some(idx) = self.node_id_to_index_map.remove(&id) {
                    // Removing a node should drop its incident edges automatically.
                    // Adjust this if your egui_graphs version uses a different removal API.
                    let _ = self.graph.remove_node(idx);
                }
            }
    
            // 3) Add or update remaining nodes
            for (id, desired) in desired_map.iter() {
                if let Some(&idx) = self.node_id_to_index_map.get(id) {
                    // Update label/color (keep position and index stable)
                    if let Some(node) = self.graph.node_mut(idx) {
                        let label = desired.label.clone().unwrap_or_else(|| {
                            match &desired.info {
                                NodeInfo::Network(_) => "Network".to_string(),
                                NodeInfo::Router(_) => "Router".to_string()
                            }
                        });
                        
                        let payload = node.payload();
                        
                        let router_color = Color32::BLUE;
                        let network_color = Color32::GREEN;
                        let inter_area_color = Color32::LIGHT_GREEN;
                        let node_color = if payload.is_inter_area() {
                            inter_area_color
                        } else {
                            match &payload.info {
                            NodeInfo::Network(_) => network_color,
                            NodeInfo::Router(_) => router_color,
                            }
                        };
    
                        node.set_label(label);
                        node.set_color(node_color);
    
                        // If you want to fully replace the payload, expose a method in egui_graphs::Node to set it.
                        // If not available, consider keeping an external payload cache keyed by UUID for non-visual data.
                    }
                } else {
                    // New node: add to graph and id map
                    let idx = self.graph.add_node(desired.clone());
    
                    // Seed a position near origin or random small radius.
                    // You could improve this by seeding near attached routers/networks when available.
                    let pos = Pos2::new(rng.random_range(0.0..40.0), rng.random_range(0.0..40.0));
                    if let Some(n) = self.graph.node_mut(idx) {
                        n.set_location(pos);
    
                        let payload = n.payload();
                        
                        let router_color = Color32::BLUE;
                        let network_color = Color32::GREEN;
                        let inter_area_color = Color32::LIGHT_GREEN;
                        let node_color = if payload.is_inter_area() {
                            inter_area_color
                        } else {
                            match &payload.info {
                            NodeInfo::Network(_) => network_color,
                            NodeInfo::Router(_) => router_color,
                            }
                        };
                        let label = desired.label.clone().unwrap_or_else(|| {
                            match &desired.info {
                                NodeInfo::Network(_) => "Network".to_string(),
                                NodeInfo::Router(_) => "Router".to_string()
                            }
                        });
                        n.set_label(label);
                        n.set_color(node_color);
                    }
    
                    self.node_id_to_index_map.insert(*id, idx);
                }
            }
    
            // 4) Rebuild edges to reflect current nodes (router -> network)
            //    You can either diff edges or clear and rebuild. For simplicity, rebuild.
            //    If your Graph API doesn’t have clear_edges(), remove via iteration.
            self.clear_all_edges();
    
            // For each Network node present, add edges from each attached router to the network node.
            // (Same logic as in build_new)
            for (net_uuid, &net_idx) in self.node_id_to_index_map.iter() {
                // We need to know if this is a network; read the payload back
                if let Some(net_node) = self.graph.node(net_idx) {
                    let payload = net_node.payload();
                    if let NodeInfo::Network(network) = &payload.info {
                        if network.attached_routers.len() == 2 && IF_SKIP_FUNCTIONALLY_P2P_NETWORKS {
                            let r1 = network.attached_routers[0].to_uuidv5();
                            let r2 = network.attached_routers[1].to_uuidv5();
                            if let (Some(&r1_idx), Some(&r2_idx)) = (
                                self.node_id_to_index_map.get(&r1),
                                self.node_id_to_index_map.get(&r2),
                            ) {
                                let e1 = Edge { source_id: r1, destination_id: *net_uuid, metric: EdgeMetric::Ospf };
                                let e2 = Edge { source_id: r2, destination_id: *net_uuid, metric: EdgeMetric::Ospf };
                                self.graph.add_edge(r1_idx, net_idx, e1);
                                self.graph.add_edge(r2_idx, net_idx, e2);
                            }
                            continue;
                        }
                        for rid in network.attached_routers.clone().iter().map(RouterId::to_uuidv5) {
                            if let Some(&r_idx) = self.node_id_to_index_map.get(&rid) {
                                let e = Edge { source_id: rid, destination_id: *net_uuid, metric: EdgeMetric::Ospf };
                                self.graph.add_edge(r_idx, net_idx, e);
                            }
                        }
                    }
                }
            }
        }
    
        // Helper: remove all edges from the graph.
        // Implement using your egui_graphs version’s edge removal API.
        fn clear_all_edges(&mut self) {
            let edge_indices: Vec<_> = self.graph.edges_iter().map(|(ei, _)| ei).collect();
            for ei in edge_indices {
                let _ = self.graph.remove_edge(ei);
            }
        }
}
