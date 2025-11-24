#![allow(dead_code)]

/*!
This module provides mechanisms necessary for storing, managing and merging topology data from different sources.

This module defines:
- `Partition`: Collection of nodes originating from one source (e.g. a router from which the topology data is collected)
- `SourceHealth`: Represents a source's status
- `SourceState`: Holds information about a source, as well as the partition it manages.
*/

use crate::network::{
    node::{Node, NodeInfo, OspfNetworkPayload, OspfPayload, PerAreaRouterFacet, ProtocolData},
    router::RouterId,
};
use ipnetwork::IpNetwork;
use ospf_parser::OspfLinkStateAdvertisement;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use std::{
    collections::{HashMap, HashSet},
    net::Ipv4Addr,
    time::SystemTime,
};
use uuid::Uuid;

pub type SourceId = RouterId;

/// Collection of nodes originating from one source (e.g. a router from which the topology data is collected)
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Partition {
    pub nodes: HashMap<Uuid, Node>,
}
impl Partition {
    /// Creates a new Partition
    pub fn new(nodes: Vec<Node>) -> Self {
        let mut map = HashMap::with_capacity(nodes.len());
        for node in nodes {
            map.insert(node.id, node);
        }
        Partition { nodes: map }
    }
}

/// Represents a source's status
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceHealth {
    Connected,
    Lost,
}

impl ToString for SourceHealth {
    fn to_string(&self) -> String {
        use SourceHealth::*;
        match self {
            Connected => "Connected".to_string(),
            Lost => "Lost".to_string()
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
/// Holds information about a source, as well as the partition it manages.
pub struct SourceState {
    pub health: SourceHealth,
    pub partition: Partition,
    pub last_snapshot: SystemTime, // when we last replaced the snapshot successfully
    pub last_connected: SystemTime, // when acquisition last succeeded
    pub last_status_change: SystemTime, // when health last changed
}
impl SourceState {
    /// Creates a new `SourceState` from a `Partition` and the `Instant` of the last data update.
    pub fn new(partition: Partition, ts: SystemTime) -> Self {
        SourceState {
            partition,
            health: SourceHealth::Connected,
            last_snapshot: ts,
            last_connected: ts,
            last_status_change: ts,
        }
    }
}

/// Storage for all known sources. Manages merging topologies from sources.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TopologyStore {
    sources: HashMap<SourceId, SourceState>,
}

#[derive(Debug, Clone, Error)]
pub enum StoreError {
    #[error("Source not found: {0}")]
    SourceNotFound(SourceId)
}

/// Represents a router as seen from one source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterFacet {
    pub source_id: SourceId,
    pub area_id: Option<Ipv4Addr>,
    pub p2p_link_count: usize,
    pub transit_link_count: usize,
    pub stub_link_count: usize,
    pub node: Node,
}

/// Holds all `RouterFacet`s of a router node
#[derive(Debug, Clone)]
struct DuplicateRouterBucket {
    pub router_uuid: Uuid,
    pub facets: Vec<RouterFacet>,
    pub area_set: HashSet<Ipv4Addr>,
    pub all_facets_identical: bool,
}

impl ToString for TopologyStore {
    fn to_string(&self) -> String {
        use std::fmt::Write;

        let mut out = String::from("TopologyStore {\n");

        // Deterministic ordering by SourceId string
        let mut sources: Vec<_> = self.sources.iter().collect();
        sources.sort_by(|(a, _), (b, _)| a.as_string().cmp(&b.as_string()));

        for (i, (src_id, state)) in sources.iter().enumerate() {
            let _ = writeln!(out, "    Source {} {{", src_id.as_string());
            let _ = writeln!(out, "        Health: {:?}", state.health);

            // Collect nodes deterministically
            let mut nodes: Vec<&Node> = state.partition.nodes.values().collect();
            nodes.sort_by(|a, b| a.id.cmp(&b.id));

            for node in nodes {
                match &node.info {
                    NodeInfo::Router(r) => {
                        if let Some(pd) = &r.protocol_data {
                            match pd {
                                ProtocolData::Ospf(odata) => match &odata.payload {
                                    OspfPayload::Router(rp) => {
                                        let tags = rp.to_str_tags();
                                        let area_list = rp
                                            .per_area_facets
                                            .iter()
                                            .map(|f| f.area_id.to_string())
                                            .collect::<Vec<_>>()
                                            .join(", ");
                                        let tag_str = if tags.is_empty() {
                                            String::new()
                                        } else {
                                            format!(", tags: [{}]", tags.join(", "))
                                        };
                                        let _ = writeln!(
                                            out,
                                            "        Router {} {{ p2p: {}, transit: {}, stub: {}, areas: [{}]{} }}",
                                            r.id.as_string(),
                                            rp.p2p_link_count,
                                            rp.transit_link_count,
                                            rp.stub_link_count,
                                            area_list,
                                            tag_str
                                        );
                                    }
                                    other_variant => {
                                        let _ = writeln!(
                                            out,
                                            "        Router {} {{ OSPF payload: {:?} }}",
                                            r.id.as_string(),
                                            other_variant
                                        );
                                    }
                                },
                                other => {
                                    let _ = writeln!(
                                        out,
                                        "        Router {} {{ protocol: {:?} }}",
                                        r.id.as_string(),
                                        other
                                    );
                                }
                            }
                        } else {
                            let _ = writeln!(out, "        Router {} {{ }}", r.id.as_string());
                        }
                    }
                    NodeInfo::Network(net) => {
                        let attached = net
                            .attached_routers
                            .iter()
                            .map(|rid| rid.as_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        let _ = writeln!(
                            out,
                            "        Network {} {{ attached_routers: [{}] }}",
                            net.ip_address, attached
                        );
                    }
                }
            }

            let _ = write!(out, "    }}");
            if i + 1 < sources.len() {
                out.push(',');
            }
            out.push('\n');
        }

        out.push('}');
        out
    }
}

impl TopologyStore {
    
