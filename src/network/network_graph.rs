use std::collections::{HashMap, HashSet};

use eframe::egui::Color32;
use egui::Pos2;
use egui_graphs::Graph;
use petgraph::{Directed, csr::DefaultIx, graph::NodeIndex, prelude::StableGraph, visit::EdgeRef};
use rand::Rng;
use uuid::Uuid;

use crate::{
    gui::node_shape::NetworkGraphNodeShape,
    network::{
        edge::{Edge, EdgeKind, EdgeMetric},
        node::{Node, NodeInfo, OspfPayload, ProtocolData},
        router::RouterId,
        // removed unused RouterId import
    }, parsers::isis_parser::core_lsp::Tlv,
};

const IF_SKIP_FUNCTIONALLY_P2P_NETWORKS: bool = false;

/// A protocol-agnostic graph wrapper used by the GUI.
///
/// Builds a graph from `Node`s and wires edges based on attached_routers.
/// `node_id_to_index_map` maps stable UUIDs to graph indices to allow safe lookups.

#[allow(dead_code)]
pub struct NetworkGraph {
    pub graph: Graph<Node, crate::network::edge::Edge, Directed, DefaultIx, NetworkGraphNodeShape>,
    pub node_id_to_index_map: HashMap<Uuid, NodeIndex>,
}

impl Default for NetworkGraph {
    fn default() -> Self {
        Self {
            graph: Graph::new(StableGraph::new()),
            node_id_to_index_map: HashMap::new(),
        }
    }
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

        // Collect edge specs & networks to remove via helper
        let (edge_specs, node_indices_to_remove) =
            Self::collect_edge_specs_stable(&graph, &node_id_to_index_map);

        for node_index in node_indices_to_remove {
            let _ = graph.remove_node(node_index);
        }

        // Materialize edges
        for (src_idx, src_uuid, dst_uuid, kind) in edge_specs {
            let metric = {
                if let Some(src_node) = graph.node_weight(src_idx) {
                    if let NodeInfo::Router(router) = &src_node.info {
                        match &router.protocol_data {
                            Some(ProtocolData::Ospf(ospf_data)) => {
                                if let OspfPayload::Router(payload) = &ospf_data.payload {
                                    let metric_by_uuid: HashMap<Uuid, u16> = payload
                                        .link_metrics
                                        .iter()
                                        .map(|(ip, metric)| {
                                            (RouterId::Ipv4(*ip).to_uuidv5(), *metric)
                                        })
                                        .collect();
                                    if let Some(metric) = metric_by_uuid.get(&dst_uuid) {
                                        EdgeMetric::Ospf(*metric as u32)
                                    } else {
                                        EdgeMetric::None
                                    }
                                } else {
                                    EdgeMetric::None
                                }
                            }
                            None => EdgeMetric::None,
                            _ => panic!("Unexpected protocol data"),
                        }
                    } else {
                        EdgeMetric::None
                    }
                } else {
                    EdgeMetric::None
                }
            };
            if let Some(&dst_idx) = node_id_to_index_map.get(&dst_uuid) {
                let edge_src_to_dst = Edge {
                    source_id: src_uuid,
                    destination_id: dst_uuid,
                    kind: kind.clone(),
                    metric: metric,
                    protocol_tag: Some("OSPF".to_string()),
                };
                graph.add_edge(src_idx, dst_idx, edge_src_to_dst);
                let edge_dst_to_src = Edge {
                    source_id: dst_uuid,
                    destination_id: src_uuid,
                    kind,
                    metric: EdgeMetric::None,
                    protocol_tag: Some("OSPF".to_string()),
                };
                graph.add_edge(dst_idx, src_idx, edge_dst_to_src);
            }
        }
        eprintln!(
            "[network_graph::build_new] materialized {} edges ({} nodes)",
            graph.edge_count(),
            graph.node_count()
        );

        let mut graph: egui_graphs::Graph<
            Node,
            crate::network::edge::Edge,
            Directed,
            DefaultIx,
            NetworkGraphNodeShape,
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
                NetworkGraphNodeShape,
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
                    NodeInfo::Router(_) => "Router".to_string(),
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

