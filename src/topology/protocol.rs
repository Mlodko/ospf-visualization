/*!
This module defines traits for abstracting (routing protocol, acquisition method) behavior.
*/

use async_trait::async_trait;
use thiserror::Error;

use crate::{network::node::Node, topology::{TopologySource, source::{SnapshotSource, TopologyError}, store::SourceId}};

#[derive(Debug)] 
pub enum AcquisitionError {
    Transport(String),
    Invalid(String)
}

#[derive(Debug)]
pub enum ProtocolParseError {
    Malformed(String),
    Unsupported(String)
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
            AcquisitionError::Invalid(s)   => TopologyError::Protocol(s),
        }
    }
}
impl From<ProtocolParseError> for TopologyError {
    fn from(e: ProtocolParseError) -> Self {
        match e {
            ProtocolParseError::Malformed(s)  => TopologyError::Protocol(s),
            ProtocolParseError::Unsupported(s)=> TopologyError::Protocol(s),
        }
    }
}
impl From<ProtocolTopologyError> for TopologyError {
    fn from(e: ProtocolTopologyError) -> Self {
        match e {
            ProtocolTopologyError::Conversion(s) => TopologyError::Protocol(s),
            ProtocolTopologyError::Semantic(s)   => TopologyError::Protocol(s),
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
    source: S
}

impl<P, S> Topology<P, S>
where 
    P: RoutingProtocol,
    S: AcquisitionSource<P>,
{
    pub fn new(protocol: P, source: S) -> Self {
        Self {protocol, source}
    }
    
    pub fn protocol(&self) -> &P { &self.protocol }
    pub fn source(&self) -> &S { &self.source }
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
        let raw = self.source.fetch_raw().await.map_err(TopologyError::from)?;
        let mut nodes = Vec::new();
        for record in raw {
            let parsed_items = self.protocol.parse(record).map_err(TopologyError::from)?;
            for item in parsed_items {
                if let Some(node) = self.protocol.item_to_node(item).map_err(TopologyError::from)? {
                    nodes.push(node);
                }
            }
        }
        self.protocol.post_process(&mut nodes).map_err(TopologyError::from)?;
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
        self.source.fetch_source_id().await.map_err(TopologyError::from)
    }
}
 