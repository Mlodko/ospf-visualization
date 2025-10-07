use ipnetwork::IpNetwork;
use uuid::Uuid;
use crate::network::router::Router;

/// Represents a node in the protocol-agnostic network graph. Multiple access networks and aggregates are represented by the Network variant.
pub struct Node {
    pub info: NodeInfo,
    pub label: Option<String>,
    pub id: Uuid
}

impl Node {
    pub fn new(info: NodeInfo, label: Option<String>) -> Self {
        let uuid = match &info {
            NodeInfo::Router(router) => Uuid::new_v5(&Uuid::NAMESPACE_OID, &router.id.as_bytes()),
            NodeInfo::Network(network) => Uuid::new_v5(&Uuid::NAMESPACE_OID, network.ip_address.to_string().as_bytes()),
        };
        Self {
            info,
            label,
            id: uuid
        }
    }
}

pub enum NodeInfo {
    Router(Router),
    Network(Network)
}

pub struct Network {
    ip_address: IpNetwork
}

