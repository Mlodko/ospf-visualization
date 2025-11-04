use std::net::Ipv4Addr;

use ipnetwork::IpNetwork;
use ospf_parser::OspfLinkStateAdvertisement;
use uuid::Uuid;
use crate::network::router::{Router, RouterId};

/// Represents a node in the protocol-agnostic network graph. Multiple access networks and aggregates are represented by the Network variant.
#[derive(Debug, Clone)]
pub struct Node {
    pub info: NodeInfo,
    pub label: Option<String>,
    pub id: Uuid
}

impl Node {
    pub fn new(info: NodeInfo, label: Option<String>) -> Self {
        let uuid = match &info {
            NodeInfo::Router(router) => router.id.to_uuidv5(),
            NodeInfo::Network(network) => Uuid::new_v5(&Uuid::NAMESPACE_OID, network.ip_address.to_string().as_bytes()),
        };
        Self {
            info,
            label,
            id: uuid
        }
    }
}

#[derive(Debug, Clone)]
pub enum NodeInfo {
    Router(Router),
    Network(Network)
}

#[derive(Debug, Clone)]
pub struct Network {
    pub ip_address: IpNetwork,
    pub protocol_data: Option<ProtocolData>,
    pub attached_routers: Vec<RouterId>
}

#[derive(Debug, Clone)]
pub struct OspfData {
    pub area_id: Ipv4Addr,
    pub advertisement: std::sync::Arc<OspfLinkStateAdvertisement>,
}

#[derive(Debug, Clone)]
pub struct IsIsData {
    // TODO
}

#[derive(Debug, Clone)]
pub enum ProtocolData {
    Ospf(OspfData),
    IsIs(IsIsData),
    Other(String)
}