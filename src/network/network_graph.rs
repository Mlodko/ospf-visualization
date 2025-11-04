
use std::collections::HashMap;

use eframe::egui::Color32;
use egui_graphs::{FruchtermanReingoldState, Graph};
use petgraph::{Directed, csr::DefaultIx, graph::NodeIndex, prelude::StableGraph};
use uuid::Uuid;

use crate::{gui::node_shape::MyNodeShape, network::{edge::{Edge, EdgeMetric}, node::{Node, NodeInfo}, router::RouterId}};

pub struct NetworkGraph {
    pub graph: Graph<Node, crate::network::edge::Edge, Directed, DefaultIx, MyNodeShape>,
    pub node_id_to_index_map: HashMap<Uuid, NodeIndex>
}

impl NetworkGraph {
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
        for index in graph.node_indices() {
            if let NodeInfo::Network(network) = &graph[index].info {
                println!("Index: {:?}", index);
                dbg!(&graph[index]);
                let source_id = graph[index].id;
                
                // If only 2 routers are attached, connect them directly and remove the network node
                if network.attached_routers.len() == 2 {
                    let router1_id = network.attached_routers[0].to_uuidv5();
                    let router2_id = network.attached_routers[1].to_uuidv5();
                    let router1_index = node_id_to_index_map[&router1_id];
                    let router2_index = node_id_to_index_map[&router2_id];
                    edges_to_add.push((router1_index, router1_id, router2_id));
                    edges_to_add.push((router2_index, router2_id, router1_id));
                    println!("removed");
                    node_indices_to_remove.push(index);
                    continue;
                }
                
                for router_node_id in network.attached_routers.iter().map(RouterId::to_uuidv5) {
                    edges_to_add.push((index, source_id, router_node_id));
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

        let mut graph: egui_graphs::Graph<Node, crate::network::edge::Edge, Directed, DefaultIx, MyNodeShape, _> = egui_graphs::to_graph(&graph);
        
        // Node formatting
        
        let node_indices: Vec<NodeIndex> = graph.nodes_iter()
            .map(|(index, _)| index)
            .collect();
        
        for index in node_indices {
            let node: &mut egui_graphs::Node<Node, crate::network::edge::Edge, Directed, DefaultIx, MyNodeShape> = if let Some(node) = graph.node_mut(index) {
                node
            } else {
                continue;
            };
            let payload = node.payload();
            let label = if let Some(label) = &payload.label {
                label.clone()
            } else {
                // Default label - network IP for Network and Router ID for router
                match &payload.info {
                    NodeInfo::Network(net) => {
                        format!("Network\nIP: {}\nMask: {}", 
                            net.ip_address.network(),
                            net.ip_address.mask()
                        )
                    }
                    NodeInfo::Router(router) => {
                        format!("Router\nID: {}", 
                            router.id
                        )
                    }
                }
            };
            
            // Set color based on node type
            let router_color = Color32::BLUE;
            let network_color = Color32::GREEN;
            let node_color = match &payload.info {
                NodeInfo::Network(_) => network_color,
                NodeInfo::Router(_) => router_color,
            };
            node.set_label(label);
            node.set_color(node_color);
        }
        
        
        Self {
            graph,
            node_id_to_index_map
        }
    }
}
