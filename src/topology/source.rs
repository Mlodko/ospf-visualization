/*!
GUI-facing topology provider interface.

This module defines:
- `TopologyError`: minimal error type for topology retrieval.
- `TopologySource`: an async trait that returns protocol-agnostic nodes for rendering.

Adapters (e.g., OSPF-over-SNMP, OSPF-over-RESTCONF) should implement `TopologySource`
and encapsulate how they obtain and parse data.
*/

use std::fmt::Display;

use async_trait::async_trait;
use egui::epaint::stats;

use crate::network::{node::Node, router::{InterfaceStats, RouterId}};

/// Error type for topology retrieval.
#[derive(Debug, Clone)]
pub enum TopologyError {
    /// Underlying data acquisition/IO/transport error (SNMP/HTTP/etc).
    Acquisition(String),
    /// Protocol/parse/semantic conversion error.
    Protocol(String),
}

impl Display for TopologyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TopologyError::Acquisition(msg) => write!(f, "acquisition error: {msg}"),
            TopologyError::Protocol(msg) => write!(f, "protocol error: {msg}"),
        }
    }
}

impl std::error::Error for TopologyError {}

/// A small async interface for providing topology data to the GUI.
/// Implementations hide transport/protocol details and return protocol-agnostic nodes.
#[async_trait]
pub trait TopologySource: Send + Sync {
    async fn fetch_nodes(&mut self) -> TopologyResult<Vec<Node>>;
}

type SourceId = RouterId;

/// An async trait for providing a snapshot of topology data to the GUI.
/// Implementations hide transport/protocol details and return protocol-agnostic nodes.
/// Prefer this over TopologySource - this allows for integration with topology source mechanisms.
#[async_trait]
pub trait SnapshotSource: TopologySource {
    /// Fetches the source ID for the topology data.
    async fn fetch_source_id(&mut self) -> TopologyResult<SourceId>;
    
    /// Fetches nodes and source id and returns a tuple.
    async fn fetch_snapshot(&mut self) -> TopologyResult<(SourceId, Vec<Node>, Vec<InterfaceStats>)> {
        let source_id = self.fetch_source_id().await?;
        let nodes = self.fetch_nodes().await?;
        let stats = self.fetch_stats().await?;
        Ok((source_id, nodes, stats))
    }
    
    async fn fetch_stats(&mut self) -> TopologyResult<Vec<InterfaceStats>>;
}

/// Convenience result alias for topology operations.
pub type TopologyResult<T> = Result<T, TopologyError>;