    /// Reconcile the existing graph in place to match the provided nodes (by UUID).
    /// - Updates/keeps positions for existing nodes
    /// - Adds new nodes with a seeded position
    /// - Removes vanished nodes
    /// - Rebuilds edges from current nodes (router -> network)
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
        for (id, _) in self.node_id_to_index_map.iter() {
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
                    // Replace payload wholesale, preserving position and selection state.
                    // (Store position first if needed.)
                    let pos = node.location();
                    *node.payload_mut() = desired.clone(); // requires a payload_mut() API; if not available, re-add node.

                    // Reapply label/color logic based on the new payload.
                    let label = desired
                        .label
                        .clone()
                        .unwrap_or_else(|| match &desired.info {
                            NodeInfo::Network(_) => "Network".to_string(),
                            NodeInfo::Router(_) => "Router".to_string(),
                        });
                    let router_color = Color32::BLUE;
                    let network_color = Color32::GREEN;
                    let inter_area_color = Color32::LIGHT_GREEN;
                    let node_color = if desired.is_inter_area() {
                        inter_area_color
                    } else {
                        match &desired.info {
                            NodeInfo::Network(_) => network_color,
                            NodeInfo::Router(_) => router_color,
                        }
                    };
                    node.set_label(label);
                    node.set_color(node_color);
                    node.set_location(pos);
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
                    let label = desired
                        .label
                        .clone()
                        .unwrap_or_else(|| match &desired.info {
                            NodeInfo::Network(_) => "Network".to_string(),
                            NodeInfo::Router(_) => "Router".to_string(),
                        });
                    n.set_label(label);
                    n.set_color(node_color);
                }

