use std::{
    fmt::Display,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use serde::{Deserialize, Serialize, de::Error};
use uuid::{Uuid, serde::compact::deserialize};

use crate::network::node::ProtocolData;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum RouterId {
    Ipv4(Ipv4Addr),
    Ipv6(Ipv6Addr),
    IsIs([u8; 6]),
    Other(String),
}

impl Serialize for RouterId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
            serializer.serialize_str(&self.as_string())
    }
}

impl<'de> Deserialize<'de> for RouterId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de> {
            let s: &str = <&str>::deserialize(deserializer)?;
            if let Some(isis_id) = s.strip_prefix("isis:") {
                let bytes = hex::decode(isis_id).map_err(D::Error::custom)?;
                if bytes.len() != 6 {
                    return Err(D::Error::custom("Invalid IS-IS ID length"));
                }
                return Ok(RouterId::IsIs(bytes.try_into().unwrap()))
            };
            if let Ok(ipv4) = s.parse::<Ipv4Addr>() {
                return Ok(RouterId::Ipv4(ipv4));
            }
            if let Ok(ipv6) = s.parse::<Ipv6Addr>() {
                return Ok(RouterId::Ipv6(ipv6));
            }
            Ok(RouterId::Other(s.to_string()))
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
            Self::IsIs(id) => format!("isis:{}", hex::encode(id)),
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
