/*!
Protocol-neutral OSPF data source.

This module defines:
- `OspfRawRow`: a minimal, transport-agnostic representation of an LSDB row.
- `OspfSourceError`: an error type for acquisition/validation issues at the source layer.
- `OspfDataSource`: a tiny async trait that returns raw OSPF rows without knowing
  whether they came from SNMP, NETCONF, RESTCONF, etc.

Adapters (e.g., SNMP, RESTCONF) should implement `OspfDataSource` and map their
transport-specific responses into `OspfRawRow`.
*/

use std::{fmt::Display, net::Ipv4Addr};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Minimal, protocol-neutral representation of an OSPF LSDB row.
/// Transport adapters (SNMP/NETCONF/RESTCONF) should populate this without leaking transport types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OspfRawRow {
    pub area_id: Ipv4Addr,
    pub link_state_id: Ipv4Addr,
    pub router_id: Ipv4Addr,
    pub lsa_bytes: Vec<u8>,
}

/// Errors that can occur when fetching OSPF raw rows from a data source.
#[derive(Debug, Clone)]
pub enum OspfSourceError {
    /// Underlying data acquisition/IO/transport error (e.g., SNMP/HTTP failure).
    Acquisition(String),
    /// Data was retrieved but is malformed or missing required fields.
    Invalid(String),
}

impl Display for OspfSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OspfSourceError::Acquisition(msg) => write!(f, "acquisition error: {msg}"),
            OspfSourceError::Invalid(msg) => write!(f, "invalid data: {msg}"),
        }
    }
}

impl std::error::Error for OspfSourceError {}

/// A tiny async interface for fetching OSPF LSDB entries as raw rows.
/// This trait is intentionally minimal and protocol-centric; it does not expose transport details.
#[async_trait]
pub trait OspfDataSource: Send + Sync {
    async fn fetch_lsdb_rows(&mut self) -> Result<Vec<OspfRawRow>, OspfSourceError>;
}