    pub fn sources_iter(&self) -> impl Iterator<Item = (&SourceId, &SourceState)> {
        self.sources.iter()
    }
    
    pub fn get_source_state(&self, src_id: &SourceId) -> Option<&SourceState> {
        self.sources.get(src_id)
    }
    
    pub fn remove_partition(&mut self, src_id: &SourceId) -> Result<(), StoreError> {
        match self.sources.remove(src_id) {
            Some(_) => Ok(()),
            None => Err(StoreError::SourceNotFound(src_id.clone()))
        }
    }
    
    /// Replace the partition of a source with a new set of nodes. If a node is already part of the partition its data is updated.
    pub fn replace_partition(&mut self, src_id: SourceId, nodes: Vec<Node>, timestamp: SystemTime) {
        // annotate nodes with their source for partition-based highlighting
        let mut annotated = Vec::with_capacity(nodes.len());
        for mut node in nodes {
            node.source_id = Some(src_id.clone());
            annotated.push(node);
        }
        let part = Partition::new(annotated);
        match self.sources.get_mut(&src_id) {
            Some(state) => {
                state.partition = part;
                state.health = SourceHealth::Connected;
                state.last_snapshot = timestamp;
                state.last_connected = timestamp;
                state.last_status_change = timestamp; // optional: only if you want “Connected” flips to count
            }
            None => {
                self.sources
                    .insert(src_id, SourceState::new(part, timestamp));
            }
        }
    }

    /// Mark a source as lost.
    pub fn mark_lost(&mut self, src_id: &SourceId, timestamp: SystemTime) {
        if let Some(state) = self.sources.get_mut(src_id) {
            state.health = SourceHealth::Lost;
            state.last_status_change = timestamp;
            // Keep last_snapshot/last_connected intact (they reflect last success)
        } else {
            // Optionally track unknown-lost source with empty partition
            self.sources.insert(
                src_id.clone(),
                SourceState {
                    health: SourceHealth::Lost,
                    partition: Partition::default(),
                    last_snapshot: timestamp,
                    last_connected: timestamp,
                    last_status_change: timestamp,
                },
            );
        }
    }

