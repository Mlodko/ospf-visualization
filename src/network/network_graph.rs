use std::collections::{HashMap, HashSet};

use eframe::egui::Color32;
use egui::Pos2;
use egui_graphs::Graph;
use petgraph::{Directed, csr::DefaultIx, graph::NodeIndex, prelude::StableGraph, visit::EdgeRef};
use rand::Rng;
use uuid::Uuid;

use crate::{
    gui::{app, edge_shape::NetworkGraphEdgeShape, node_shape::NetworkGraphNodeShape},
    network::{
        edge::{Edge, EdgeKind, EdgeMetric, ManualEdgeSpec, UndirectedEdgeKey},
        node::{IsIsData, Node, NodeInfo, OspfData, OspfPayload, ProtocolData},
        router::{Router, RouterId},
        // removed unused RouterId import
    },
    parsers::isis_parser::core_lsp::Tlv,
};

const IF_SKIP_FUNCTIONALLY_P2P_NETWORKS: bool = false;

/// A protocol-agnostic graph wrapper used by the GUI.
///
/// Builds a graph from `Node`s and wires edges based on attached_routers.
/// `node_id_to_index_map` maps stable UUIDs to graph indices to allow safe lookups.

#[allow(dead_code)]
pub struct NetworkGraph {
    pub graph: Graph<
        Node,
        crate::network::edge::Edge,
        Directed,
        DefaultIx,
        NetworkGraphNodeShape,
        NetworkGraphEdgeShape,
    >,
    pub node_id_to_index_map: HashMap<Uuid, NodeIndex>,
    manual_edges: HashMap<UndirectedEdgeKey, ManualEdgeSpec>,
    manual_removed_edges: HashSet<UndirectedEdgeKey>,
}

