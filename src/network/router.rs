use std::{
    fmt::Display,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{network::node::ProtocolData, parsers::isis_parser::core_lsp::{SystemId}};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InterfaceStats {
    pub ip_address: IpAddr,
    pub tx_bytes: Option<u64>,
    pub tx_packets: Option<u64>,
    pub rx_bytes: Option<u64>,
    pub rx_packets: Option<u64>,
}

impl InterfaceStats {
    pub fn get_weight(&self) -> u64 {
        let tx_bytes = self.tx_bytes.unwrap_or(0);
        let rx_bytes = self.rx_bytes.unwrap_or(0);
        let total_bytes = tx_bytes + rx_bytes;
        total_bytes
    }
    
    pub fn get_tx_to_rx_ratio(&self) -> f64 {
        let tx_bytes = self.tx_bytes.unwrap_or(0);
        let rx_bytes = self.rx_bytes.unwrap_or(0);
        if rx_bytes == 0 {
            0.0
        } else {
            tx_bytes as f64 / rx_bytes as f64
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum RouterId {
    Ipv4(Ipv4Addr),
    Ipv6(Ipv6Addr),
    IsIs(SystemId),
    Other(String),
}

// Implementing Serialize and Deserialize manually because they're used as keys, which must be strings

impl Serialize for RouterId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        let out = match self {
            RouterId::Ipv4(addr) => format!("IPv4:{}", addr),
            RouterId::Ipv6(addr) => format!("IPv6:{}", addr),
            RouterId::IsIs(id) => format!("ISIS:{}", id),
            RouterId::Other(string) => format!("Other:{}", string),
        };
        serializer.serialize_str(&out)
    }
}

impl<'de> Deserialize<'de> for RouterId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de> {
            let s = String::deserialize(deserializer)?;
            
            if let Some(pos) = s.find(':') {
                let (kind, rest_with_colon) = s.split_at(pos);
                let rest = &rest_with_colon[1..]; // skip the colon
    
                match kind {
                    "IPv4" => rest.parse::<Ipv4Addr>()
                        .map(RouterId::Ipv4)
                        .map_err(serde::de::Error::custom),
                    "IPv6" => rest.parse::<Ipv6Addr>()
                        .map(RouterId::Ipv6)
                        .map_err(serde::de::Error::custom),
                    "ISIS" => SystemId::from_string(rest)
                        .map(RouterId::IsIs)
                        .map_err(serde::de::Error::custom),
                    "Other" => Ok(RouterId::Other(rest.to_string())),
                    other => Err(serde::de::Error::custom(format!("Unknown RouterId prefix: {}", other))),
                }
            } else {
                // No prefix: try to be permissive — accept plain IPv4/IPv6 strings, otherwise store as Other
                if let Ok(ipv4) = s.parse::<Ipv4Addr>() {
                    Ok(RouterId::Ipv4(ipv4))
                } else if let Ok(ipv6) = s.parse::<Ipv6Addr>() {
                    Ok(RouterId::Ipv6(ipv6))
                } else {
                    Ok(RouterId::Other(s))
                }
            }
    }
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
    
    pub fn as_string(&self) -> String {
        match self {
            Self::Ipv4(ip) => ip.to_string(),
            Self::Ipv6(ip) => ip.to_string(),
            Self::IsIs(id) => format!("{}", id),
            Self::Other(string) => string.clone(),
        }
    }
}

impl Display for RouterId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_string())
    }
}

/// Represents a router in the protocol-agnostic network graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