    /// Detect duplicate routers in the topology.
    fn detect_router_duplicates(&self, connected_only: bool) -> Vec<DuplicateRouterBucket> {
        let mut buckets = HashMap::new();
        let partitions = self
            .sources
            .iter()
            .filter(|(_, src)| !connected_only || src.health == SourceHealth::Connected)
            .map(|(src_id, src)| (src_id, &src.partition));

        for (source_id, partition) in partitions {
            for (router_uuid, node) in &partition.nodes {
                if !matches!(node.info, crate::network::node::NodeInfo::Router(_)) {
                    continue;
                }
                buckets
                    .entry(*router_uuid)
                    .or_insert_with(|| Vec::new())
                    .push((source_id.clone(), node))
            }
        }

        let mut out = Vec::new();
        for (router_uuid, nodes_by_source) in buckets.into_iter() {
            if nodes_by_source.len() <= 1 {
                continue;
            }
            // Handle duplicate routers here
            let mut facets = Vec::with_capacity(nodes_by_source.len());
            let mut areas = HashSet::new();

            for (source_id, node) in nodes_by_source.into_iter() {
                let (mut area_id, mut p2p, mut transit, mut stub) = (None, 0usize, 0usize, 0usize);
                if let NodeInfo::Router(router) = &node.info {
                    if let Some(pd) = &router.protocol_data {
                        if let ProtocolData::Ospf(odata) = pd {
                            area_id = Some(odata.area_id);
                            if let OspfPayload::Router(rp) = &odata.payload {
                                p2p = rp.p2p_link_count;
                                transit = rp.transit_link_count;
                                stub = rp.stub_link_count;
                            }
                        }
                    }
                }

                if let Some(area_id) = area_id {
                    areas.insert(area_id);
                }

                facets.push(RouterFacet {
                    source_id: source_id.clone(),
                    area_id,
                    p2p_link_count: p2p,
                    transit_link_count: transit,
                    stub_link_count: stub,
                    node: node.clone(),
                });
            }

            let all_identical = if facets.is_empty() {
                true
            } else {
                let first: &RouterFacet = &facets[0];
                facets.iter().all(|f| {
                    f.area_id == first.area_id
                        && f.p2p_link_count == first.p2p_link_count
                        && f.transit_link_count == first.transit_link_count
                        && f.stub_link_count == first.stub_link_count
                })
            };

            out.push(DuplicateRouterBucket {
                router_uuid: router_uuid,
                facets,
                area_set: areas,
                all_facets_identical: all_identical,
            });
        }
        out
    }

    /// Fuse a duplicate router bucket into a single router node. Rules are:
    /// TODO
    fn fuse_router_bucket(bucket: DuplicateRouterBucket) -> Node {
        let base_router = match &bucket.facets[0].node.info {
            NodeInfo::Router(r) => r.clone(),
            _ => panic!("Bucket facet is not a router"),
        };

        let mut area_map: HashMap<std::net::Ipv4Addr, (usize, usize, usize)> = HashMap::new();
        let mut is_asbr = false;
        let mut is_virtual = false;
        let mut is_nssa = false;
        let mut link_metrics_merged: HashMap<std::net::Ipv4Addr, u16> = HashMap::new();

        for facet in &bucket.facets {
            if let Some(area) = facet.area_id {
                let entry = area_map.entry(area).or_insert((0, 0, 0));
                entry.0 += facet.p2p_link_count;
                entry.1 += facet.transit_link_count;
                entry.2 += facet.stub_link_count;
            }
            // Merge flags & metrics from facet's payload
            if let NodeInfo::Router(r) = &facet.node.info {
                if let Some(pd) = &r.protocol_data {
                    if let ProtocolData::Ospf(odata) = pd {
                        if let OspfPayload::Router(rp) = &odata.payload {
                            is_asbr |= rp.is_asbr;
                            is_virtual |= rp.is_virtual_link_endpoint;
                            is_nssa |= rp.is_nssa_capable;
                            // Merge metrics (last wins)
                            for (k, v) in &rp.link_metrics {
                                link_metrics_merged.insert(*k, *v);
                            }
                        }
                    }
                }
            }
        }

        // Build per-area facets vector and total counts
        let mut per_area_facets = Vec::with_capacity(area_map.len());
        let (mut total_p2p, mut total_transit, mut total_stub) = (0usize, 0usize, 0usize);
        for (area_id, (p2p, transit, stub)) in area_map {
            per_area_facets.push(PerAreaRouterFacet {
                area_id,
                p2p_link_count: p2p,
                transit_link_count: transit,
                stub_link_count: stub,
            });
            total_p2p += p2p;
            total_transit += transit;
            total_stub += stub;
        }

        let mut fused_router = base_router.clone();
        if let Some(pd) = &mut fused_router.protocol_data {
            if let ProtocolData::Ospf(odata) = pd {
                if let OspfPayload::Router(rp) = &mut odata.payload {
                    rp.is_asbr = is_asbr;
                    rp.is_virtual_link_endpoint = is_virtual;
                    rp.is_nssa_capable = is_nssa;
                    rp.is_abr = per_area_facets.len() > 1;
                    rp.p2p_link_count = total_p2p;
                    rp.transit_link_count = total_transit;
                    rp.stub_link_count = total_stub;
                    rp.link_metrics = link_metrics_merged;
                    rp.per_area_facets = per_area_facets;
                }
            }
        }

        let mut fused_node = bucket.facets[0].node.clone();
        fused_node.info = NodeInfo::Router(fused_router);

        fused_node
    }

