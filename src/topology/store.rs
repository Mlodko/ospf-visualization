/*!
This module provides mechanisms necessary for storing, managing and merging topology data from different sources.

This module defines:
- `Partition`: Collection of nodes originating from one source (e.g. a router from which the topology data is collected)
- `SourceHealth`: Represents a source's status
- `SourceState`: Holds information about a source, as well as the partition it manages.
*/

use crate::{network::{
    node::{Node, NodeInfo},
    router::RouterId,
}, topology::{ospf_protocol::OspfFederator, protocol::{FederationError, ProtocolFederator}}};
use ipnetwork::IpNetwork;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use std::{
    collections::{HashMap, HashSet},
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    SourceNotFound(SourceId),
    #[error("Source {0} already in desired state {1}")]
    SourceAlreadyInDesiredState(SourceId, bool),
}

pub struct MergeConfig {
    federator: Option<Box<dyn ProtocolFederator>>,
    disabled_sources: HashSet<SourceId>,
    connected_only: bool
}

impl Default for MergeConfig {
    fn default() -> Self {
        Self { 
            federator: Some(Box::new(OspfFederator::new())), 
            disabled_sources: Default::default(), 
            connected_only: false 
        }
    }
}

impl MergeConfig {
    pub fn new(
        federator: Option<Box<dyn ProtocolFederator>>,
        enabled_sources: HashSet<SourceId>,
        connected_only: bool
    ) -> Self {
        Self {
            federator,
            disabled_sources: enabled_sources,
            connected_only
        }
    }
    pub fn get_federator(&self) -> Option<&dyn ProtocolFederator> {
        self.federator.as_deref()
    }
    pub fn set_federator(&mut self, federator: Option<Box<dyn ProtocolFederator>>) {
        self.federator = federator;
    }
    pub fn get_disabled_sources(&self) -> &HashSet<SourceId> {
        &self.disabled_sources
    }
    pub fn set_disabled_sources(&mut self, sources: &[SourceId]) {
        self.disabled_sources.clear();
        self.disabled_sources.extend(sources.iter().cloned());
    }
    pub fn enable_source(&mut self, source: &SourceId) -> Result<(), StoreError> {
        if let Some(source) = self.disabled_sources.get(source) {
            self.disabled_sources.remove(&source.clone());
            Ok(())
        } else {
            Err(StoreError::SourceAlreadyInDesiredState(source.clone(), true))
        }
    }
    pub fn disable_source(&mut self, source: &SourceId) -> Result<(), StoreError> {
        if let Some(source) = self.disabled_sources.get(source) {
            Err(StoreError::SourceAlreadyInDesiredState(source.clone(), false))
        } else {
            self.disabled_sources.insert(source.clone());
            Ok(())
        }
    }
    pub fn toggle_source(&mut self, source: &SourceId) {
        if let Some(source) = self.disabled_sources.get(source) {
            self.disabled_sources.remove(&source.clone());
        } else {
            self.disabled_sources.insert(source.clone());
        }
    }
    pub fn is_source_enabled(&self, source: &SourceId) -> bool {
        !self.disabled_sources.contains(source)
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
    pub fn replace_partition(&mut self, src_id: &SourceId, nodes: Vec<Node>, timestamp: SystemTime) {
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
                    .insert(src_id.clone(), SourceState::new(part, timestamp));
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
    
    pub fn build_merged_view_with(&self, config: &MergeConfig) -> Result<Vec<Node>, FederationError> {
        let mut routers_by_rid: HashMap<RouterId, Vec<Node>> = HashMap::new();
        let mut networks_by_prefix: HashMap<IpNetwork, Vec<Node>> = HashMap::new();

        for (src_id, state) in &self.sources {
            if (config.connected_only && state.health != SourceHealth::Connected) || !config.is_source_enabled(src_id) {
                continue;
            }

            for node in state.partition.nodes.values() {
                match &node.info {
                    NodeInfo::Router(r) => {
                        routers_by_rid.entry(r.id.clone()).or_default().push(node.clone());
                    }
                    NodeInfo::Network(net) => {
                        networks_by_prefix.entry(net.ip_address).or_default().push(node.clone());
                    }
                }
            }
        }

        let mut out = Vec::new();
        
        let federator = config.get_federator();
        
        // Routers
        for (_rid, facets) in routers_by_rid {
            if let Some(f) = federator {
                match f.can_merge_router_facets(&facets) {
                    Ok(()) => {
                        out.push(f.merge_routers(&facets));
                        continue;
                    }
                    Err(_e) => {
                        // Fallback: select a representative facet
                        out.push(Self::select_best_router(&facets));
                        continue;
                    }
                }
            }
            out.push(Self::select_best_router(&facets));
        }

        // Networks
        for (_prefix, facets) in networks_by_prefix {
            if let Some(f) = federator {
                match f.can_merge_network_facets(&facets) {
                    Ok(()) => {
                        out.push(f.merge_networks(&facets));
                        continue; // IMPORTANT: prevent double insert
                    }
                    Err(_e) => {
                        // Fallback if federation not applicable (stub synthetic, mixed protocol, etc.)
                        out.push(Self::select_best_network(&facets));
                        continue;
                    }
                }
            }
            out.push(Self::select_best_network(&facets));
        }

        Ok(out)
    }
    
    fn select_best_router(facets: &[Node]) -> Node {
        // For now return the first router
        facets[0].clone()
    }
    
    fn select_best_network(facets: &[Node]) -> Node {
        facets[0].clone()
    }
}

mod tests {
    #[allow(unused_imports)]
    use std::net::Ipv4Addr;

    #[allow(unused_imports)]
    use super::*;
    
    #[test]
    fn test_store_deserialization() {
        let json = include_str!("../../test_data/test_store.json");
        
        let store: TopologyStore = serde_json::from_str(json).unwrap();
        
        // Sources
        let sources: Vec<_> = store.sources.keys().collect();
        let expected_sources = [
            SourceId::Ipv4(Ipv4Addr::new(172, 21, 0, 1)),
            SourceId::Ipv4(Ipv4Addr::new(10, 0, 56, 6))
        ];
        
        assert_eq!(sources.len(), expected_sources.len());
        
        for expected in expected_sources.iter() {
            assert!(sources.contains(&expected));
        }
        
        // Nodes before merging
        let node_uuids: Vec<_> = store.sources.values()
            .flat_map(|source| 
                source.partition.nodes.keys()
                    .map(|uuid| uuid.to_string())
            )
            .collect();
        let expected_uuids = [
            // Source 172.21.0.1
            "62d75015-d189-5618-b756-8bd562aa6fe2",
            "cb84d575-3dec-5a6d-9252-5798bd5711fa",
            "26b1296a-d160-5a6a-8c9c-3359b1bc3946",
            "dca7c5a5-366f-5c8b-a4ea-25c8e8375af4",
            "95dff25a-9c61-5d84-b2d8-15eacaa3fd06",
            "1500a360-a0e0-50c3-aa01-c1a355b81733",
            "b58c1c6f-1242-5bdd-807e-5ed4f6c26b05",
            "6018ecef-d6be-5d56-b725-97b694be08c0",
            "95fdd0b0-703c-5d59-a0b8-f19110fc7e68",
            "a828b733-997f-5cab-becd-910a6826aa3d",
            "41e5203a-eac4-5cae-bf47-76066ea8852c",
            // Source 10.0.56.6
            "ea639b3e-3d98-5f5c-b3d5-07fd9cee8ec3",
            "1500a360-a0e0-50c3-aa01-c1a355b81733",
            "26b1296a-d160-5a6a-8c9c-3359b1bc3946",
            "bbe4a22e-891c-564d-9065-b2d01d394d31",
            "a828b733-997f-5cab-becd-910a6826aa3d",
            "6018ecef-d6be-5d56-b725-97b694be08c0",
            "cb84d575-3dec-5a6d-9252-5798bd5711fa",
            "dca7c5a5-366f-5c8b-a4ea-25c8e8375af4",
            "62d75015-d189-5618-b756-8bd562aa6fe2",
        ];
        
        assert_eq!(node_uuids.len(), expected_uuids.len());
        for expected in expected_uuids {
            assert!(node_uuids.contains(&expected.to_string()))
        }
        
        // Nodes after merging
        
    }
    
    #[test]
    fn test_store_merging_logic() {
        let json = include_str!("../../test_data/test_store.json");
        let store: TopologyStore = serde_json::from_str(json).unwrap();
        
        let merged_nodes = store.build_merged_view_with(&MergeConfig::default()).unwrap();
        let merged_uuids: Vec<_> = merged_nodes.iter().map(|node| node.id.to_string()).collect();
        let expected_merged_uuids = [
            "95dff25a-9c61-5d84-b2d8-15eacaa3fd06",
            "a828b733-997f-5cab-becd-910a6826aa3d",
            "6018ecef-d6be-5d56-b725-97b694be08c0",
            "95fdd0b0-703c-5d59-a0b8-f19110fc7e68",
            "26b1296a-d160-5a6a-8c9c-3359b1bc3946",
            "1500a360-a0e0-50c3-aa01-c1a355b81733",
            "62d75015-d189-5618-b756-8bd562aa6fe2",
            "41e5203a-eac4-5cae-bf47-76066ea8852c",
            "dca7c5a5-366f-5c8b-a4ea-25c8e8375af4",
            "b58c1c6f-1242-5bdd-807e-5ed4f6c26b05",
            "cb84d575-3dec-5a6d-9252-5798bd5711fa",
            "bbe4a22e-891c-564d-9065-b2d01d394d31",
            "ea639b3e-3d98-5f5c-b3d5-07fd9cee8ec3"
        ];
        
        assert_eq!(merged_uuids.len(), expected_merged_uuids.len());
        for expected in expected_merged_uuids {
            assert!(merged_uuids.contains(&expected.to_string()))
        }
    }
}