                self.node_id_to_index_map.insert(*id, idx);
            }
        }

        // 4) Rebuild edges using helper (membership + logical reachability)
        self.clear_all_edges();
        let edge_specs = self.collect_edge_specs_live();
        self.materialize_edges(edge_specs, "[network_graph::reconcile]");
    }

    /// Helper: remove all edges from the graph.
    fn clear_all_edges(&mut self) {
        let edge_indices: Vec<_> = self.graph.edges_iter().map(|(ei, _)| ei).collect();
        for ei in edge_indices {
            let _ = self.graph.remove_edge(ei);
        }
    }

    /// Helper: collect edge specs from StableGraph during build_new
    /// Returns a tuple `(Vec<graph source node index, source uuid, destination uuid, EdgeKind>, Vec<graph indices of nodes to remove>)`
    fn collect_edge_specs_stable(
        graph: &StableGraph<Node, Edge, Directed, DefaultIx>,
        id_map: &HashMap<Uuid, NodeIndex>,
    ) -> (Vec<(NodeIndex, Uuid, Uuid, EdgeKind)>, Vec<NodeIndex>) {
        let mut node_indices_to_remove = Vec::new();
        let mut specs: Vec<(NodeIndex, Uuid, Uuid, EdgeKind)> = Vec::new();
        let mut seen: HashSet<(Uuid, Uuid, EdgeKind)> = HashSet::new();

        for net_index in graph.node_indices() {
            if let NodeInfo::Network(network) = &graph[net_index].info {
                let net_uuid = graph[net_index].id;

                if network.attached_routers.len() == 2 && IF_SKIP_FUNCTIONALLY_P2P_NETWORKS {
                    // Optionally collapse; current policy: just mark for removal
                    node_indices_to_remove.push(net_index);
                    continue;
                }

                // Membership
                for rid in &network.attached_routers {
                    let r_uuid = rid.to_uuidv5();
                    if let Some(&r_idx) = id_map.get(&r_uuid) {
                        let kind = EdgeKind::Membership;
                        if seen.insert((r_uuid, net_uuid, kind.clone())) {
                            specs.push((r_idx, r_uuid, net_uuid, kind));
                        }
                    }
                }

                // Logical Reachability
                if let Some(ProtocolData::Ospf(data)) = &network.protocol_data {
                    if let OspfPayload::Network(payload) = &data.payload {
                        for s in &payload.summaries {
                            // Skip logical reachability edge if:
                            // 1) This is a detailed network (designated_router_id present)
                            // 2) The originating ABR is already attached (membership edge exists)
                            if payload.designated_router_id.is_some()
                                || network.attached_routers.iter().any(|r| r == &s.origin_abr)
                            {
                                continue;
                            }
                            let abr_uuid = s.origin_abr.to_uuidv5();
                            if let Some(&abr_idx) = id_map.get(&abr_uuid) {
                                let kind = EdgeKind::LogicalReachability;
                                if seen.insert((abr_uuid, net_uuid, kind.clone())) {
                                    specs.push((abr_idx, abr_uuid, net_uuid, kind));
                                }
                            }
                        }
                    }
                }
            }
        }

        (specs, node_indices_to_remove)
    }

    /// Helper: collect edge specs from the live egui_graphs graph during reconcile
    /// Returns a `Vec<(graph source node index, source uuid, destination uuid, EdgeKind)>`
    fn collect_edge_specs_live(&self) -> Vec<(NodeIndex, Uuid, Uuid, EdgeKind)> {
        let mut specs = Vec::new();
        let mut seen: HashSet<(Uuid, Uuid, EdgeKind)> = HashSet::new();

        for (net_uuid, &net_idx) in self.node_id_to_index_map.iter() {
            if let Some(net_node) = self.graph.node(net_idx) {
                let payload = net_node.payload();
                if let NodeInfo::Network(network) = &payload.info {
                    if network.attached_routers.len() == 2 && IF_SKIP_FUNCTIONALLY_P2P_NETWORKS {
                        continue;
                    }

                    // Membership
                    for rid in &network.attached_routers {
                        let r_uuid = rid.to_uuidv5();
                        if let Some(&r_idx) = self.node_id_to_index_map.get(&r_uuid) {
                            let kind = EdgeKind::Membership;
                            if seen.insert((r_uuid, *net_uuid, kind.clone())) {
                                specs.push((r_idx, r_uuid, *net_uuid, kind));
                            }
                        }
                    }

                    // Logical Reachability
                    if let Some(ProtocolData::Ospf(data)) = &network.protocol_data {
                        if let OspfPayload::Network(net_payload) = &data.payload {
                            for s in &net_payload.summaries {
                                // Skip logical reachability edge if detailed network or ABR already attached.
                                if net_payload.designated_router_id.is_some()
                                    || network.attached_routers.iter().any(|r| r == &s.origin_abr)
                                {
                                    continue;
                                }
                                let abr_uuid = s.origin_abr.to_uuidv5();
                                if let Some(&abr_idx) = self.node_id_to_index_map.get(&abr_uuid) {
                                    let kind = EdgeKind::LogicalReachability;
                                    if seen.insert((abr_uuid, *net_uuid, kind.clone())) {
                                        specs.push((abr_idx, abr_uuid, *net_uuid, kind));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        specs
    }

    /// Helper: materialize edge specs into the live graph
    fn materialize_edges(&mut self, specs: Vec<(NodeIndex, Uuid, Uuid, EdgeKind)>, log_tag: &str) {
        let mut added = 0usize;
        for (src_idx, src_uuid, dst_uuid, kind) in specs {
            let metric = {
                if let Some(src_node) = self.graph.node(src_idx).map(|node| node.payload()) {
                    if let NodeInfo::Router(router) = &src_node.info {
                        match &router.protocol_data {
                            Some(ProtocolData::Ospf(ospf_data)) => {
                                if let OspfPayload::Router(payload) = &ospf_data.payload {
                                    let metric_by_uuid: HashMap<Uuid, u16> = payload
                                        .link_metrics
                                        .iter()
                                        .map(|(ip, metric)| {
                                            (RouterId::Ipv4(*ip).to_uuidv5(), *metric)
                                        })
                                        .collect();
                                    if let Some(metric) = metric_by_uuid.get(&dst_uuid) {
                                        EdgeMetric::Ospf(*metric as u32)
                                    } else {
                                        EdgeMetric::None
                                    }
                                } else {
                                    EdgeMetric::None
                                }
                            }
                            Some(ProtocolData::IsIs(data)) => {
                                // We only consider IS reachability, ignoring IP reachability TLVs
                                let metrics_by_uuid: HashMap<Uuid, u32> = {
                                    let mut metrics = HashMap::new();
                                    if let Some(Tlv::ExtendedReachability(ext_reach)) = data.tlvs.iter().find(|t| matches!(t, Tlv::ExtendedReachability(_))) {
                                        ext_reach.neighbors.iter()
                                            .map(|neighbor| {
                                                let neighbor_uuid = RouterId::IsIs(neighbor.neighbor_id.clone()).to_uuidv5();
                                                (neighbor_uuid, neighbor.metric)
                                            })
                                            .for_each(|(uuid, metric)| {
                                                metrics.insert(uuid, metric);
                                            })
                                    }
                                    if let Some(Tlv::IsReachability(is_reach)) = data.tlvs.iter().find(|t| matches!(t, Tlv::IsReachability(_))) {
                                        is_reach.neighbors_iter()
                                            .map(|neighbor| {
                                                let neighbor_uuid = RouterId::IsIs(neighbor.system_id.clone()).to_uuidv5();
                                                (neighbor_uuid, neighbor.metric)
                                            })
                                            .for_each(|(uuid, metric)| {
                                                if !metrics.contains_key(&uuid) {
                                                    metrics.insert(uuid, metric);
                                                }
                                            })
                                    }
                                    metrics
                                };
                                if let Some(metric) = metrics_by_uuid.get(&dst_uuid) {
                                    EdgeMetric::IsIs(*metric)
                                } else {
                                    EdgeMetric::None
                                }
                            }
                            None => EdgeMetric::None,
                            _ => panic!("Unexpected protocol data"),
                        }
                    } else {
                        EdgeMetric::None
                    }
                } else {
                    EdgeMetric::None
                }
            };
            if let Some(&dst_idx) = self.node_id_to_index_map.get(&dst_uuid) {
                let edge_src_to_dst = Edge {
                    source_id: src_uuid,
                    destination_id: dst_uuid,
                    kind: kind.clone(),
                    metric: metric,
                    protocol_tag: Some("OSPF".to_string()),
                };
                self.graph.add_edge(src_idx, dst_idx, edge_src_to_dst);
                let edge_dst_to_src = Edge {
                    source_id: dst_uuid,
                    destination_id: src_uuid,
                    kind,
                    metric: EdgeMetric::None,
                    protocol_tag: Some("OSPF".to_string()),
                };
                self.graph.add_edge(dst_idx, src_idx, edge_dst_to_src);
                added += 2;
            }
        }
        eprintln!("{log_tag} materialized {added} edges");
    }
}

impl ToString for NetworkGraph {
    fn to_string(&self) -> String {
        use petgraph::Direction;
        let mut output = String::from("Network Graph {\n");

        // Collect nodes with indices for deterministic ordering (Routers first, then Networks, then by identifier)
        let mut nodes: Vec<(
            NodeIndex,
            &egui_graphs::Node<Node, Edge, Directed, DefaultIx, NetworkGraphNodeShape>,
        )> = self.graph.nodes_iter().collect();
        nodes.sort_by(|(_, a), (_, b)| {
            let ta = match &a.payload().info {
                NodeInfo::Router(_) => 0,
                NodeInfo::Network(_) => 1,
            };
            let tb = match &b.payload().info {
                NodeInfo::Router(_) => 0,
                NodeInfo::Network(_) => 1,
            };
            if ta != tb {
                return ta.cmp(&tb);
            }
            let sa = match &a.payload().info {
                NodeInfo::Router(r) => r.id.as_string(),
                NodeInfo::Network(n) => n.ip_address.to_string(),
            };
            let sb = match &b.payload().info {
                NodeInfo::Router(r) => r.id.as_string(),
                NodeInfo::Network(n) => n.ip_address.to_string(),
            };
            sa.cmp(&sb)
        });

        for (idx, (node_index, node)) in nodes.iter().enumerate() {
            let payload = node.payload();
            let (kind, ident) = match &payload.info {
                NodeInfo::Router(r) => ("Router", r.id.as_string()),
                NodeInfo::Network(n) => ("Network", n.ip_address.to_string()),
            };

            output += &format!("    {} {} {{\n", kind, ident);

            // Outgoing edges
            let mut outgoing: Vec<String> = self
                .graph
                .edges_directed(*node_index, Direction::Outgoing)
                .filter_map(|edge| {
                    let target = edge.target();
                    self.graph.node(target).map(|n| match &n.payload().info {
                        NodeInfo::Router(r) => format!("Router {}", r.id.as_string()),
                        NodeInfo::Network(net) => format!("Network {}", net.ip_address),
                    })
                })
                .collect();
            outgoing.sort();

            // Incoming edges
            let mut incoming: Vec<String> = self
                .graph
                .edges_directed(*node_index, Direction::Incoming)
                .filter_map(|edge| {
                    let source = edge.source();
                    self.graph.node(source).map(|n| match &n.payload().info {
                        NodeInfo::Router(r) => format!("Router {}", r.id.as_string()),
                        NodeInfo::Network(net) => format!("Network {}", net.ip_address),
                    })
                })
                .collect();
            incoming.sort();

            output += &format!("        Outgoing -> [{}]\n", outgoing.join(", "));
            output += &format!("        Incoming <- [{}]\n", incoming.join(", "));

            output += "    }";
            if idx + 1 < nodes.len() {
                output += ",\n";
            } else {
                output += "\n";
            }
        }

        output += "}";
        output
    }
}