    pub fn build_merged_view(&self, connected_only: bool) -> Vec<Node> {
        let mut out = Vec::new();
        let buckets = self.detect_router_duplicates(connected_only);
        let duplicate_router_uuids: HashSet<Uuid> = buckets.iter().map(|b| b.router_uuid).collect();
        
        let mut network_groups: HashMap<IpNetwork, Vec<Node>> = HashMap::new();
        
        // Handle non-duplicates first
        let partitions = self
            .sources
            .iter()
            .filter(|(_, src)| !connected_only || src.health == SourceHealth::Connected)
            .map(|(_, src)| &src.partition);

        for partition in partitions {
            for (node_uuid, node) in partition.nodes.iter() {
                match &node.info {
                    NodeInfo::Router(_) => {
                        if !duplicate_router_uuids.contains(node_uuid) {
                            out.push(node.clone());
                        }
                    }
                    NodeInfo::Network(net) => {
                        network_groups.entry(net.ip_address).or_default().push(node.clone());
                    }
                }
            }
        }

        // Now duplicates
        let fused_duplicate_routers = buckets.into_iter().map(TopologyStore::fuse_router_bucket);
        out.extend(fused_duplicate_routers);
        
        for (_, group_nodes) in network_groups {
            let fused = Self::fuse_network_group(group_nodes);
            out.push(fused);
        }

        out
    }

    // Build merged view, dedupe by Node.id with explicit selection policy:
    // 1) prefer Connected over Lost
    // 2) if same, prefer newer last_snapshot
    // 3) if same, prefer smaller SourceId (deterministic)
    #[deprecated(note = "Use build_merged_view instead")]
    pub fn union_nodes(&self, connected_only: bool) -> Vec<Node> {
        let mut best: HashMap<Uuid, (Node, bool, SystemTime, &SourceId)> = HashMap::new();

        for (src_id, state) in &self.sources {
            let is_connected = matches!(state.health, SourceHealth::Connected);
            if connected_only && !is_connected {
                continue;
            }

            for (id, node) in &state.partition.nodes {
                match best.get(id) {
                    None => {
                        let mut n = node.clone();
                        n.source_id = Some(src_id.clone());
                        best.insert(*id, (n, is_connected, state.last_snapshot, src_id));
                    }
                    Some((_, best_connected, best_ts, best_src)) => {
                        let take = (!*best_connected && is_connected)
                            || (*best_connected == is_connected && state.last_snapshot > *best_ts)
                            || (*best_connected == is_connected
                                && state.last_snapshot == *best_ts
                                && format!("{:?}", src_id) < format!("{:?}", best_src));
                        if take {
                            let mut n = node.clone();
                            n.source_id = Some(src_id.clone());
                            best.insert(*id, (n, is_connected, state.last_snapshot, src_id));
                        }
                    }
                }
            }
        }

        best.into_values().map(|(n, _, _, _)| n).collect()
    }

