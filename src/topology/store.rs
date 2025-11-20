#![allow(dead_code)]
use crate::network::{node::Node, router::RouterId};
use std::{collections::HashMap, time::Instant};
use uuid::Uuid;

pub type SourceId = RouterId;

#[derive(Clone, Debug, Default)]
pub struct Partition {
    pub nodes: HashMap<Uuid, Node>,
}
impl Partition {
    pub fn new(nodes: Vec<Node>) -> Self {
        let mut map = HashMap::with_capacity(nodes.len());
        for node in nodes {
            map.insert(node.id, node);
        }
        Partition { nodes: map }
    }
    
    pub fn to_string_summary(&self) -> String {
        let mut summary = String::new();
        for node in self.nodes.values() {
            let node_summary = match &node.info {
                crate::network::node::NodeInfo::Network(net) => {
                    format!("Network: {}", net.ip_address)
                },
                crate::network::node::NodeInfo::Router(router) => {
                    format!("Router: {}", router.id)
                }
            };
            summary += &format!("{}\n", node_summary);
        }
        summary
    } 
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceHealth {
    Connected,
    Lost,
}

#[derive(Debug)]
pub struct SourceState {
    pub health: SourceHealth,
    pub partition: Partition,
    pub last_snapshot: Instant, // when we last replaced the snapshot successfully
    pub last_connected: Instant, // when acquisition last succeeded
    pub last_status_change: Instant, // when health last changed
}
impl SourceState {
    pub fn new(partition: Partition, ts: Instant) -> Self {
        SourceState {
            partition,
            health: SourceHealth::Connected,
            last_snapshot: ts,
            last_connected: ts,
            last_status_change: ts,
        }
    }
}

#[derive(Debug, Default)]
pub struct TopologyStore {
    sources: HashMap<SourceId, SourceState>,
}

impl TopologyStore {
    pub fn replace_partition(&mut self, src_id: SourceId, nodes: Vec<Node>, timestamp: Instant) {
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

    pub fn mark_lost(&mut self, src_id: &SourceId, timestamp: Instant) {
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
    pub fn to_summary_string(&self) -> String {
        let mut summary = String::new();
        for (src_id, src_state) in &self.sources {
            let part_sum = src_state.partition.to_string_summary();
            summary += &format!("{}:\n{}", src_id, part_sum);
        }
        summary
    }

    // Build merged view, dedupe by Node.id with explicit selection policy:
    // 1) prefer Connected over Lost
    // 2) if same, prefer newer last_snapshot
    // 3) if same, prefer smaller SourceId (deterministic)
    pub fn union_nodes(&self, connected_only: bool) -> Vec<Node> {
        let mut best: HashMap<Uuid, (Node, bool, Instant, &SourceId)> = HashMap::new();

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
}
