use std::{collections::HashMap, net::Ipv4Addr};

use crate::network::router::{Router, RouterId};
use ipnetwork::IpNetwork;
use ospf_parser::OspfLinkStateAdvertisement;
use uuid::Uuid;

/// Represents a node in the protocol-agnostic network graph. Multiple access networks and aggregates are represented by the Network variant.
#[derive(Debug, Clone)]
pub struct Node {
    pub info: NodeInfo,
    pub label: Option<String>,
    pub source_id: Option<RouterId>,
    pub id: Uuid,
}

impl Node {
    pub fn new(info: NodeInfo, label: Option<String>) -> Self {
        let uuid = match &info {
            NodeInfo::Router(router) => router.id.to_uuidv5(),
            NodeInfo::Network(network) => Uuid::new_v5(
                &Uuid::NAMESPACE_OID,
                network.ip_address.to_string().as_bytes(),
            ),
        };
        Self {
            info,
            label,
            source_id: None,
            id: uuid,
        }
    }

    /// Inter-area if derived from a Type 3 Summary LSA (or later from Type 4 when you map it).
    pub fn is_inter_area(&self) -> bool {
        match &self.info {
            NodeInfo::Network(net) => {
                if let Some(ProtocolData::Ospf(data)) = &net.protocol_data {
                    matches!(
                        *data.advertisement,
                        OspfLinkStateAdvertisement::SummaryLinkIpNetwork(_)
                    )
                } else {
                    false
                }
            }
            NodeInfo::Router(_r) => {
                // Optional future logic: if router is ABR (multiple areas or has summary LSAs)
                false
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum NodeInfo {
    Router(Router),
    Network(Network),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Network {
    pub ip_address: IpNetwork,
    pub protocol_data: Option<ProtocolData>,
    pub attached_routers: Vec<RouterId>,
}

#[derive(Debug, Clone)]
pub enum OspfPayload {
    Router(OspfRouterPayload),
    Network(OspfNetworkPayload),
    SummaryNetwork(OspfSummaryNetPayload),
}

#[derive(Debug, Clone)]
pub struct OspfRouterPayload {
    pub is_abr: bool,
    pub is_asbr: bool,
    pub is_virtual_link_endpoint: bool,
    pub is_nssa_capable: bool,
    pub p2p_link_count: usize,
    pub transit_link_count: usize,
    pub stub_link_count: usize,
    pub link_metrics: HashMap<Ipv4Addr, u16>,
    pub per_area_facets: Vec<PerAreaRouterFacet>,
    pub virtual_links: Vec<OspfVirtualLink>,
}

#[derive(Debug, Clone)]
pub struct OspfVirtualLink {
    pub peer_router_id: crate::network::router::RouterId,
    pub transit_area_id: std::net::Ipv4Addr,
}

#[derive(Debug, Clone)]
pub struct PerAreaRouterFacet {
    pub area_id: Ipv4Addr,
    pub p2p_link_count: usize,
    pub transit_link_count: usize,
    pub stub_link_count: usize
}

impl OspfRouterPayload {
    pub fn to_str_tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        if self.is_abr {
            tags.push("ABR".to_string());
        }
        if self.is_asbr {
            tags.push("ASBR".to_string());
        }
        if self.is_nssa_capable {
            tags.push("NSSA capable".to_string());
        }
        tags
    }
}

#[derive(Debug, Clone)]
pub struct OspfNetworkPayload {
    pub designated_router_id: Option<RouterId>,
    pub summaries: Vec<OspfSummaryNetPayload>,
    pub externals: Vec<OspfExternalNetPayload>,
}

#[derive(Debug, Clone)]
pub struct OspfExternalNetPayload {
    pub origin_asbr: RouterId,
    pub metric: u32,
    pub route_tag: Option<u32>,
    pub forwarding_address: Option<Ipv4Addr>,
}

#[derive(Debug, Clone)]
pub struct OspfSummaryNetPayload {
    pub metric: u32,
    pub origin_abr: RouterId,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct OspfData {
    pub area_id: Ipv4Addr,
    pub advertisement: std::sync::Arc<OspfLinkStateAdvertisement>,
    pub link_state_id: Ipv4Addr,
    pub advertising_router: Ipv4Addr,
    pub checksum: Option<u16>,
    pub payload: OspfPayload,
}

#[derive(Debug, Clone)]
pub struct OspfRouterData {
    pub is_abr: bool,
    pub is_asbr: bool,
    pub is_nssa_capable: bool,
    
}

#[derive(Debug, Clone)]
pub struct IsIsData {
    // TODO
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ProtocolData {
    Ospf(OspfData),
    IsIs(IsIsData),
    Other(String),
}