    /// Fuse a group of network nodes into a single node.
    fn fuse_network_group(nodes: Vec<Node>) -> Node {
        use std::collections::HashSet;

        let mut detailed: Vec<Node> = Vec::new();
        let mut summary: Vec<Node> = Vec::new();

        for n in nodes {
            match classify_network_kind(&n) {
                NetKind::Detailed => detailed.push(n),
                NetKind::Summary => summary.push(n),
                NetKind::Other => {
                    // Non-OSPF or unexpected advertisement inside a Network node.
                    // Panic here intentionally per your requirement.
                    panic!(
                        "[fuse_network_group] Encountered unsupported network node uuid={} (non-OSPF or unsupported LSA type)",
                        n.id
                    );
                }
            }
        }

        // Helper: union attached router IDs
        let union_attached = |base: &mut Vec<RouterId>, extra: &[RouterId]| {
            let mut seen: HashSet<Uuid> = base.iter().map(|r| r.to_uuidv5()).collect();
            for rid in extra {
                let id = rid.to_uuidv5();
                if seen.insert(id) {
                    base.push(rid.clone());
                }
            }
        };

        // Helper: union summary metrics from multiple OSPF network payloads
        let union_summaries = |base_payload: &mut OspfNetworkPayload,
                               extras: &[&OspfNetworkPayload]| {
            let mut seen: HashSet<(u32, Uuid)> = base_payload
                .summaries
                .iter()
                .map(|s| (s.metric, s.origin_abr.to_uuidv5()))
                .collect();
            for extra_pd in extras {
                for s in &extra_pd.summaries {
                    let sig = (s.metric, s.origin_abr.to_uuidv5());
                    if seen.insert(sig) {
                        base_payload.summaries.push(s.clone());
                    }
                }
            }
        };

        // Case 1: Detailed present → pick one base, fold others + all summaries
        if let Some(mut base) = detailed.pop() {
            if let NodeInfo::Network(base_net) = &mut base.info {
                // Union attached routers from other detailed nodes
                for d in &detailed {
                    if let NodeInfo::Network(net) = &d.info {
                        union_attached(&mut base_net.attached_routers, &net.attached_routers);
                    } else {
                        panic!("[fuse_network_group] Expected Network node in detailed list");
                    }
                }

                // Match explicitly on protocol; panic for unknown protocol variants
                match &mut base_net.protocol_data {
                    Some(ProtocolData::Ospf(base_pd)) => {
                        if let OspfPayload::Network(base_np) = &mut base_pd.payload {
                            // Collect summary payload references
                            let mut summary_payload_refs: Vec<&OspfNetworkPayload> = Vec::new();
                            for s in &summary {
                                if let NodeInfo::Network(net) = &s.info {
                                    match &net.protocol_data {
                                        Some(ProtocolData::Ospf(opd)) => {
                                            if let OspfPayload::Network(np) = &opd.payload {
                                                summary_payload_refs.push(np);
                                            } else {
                                                panic!(
                                                    "[fuse_network_group] Network node with OSPF data but non-Network payload"
                                                );
                                            }
                                        }
                                        Some(ProtocolData::IsIs(_)) => {
                                            panic!(
                                                "[fuse_network_group] Encountered IS-IS network payload (unsupported here)"
                                            );
                                        }
                                        Some(ProtocolData::Other(desc)) => {
                                            panic!(
                                                "[fuse_network_group] Encountered unsupported protocol payload: {}",
                                                desc
                                            );
                                        }
                                        None => {
                                            panic!(
                                                "[fuse_network_group] Missing protocol_data in summary network node"
                                            );
                                        }
                                    }
                                } else {
                                    panic!(
                                        "[fuse_network_group] Summary list contained non-network node"
                                    );
                                }
                            }
                            union_summaries(base_np, &summary_payload_refs);
                        } else {
                            panic!(
                                "[fuse_network_group] Base detailed network has OSPF protocol but non-Network payload"
                            );
                        }
                    }
                    Some(ProtocolData::IsIs(_)) => {
                        panic!(
                            "[fuse_network_group] Encountered IS-IS detailed network (unsupported)"
                        );
                    }
                    Some(ProtocolData::Other(desc)) => {
                        panic!(
                            "[fuse_network_group] Encountered unsupported detailed network protocol: {}",
                            desc
                        );
                    }
                    None => {
                        panic!("[fuse_network_group] Detailed network missing protocol_data");
                    }
                }
            } else {
                panic!("[fuse_network_group] Detailed vector first element not a Network node");
            }
            return base;
        }

        // Case 2: Only summaries exist → produce one summary base
        let mut base = summary
            .pop()
            .expect("[fuse_network_group] Called with empty group (no nodes)");

        if let NodeInfo::Network(base_net) = &mut base.info {
            // Union ABR attachments from remaining summary nodes
            for s in &summary {
                if let NodeInfo::Network(net) = &s.info {
                    union_attached(&mut base_net.attached_routers, &net.attached_routers);
                } else {
                    panic!("[fuse_network_group] Summary-only group contained non-network node");
                }
            }

            // Merge summary metrics
            match &mut base_net.protocol_data {
                Some(ProtocolData::Ospf(base_pd)) => {
                    if let OspfPayload::Network(base_np) = &mut base_pd.payload {
                        let mut extra_refs: Vec<&OspfNetworkPayload> = Vec::new();
                        for s in &summary {
                            if let NodeInfo::Network(net) = &s.info {
                                match &net.protocol_data {
                                    Some(ProtocolData::Ospf(opd)) => {
                                        if let OspfPayload::Network(np) = &opd.payload {
                                            extra_refs.push(np);
                                        } else {
                                            panic!(
                                                "[fuse_network_group] Summary node OSPF data but non-Network payload"
                                            );
                                        }
                                    }
                                    Some(ProtocolData::IsIs(_)) => {
                                        panic!(
                                            "[fuse_network_group] Encountered IS-IS summary network (unsupported)"
                                        );
                                    }
                                    Some(ProtocolData::Other(desc)) => {
                                        panic!(
                                            "[fuse_network_group] Encountered unsupported summary network protocol: {}",
                                            desc
                                        );
                                    }
                                    None => {
                                        panic!(
                                            "[fuse_network_group] Missing protocol_data in summary network node"
                                        );
                                    }
                                }
                            }
                        }
                        union_summaries(base_np, &extra_refs);
                    } else {
                        panic!(
                            "[fuse_network_group] Summary base network has OSPF protocol but non-Network payload"
                        );
                    }
                }
                Some(ProtocolData::IsIs(_)) => {
                    panic!("[fuse_network_group] Encountered IS-IS network in summary-only group");
                }
                Some(ProtocolData::Other(desc)) => {
                    panic!(
                        "[fuse_network_group] Encountered unsupported protocol in summary-only group: {}",
                        desc
                    );
                }
                None => {
                    panic!("[fuse_network_group] Summary base network missing protocol_data");
                }
            }
        } else {
            panic!("[fuse_network_group] Summary-only base node not a Network");
        }

        base
    }
}

/// Kinds of network nodes.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum NetKind {
    Detailed, // Type-2 Network-LSA
    Summary,  // Type-3 Summary LSA
    Other,
}

/// Classify a network node based on its protocol data.
fn classify_network_kind(node: &Node) -> NetKind {
    match &node.info {
        NodeInfo::Network(net) => {
            if let Some(ProtocolData::Ospf(data)) = &net.protocol_data {
                match *data.advertisement {
                    OspfLinkStateAdvertisement::NetworkLinks(_) => NetKind::Detailed,
                    OspfLinkStateAdvertisement::SummaryLinkIpNetwork(_) => NetKind::Summary,
                    _ => NetKind::Other,
                }
            } else {
                NetKind::Other
            }
        }
        _ => NetKind::Other,
    }
}

mod tests {
    use super::*;

}
