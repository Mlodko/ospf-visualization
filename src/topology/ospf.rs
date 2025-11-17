use async_trait::async_trait;

use crate::data_aquisition::snmp::SnmpClient;
use crate::network::node::Node;
use crate::parsers::ospf_parser::lsa::{LsaError, OspfLsdbEntry};
use crate::parsers::ospf_parser::snmp_source::OspfSnmpSource;
use crate::parsers::ospf_parser::source::{OspfDataSource, OspfSourceError};
use crate::topology::source::{SnapshotSource, TopologyError, TopologySource};
use crate::topology::store::SourceId;

/// OSPF-over-SNMP implementation of the GUI-facing topology source.
/// Internally, this uses the protocol-centric OspfDataSource (implemented by OspfSnmpSource)
/// and converts parsed LSAs into protocol-agnostic `Node`s.
pub struct OspfTopology<S: OspfDataSource> {
    source: S,
}

/// Convenience alias for the SNMP-backed OSPF topology.
pub type OspfSnmpTopology = OspfTopology<OspfSnmpSource>;

impl OspfTopology<OspfSnmpSource> {
    /// Construct an SNMP-backed OSPF topology from an SNMP client.
    pub fn new(client: SnmpClient) -> Self {
        Self {
            source: OspfSnmpSource::new(client),
        }
    }
}

#[async_trait]
impl<S: OspfDataSource + Send + Sync> TopologySource for OspfTopology<S> {
    async fn fetch_nodes(&mut self) -> Result<Vec<Node>, TopologyError> {
        let rows = self
            .source
            .fetch_lsdb_rows()
            .await
            .map_err(map_ospf_source_err)?;

        let mut nodes = Vec::with_capacity(rows.len());
        for row in rows {
            let entry = OspfLsdbEntry::try_from(row)
                .map_err(|e| TopologyError::Protocol(format!("{:?}", e)))?;
            match entry.try_into() as Result<Node, LsaError> {
                Ok(node) => nodes.push(node),
                Err(LsaError::InvalidLsaType) => {
                    // Skip unsupported LSA types for topology nodes
                }
                Err(e) => return Err(TopologyError::Protocol(format!("{:?}", e))),
            }
        }
        Ok(nodes)
    }
}

#[async_trait]
impl SnapshotSource for OspfTopology<OspfSnmpSource> {
    async fn fetch_source_id(&mut self) -> Result<SourceId, TopologyError> {
        self.source.fetch_source_id().await.map_err(map_ospf_source_err)
    }
}

fn map_ospf_source_err(err: OspfSourceError) -> TopologyError {
    match err {
        OspfSourceError::Acquisition(s) => TopologyError::Acquisition(s),
        OspfSourceError::Invalid(s) => TopologyError::Protocol(s),
    }
}
