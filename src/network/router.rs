use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RouterId {
    Ipv4(Ipv4Addr),
    Ipv6(Ipv6Addr),
    IsIs([u8; 6]),
    Other(String)
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
}

/// Represents a router in the protocol-agnostic network graph.
pub struct Router {
    pub id: RouterId,
    pub interfaces: Vec<IpAddr>,
    pub protocol_data: Option<ProtocolData>,
}

pub enum ProtocolData {
    Ospf(OspfData),
    IsIs(IsIsData),
    Other(String)
}

pub enum OspfRouterType {
    Internal,
    Designated(Ipv4Addr /* Network in which the router is designated */),
    AreaBorder,
    External,
    Virtual,
    Stub,
    NotDefined
}

pub struct OspfData {
    router_type: OspfRouterType,
    area_id: Ipv4Addr
}

pub struct IsIsData {
    // TODO
}