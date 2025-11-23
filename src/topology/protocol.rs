/*!
This module defines traits for abstracting (routing protocol, acquisition method) behavior.
*/

use std::error::Error;

use async_trait::async_trait;

use crate::{network::node::Node, topology::{TopologySource, source::{SnapshotSource, TopologyError}, store::SourceId}};

/// Acquisition errors: transport and shaping
#[derive(Debug)] 
pub enum AcquisitionError {
    Transport(String),
    Invalid(String)
}

/// Protocol parsing errors
#[derive(Debug)]
pub enum ProtocolParseError {
    Malformed(String),
    Unsupported(String)
}

/// Protocol topology (mapping + semantic consolidation) errors
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

/// Represents a source of raw records for a routing protocol, e.g. OSPF over SNMP.
#[async_trait]
pub trait AcquisitionSource<P: RoutingProtocol>: Send + Sync {
    /// Fetches the raw records from the source.
    async fn fetch_raw(&mut self) -> AcquisitionResult<Vec<P::RawRecord>>;
    /// Fetches the source ID from the source.
    async fn fetch_source_id(&mut self) -> AcquisitionResult<SourceId>;
}

/// Represents a routing protocol, e.g. OSPF.
pub trait RoutingProtocol: Send + Sync {
    type RawRecord: Send;
    type ParsedItem: Send;
    /// Parses raw records into protocol-specific items.
    fn parse(&self, raw: Self::RawRecord) -> Result<Vec<Self::ParsedItem>, ProtocolParseError>;
    /// Converts protocol-specific items into topology nodes.
    fn item_to_node(&self, item: &Self::ParsedItem) -> Result<Option<Node>, ProtocolTopologyError>;
    /// Performs post-processing on the topology nodes.
    fn post_process(&self, nodes: &mut Vec<Node>) -> Result<(), ProtocolTopologyError>;
}

/// Generic topology wrapper representing a (routing protocol, acquisition source) pair.
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
                if let Some(node) = self.protocol.item_to_node(&item).map_err(TopologyError::from)? {
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