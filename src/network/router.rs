use std::{
    fmt::Display,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use uuid::Uuid;

use crate::network::node::ProtocolData;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum RouterId {
    Ipv4(Ipv4Addr),
    Ipv6(Ipv6Addr),
    IsIs([u8; 6]),
    Other(String),
}

impl RouterId {
    pub fn as_bytes(&self) -> Vec<u8> {
        match self {
            RouterId::Ipv4(addr) => addr.octets().to_vec(),
            RouterId::Ipv6(addr) => addr.octets().to_vec(),
            RouterId::IsIs(id) => id.to_vec(),
            RouterId::Other(string) => string.as_bytes().to_vec(),
        }
    }

    pub fn to_uuidv5(&self) -> Uuid {
        Uuid::new_v5(&Uuid::NAMESPACE_OID, &self.as_bytes())
    }
}

impl Display for RouterId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouterId::Ipv4(addr) => write!(f, "IPv4: {}", addr),
            RouterId::Ipv6(addr) => write!(f, "IPv6: {}", addr),
            RouterId::IsIs(id) => write!(
                f,
                "IS-IS: {:?} ({})",
                id,
                id.iter()
                    .map(|b| format!("{:02X}", b))
                    .collect::<Vec<_>>()
                    .join(":")
            ),
            RouterId::Other(string) => write!(f, "Other: {}", string),
        }
    }
}

/// Represents a router in the protocol-agnostic network graph.
#[derive(Debug, Clone)]
pub struct Router {
    pub id: RouterId,
    pub interfaces: Vec<IpAddr>,
    pub protocol_data: Option<ProtocolData>,
}

impl Display for Router {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Router ID: {}", self.id)?;
        write!(
            f,
            "\nInterfaces: {}",
            self.interfaces
                .iter()
                .map(|i| format!("{}", i))
                .collect::<Vec<_>>()
                .join(", ")
        )?;
        if let Some(data) = &self.protocol_data {
            write!(f, "\nProtocol Data: {}", data)?;
        }
        Ok(())
    }
}

impl Display for ProtocolData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolData::Ospf(data) => write!(f, "OSPF: {:?}", data),
            ProtocolData::IsIs(data) => write!(f, "IS-IS: {:?}", data),
            ProtocolData::Other(string) => write!(f, "Other: {}", string),
        }
    }
}
