/*!
This module defines traits for abstracting (routing protocol, acquisition method) behavior.
*/

use async_trait::async_trait;
use thiserror::Error;

use crate::{
    network::node::Node,
    topology::{
        TopologySource,
        source::{SnapshotSource, TopologyError},
        store::SourceId,
    },
};

#[derive(Debug)]
pub enum AcquisitionError {
    Transport(String),
    Invalid(String),
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum ProtocolParseError {
    Malformed(String),
    Unsupported(String),
}

#[derive(Debug)]
pub enum ProtocolTopologyError {
    Conversion(String),
    Semantic(String),
}

impl From<AcquisitionError> for TopologyError {
    fn from(e: AcquisitionError) -> Self {
        match e {
            AcquisitionError::Transport(s) => TopologyError::Acquisition(s),
            AcquisitionError::Invalid(s) => TopologyError::Protocol(s),
        }
    }
}
impl From<ProtocolParseError> for TopologyError {
    fn from(e: ProtocolParseError) -> Self {
        match e {
            ProtocolParseError::Malformed(s) => TopologyError::Protocol(s),
            ProtocolParseError::Unsupported(s) => TopologyError::Protocol(s),
        }
    }
}
impl From<ProtocolTopologyError> for TopologyError {
    fn from(e: ProtocolTopologyError) -> Self {
        match e {
            ProtocolTopologyError::Conversion(s) => TopologyError::Protocol(s),
            ProtocolTopologyError::Semantic(s) => TopologyError::Protocol(s),
        }
    }
}

type AcquisitionResult<T> = Result<T, AcquisitionError>;

#[async_trait]
pub trait AcquisitionSource<P: RoutingProtocol>: Send + Sync {
    async fn fetch_raw(&mut self) -> AcquisitionResult<Vec<P::RawRecord>>;
    async fn fetch_source_id(&mut self) -> AcquisitionResult<SourceId>;
}

/// Routing protocol contract.
pub trait RoutingProtocol: Send + Sync {
    type RawRecord: Send;
    type ParsedItem: Send;

    fn parse(&self, raw: Self::RawRecord) -> Result<Vec<Self::ParsedItem>, ProtocolParseError>;
    // CHANGED: consume ParsedItem so protocol implementations can rely on existing TryInto
    fn item_to_node(&self, item: Self::ParsedItem) -> Result<Option<Node>, ProtocolTopologyError>;
    fn post_process(&self, nodes: &mut Vec<Node>) -> Result<(), ProtocolTopologyError>;
}

pub struct Topology<P, S>
where
    P: RoutingProtocol,
    S: AcquisitionSource<P>,
{
    protocol: P,
    source: S,
}

impl<P, S> Topology<P, S>
where
    P: RoutingProtocol,
    S: AcquisitionSource<P>,
{
    pub fn new(protocol: P, source: S) -> Self {
        Self { protocol, source }
    }

    #[allow(unused)]
    pub fn protocol(&self) -> &P {
        &self.protocol
    }

    #[allow(unused)]
    pub fn source(&self) -> &S {
        &self.source
    }
}

/// ProtocolFederator performs cross-source semantic merging for a single protocol.
/// All pre-source semantic normalization (summary folding, stub synthesis) must be done in RoutingProtocol::post_process.
/// The implementor must treat the provided facets as different source views of the same identity (RouterId or prefix).
pub trait ProtocolFederator: Send + Sync {
    /// Merge multiple router nodes (same RouterId) from different sources.
    fn merge_routers(&self, facets: &[Node]) -> Node;

    /// Merge multiple network nodes (same prefix) from different sources.
    fn merge_networks(&self, facets: &[Node]) -> Node;

    /// Check if all provided router nodes are compatible with the federator.
    fn can_merge_router_facets(&self, facets: &[Node]) -> Result<(), FederationError>;

    fn can_merge_network_facets(&self, facets: &[Node]) -> Result<(), FederationError>;
}

#[derive(Debug, Clone, Error)]
pub enum FederationError {
    #[error("Facets cannot be empty")]
    EmptyFacets,
    #[error("Mixed protocols")]
    MixedProtocols,
    #[error("Mixed node kinds")]
    MixedNodeKinds,
    #[error("Unsupported payload")]
    UnsupportedPayload,
    #[error("Facets have differing RouterId / prefix identity")]
    MixedIdentity,
}

#[async_trait]
impl<P, S> TopologySource for Topology<P, S>
where
    P: RoutingProtocol,
    S: AcquisitionSource<P>,
{
    async fn fetch_nodes(&mut self) -> Result<Vec<Node>, TopologyError> {
        println!("[topology] fetch_nodes: starting");

        // Fetch raw records from the underlying acquisition source.
        let raw = match self.source.fetch_raw().await {
            Ok(r) => {
                println!("[topology] fetch_raw: received {} raw record(s)", r.len());
                r
            }
            Err(e) => {
                eprintln!("[topology] fetch_raw error: {:?}", e);
                return Err(TopologyError::from(e));
            }
        };

        let mut nodes: Vec<Node> = Vec::new();

        // Parse each raw record via the protocol implementation.
        for (rec_idx, record) in raw.into_iter().enumerate() {
            println!("[topology] parsing record #{}", rec_idx);
            let parsed_items = match self.protocol.parse(record) {
                Ok(items) => {
                    println!(
                        "[topology] parsed {} item(s) from record #{}",
                        items.len(),
                        rec_idx
                    );
                    items
                }
                Err(e) => {
                    eprintln!(
                        "[topology] protocol.parse failed for record #{}: {:?}",
                        rec_idx, e
                    );
                    return Err(TopologyError::from(e));
                }
            };

            for (item_idx, item) in parsed_items.into_iter().enumerate() {
                match self.protocol.item_to_node(item) {
                    Ok(Some(node)) => {
                        println!(
                            "[topology] item_to_node: record #{}, item #{} -> produced node",
                            rec_idx, item_idx
                        );
                        nodes.push(node);
                    }
                    Ok(None) => {
                        println!(
                            "[topology] item_to_node: record #{}, item #{} -> no node produced",
                            rec_idx, item_idx
                        );
                    }
                    Err(e) => {
                        eprintln!(
                            "[topology] item_to_node error: record #{}, item #{}: {:?}",
                            rec_idx, item_idx, e
                        );
                        return Err(TopologyError::from(e));
                    }
                }
            }
        }

        // Allow the protocol to post-process the collected nodes before returning.
        println!(
            "[topology] running protocol.post_process on {} node(s)",
            nodes.len()
        );
        if let Err(e) = self.protocol.post_process(&mut nodes) {
            eprintln!("[topology] protocol.post_process failed: {:?}", e);
            return Err(TopologyError::from(e));
        }
        println!(
            "[topology] post_process complete, returning {} node(s)",
            nodes.len()
        );

        Ok(nodes)
    }
}

#[async_trait]
impl<P, S> SnapshotSource for Topology<P, S>
where
    P: RoutingProtocol,
    S: AcquisitionSource<P>,
{
    async fn fetch_source_id(&mut self) -> Result<SourceId, TopologyError> {
        self.source
            .fetch_source_id()
            .await
            .map_err(TopologyError::from)
    }
}