impl Default for NetworkGraph {
    fn default() -> Self {
        Self {
            graph: Graph::new(StableGraph::new()),
            node_id_to_index_map: HashMap::new(),
            manual_edges: HashMap::new(),
            manual_removed_edges: HashSet::new(),
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
                            Some(ProtocolData::IsIs(isis_data)) => {
                                if let Some(Tlv::ExtendedReachability(tlv)) = isis_data
                                    .tlvs
                                    .iter()
                                    .find(|tlv| matches!(tlv, Tlv::ExtendedReachability(_)))
                                {
                                    let metrics_by_uuid: HashMap<Uuid, u32> = tlv
                                        .neighbors
                                        .iter()
                                        .map(|n| {
                                            (
                                                RouterId::IsIs(n.neighbor_id.clone()).to_uuidv5(),
                                                n.metric,
                                            )
                                        })
                                        .collect();
                                    if let Some(metric) = metrics_by_uuid.get(&dst_uuid) {
                                        EdgeMetric::IsIs(*metric as u32)
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

            if let EdgeMetric::None = metric {
                println!("Metric is None");
            }
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
            let theme = app::get_theme();

            // Set label; color will be derived by NetworkGraphNodeShape via theme visuals
            node.set_label(label);
        }

        Self {
            graph,
            node_id_to_index_map,
            ..Default::default()
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
                }

                self.node_id_to_index_map.insert(*id, idx);
            }
        }

        // 4) Rebuild edges using helper (membership + logical reachability)
        self.clear_all_edges();
        let edge_specs = self.collect_edge_specs_live();
        self.materialize_edges(edge_specs, "[network_graph::reconcile]");
        self.apply_overlay_after_reconcile();
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
            let metric = match kind {
                // Membership edges don't carry a metric
                EdgeKind::Membership => self.membership_metric(src_idx, src_uuid, dst_uuid),
                // For OSPF logical reachability (ABR -> Network), use the Summary metric
                EdgeKind::LogicalReachability => {
                    self.logical_reachability_metric(src_idx, src_uuid, dst_uuid)
                }
                // Default: no metric
                _ => EdgeMetric::None,
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

    fn membership_metric(&self, src_idx: NodeIndex, src_uuid: Uuid, dst_uuid: Uuid) -> EdgeMetric {
        let src_node = match self.graph.node(src_idx) {
            Some(n) => n.payload(),
            None => return EdgeMetric::None,
        };

        let router = match &src_node.info {
            NodeInfo::Router(r) => r,
            _ => return EdgeMetric::None,
        };

        match &router.protocol_data {
            Some(ProtocolData::IsIs(isis_data)) => self.isis_membership_metric(dst_uuid, isis_data),
            _ => EdgeMetric::None,
        }
    }

    fn isis_membership_metric(&self, dst_uuid: Uuid, isis_data: &IsIsData) -> EdgeMetric {
        let dst_idx = match self.node_id_to_index_map.get(&dst_uuid) {
            Some(i) => i,
            None => return EdgeMetric::None,
        };

        let dst_node = match self.graph.node(*dst_idx) {
            Some(n) => n.payload(),
            None => return EdgeMetric::None,
        };

        let network = match &dst_node.info {
            NodeInfo::Network(n) => n,
            _ => return EdgeMetric::None,
        };

        let ext_ip_reach = match isis_data.tlvs.iter().find_map(|t| {
            if let Tlv::ExtendedIpReachability(e) = t {
                Some(e)
            } else {
                None
            }
        }) {
            Some(e) => e,
            None => return EdgeMetric::None,
        };

        match ext_ip_reach
            .neighbors
            .iter()
            .find(|n| n.prefix == network.ip_address)
        {
            Some(nbr) => EdgeMetric::IsIs(nbr.metric),
            None => EdgeMetric::None,
        }
    }

    fn logical_reachability_metric(
        &self,
        src_idx: NodeIndex,
        src_uuid: Uuid,
        dst_uuid: Uuid,
    ) -> EdgeMetric {
        let src_node = match self.graph.node(src_idx) {
            Some(n) => n.payload(),
            None => return EdgeMetric::None,
        };

        let router = match &src_node.info {
            NodeInfo::Router(r) => r,
            _ => return EdgeMetric::None,
        };

        match &router.protocol_data {
            Some(ProtocolData::Ospf(d)) => {
                self.ospf_logical_reachability_metric(src_idx, src_uuid, dst_uuid, d, router)
            }
            _ => EdgeMetric::None,
        }
    }

    fn ospf_logical_reachability_metric(
        &self,
        src_idx: NodeIndex,
        src_uuid: Uuid,
        dst_uuid: Uuid,
        ospf_data: &OspfData,
        src_router: &Router,
    ) -> EdgeMetric {
        let dst_idx = match self.node_id_to_index_map.get(&src_uuid) {
            Some(idx) => idx,
            None => return EdgeMetric::None,
        };

        let dst_node = match self.graph.node(*dst_idx) {
            Some(n) => n.payload(),
            None => return EdgeMetric::None,
        };

        let dst_net = match &dst_node.info {
            NodeInfo::Network(n) => n,
            _ => return EdgeMetric::None,
        };

        let dst_ospf_data = match &dst_net.protocol_data {
            Some(ProtocolData::Ospf(d)) => d,
            _ => return EdgeMetric::None,
        };

        match &dst_ospf_data.payload {
            // Concrete network node with summaries attached
            OspfPayload::Network(net_payload) => {
                if let Some(s) = net_payload
                    .summaries
                    .iter()
                    .find(|s| s.origin_abr == src_router.id)
                {
                    EdgeMetric::Ospf(s.metric)
                } else {
                    EdgeMetric::None
                }
            }
            // Node itself represents a summarized prefix
            OspfPayload::SummaryNetwork(summary) => EdgeMetric::Ospf(summary.metric),
            // Other payload types don’t carry ABR->Network summary metrics
            _ => EdgeMetric::None,
        }
    }

    pub fn add_manual_edge(&mut self, a: Uuid, b: Uuid, kind: EdgeKind, metric: u32) {
        let key = UndirectedEdgeKey::new(a, b, kind.clone());
        let spec = ManualEdgeSpec::new(key, metric);

        self.manual_edges.insert(key, spec);

        self.manual_removed_edges.remove(&key);

        self.apply_manual_edge_live(key);
    }

    pub fn update_manual_edge(&mut self, a: Uuid, b: Uuid, kind: EdgeKind, metric: u32) {
        let key = UndirectedEdgeKey::new(a, b, kind.clone());
        if let Some(spec) = self.manual_edges.get_mut(&key) {
            spec.set_metric(metric);
            self.apply_manual_edge_live(key);
        } else {
            println!("Edge not found")
        }
    }

    /// Hide existing base edge until manual_removed is cleared
    pub fn supress_base_edge(&mut self, a: Uuid, b: Uuid, kind: EdgeKind) {
        let key = UndirectedEdgeKey::new(a, b, kind.clone());
        self.manual_edges.remove(&key);
        self.manual_removed_edges.insert(key);
        let (x, y) = key.endpoints();
        self.remove_edge_pair_live(x, y, kind);
    }

    /// Remove only manually added edge: base edge may reappear
    pub fn remove_manual_edge(&mut self, a: Uuid, b: Uuid, kind: EdgeKind) {
        let key = UndirectedEdgeKey::new(a, b, kind.clone());
        self.manual_edges.remove(&key);
        self.remove_edge_pair_live(a, b, kind);
    }

    pub fn any_manual_changes(&self) -> bool {
        !self.manual_edges.is_empty() || !self.manual_removed_edges.is_empty()
    }

    pub fn clear_manual_changes(&mut self) {
        // Remove manual edges
        let mut to_remove = Vec::new();
        for (ei, e) in self.graph.edges_iter() {
            if e.payload().protocol_tag.as_deref() == Some("MANUAL") {
                to_remove.push(ei);
            }
        }
        for ei in &to_remove {
            let _ = self.graph.remove_edge(*ei);
        }
        self.manual_edges.clear();
        self.manual_removed_edges.clear();

        // Rebuild base edges
        self.clear_all_edges();
        let specs = self.collect_edge_specs_live();
        self.materialize_edges(specs, "[network_graph::clear_manual_changes]");
        // Overlay skip (empty)
        eprintln!(
            "[network_graph] manual overlay cleared; removed {} manual edges",
            to_remove.len()
        );
    }

    fn apply_manual_edge_live(&mut self, key: UndirectedEdgeKey) {
        // Ensure no duplicates: remove any existing edges for this pair/kind (base or prior manual)
        let (a, b) = key.endpoints();
        self.remove_edge_pair_live(a, b, key.kind.clone());

        // Add both directions with metric and protocol_tag "MANUAL"
        if let (Some(&ai), Some(&bi)) = (
            self.node_id_to_index_map.get(&a),
            self.node_id_to_index_map.get(&b),
        ) {
            let spec = self.manual_edges.get(&key).cloned();
            if let Some(spec) = spec {
                let e_ab = Edge {
                    source_id: a,
                    destination_id: b,
                    kind: key.kind.clone(),
                    metric: spec.metric.clone(),
                    protocol_tag: Some(spec.protocol_tag.clone()),
                };
                let e_ba = Edge {
                    source_id: b,
                    destination_id: a,
                    kind: key.kind.clone(),
                    metric: spec.metric,
                    protocol_tag: Some(spec.protocol_tag),
                };
                self.graph.add_edge(ai, bi, e_ab);
                self.graph.add_edge(bi, ai, e_ba);
            }
        }
    }

    pub fn is_manual_edge(&self, a: Uuid, b: Uuid, kind: EdgeKind) -> bool {
        self.manual_edges
            .contains_key(&UndirectedEdgeKey::new(a, b, kind))
    }

    fn remove_edge_pair_live(&mut self, a: Uuid, b: Uuid, kind: EdgeKind) {
        // Find and remove both directions matching (a->b) and (b->a) with given kind
        let mut to_remove = Vec::new();
        for (ei, e) in self.graph.edges_iter() {
            if e.payload().source_id == a
                && e.payload().destination_id == b
                && e.payload().kind == kind
            {
                to_remove.push(ei);
            } else if e.payload().source_id == b
                && e.payload().destination_id == a
                && e.payload().kind == kind
            {
                to_remove.push(ei);
            }
        }
        for ei in to_remove {
            let _ = self.graph.remove_edge(ei);
        }
    }

    pub fn apply_overlay_after_reconcile(&mut self) {
        // 1) Remove overridden base edges
        for key in self.manual_removed_edges.clone() {
            let (a, b) = key.endpoints();
            self.remove_edge_pair_live(a, b, key.kind.clone());
        }
        // 2) Add manual edges with stored metrics
        for key in self.manual_edges.keys().cloned().collect::<Vec<_>>() {
            self.apply_manual_edge_live(key);
        }
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
