use std::{collections::HashMap, net::Ipv4Addr};

use crate::network::router::{Router, RouterId};
use ipnetwork::IpNetwork;
use nom_derive::Parse;
use ospf_parser::OspfLinkStateAdvertisement;
use serde::{Deserialize, Serialize, ser::SerializeStruct};
use uuid::Uuid;

/// Represents a node in the protocol-agnostic network graph. Multiple access networks and aggregates are represented by the Network variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeInfo {
    Router(Router),
    Network(Network),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Network {
    pub ip_address: IpNetwork,
    pub protocol_data: Option<ProtocolData>,
    pub attached_routers: Vec<RouterId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OspfPayload {
    Router(OspfRouterPayload),
    Network(OspfNetworkPayload),
    SummaryNetwork(OspfSummaryNetPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OspfVirtualLink {
    pub peer_router_id: crate::network::router::RouterId,
    pub transit_area_id: std::net::Ipv4Addr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OspfNetworkPayload {
    pub designated_router_id: Option<RouterId>,
    pub summaries: Vec<OspfSummaryNetPayload>,
    pub externals: Vec<OspfExternalNetPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OspfExternalNetPayload {
    pub origin_asbr: RouterId,
    pub metric: u32,
    pub route_tag: Option<u32>,
    pub forwarding_address: Option<Ipv4Addr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub raw_lsa_bytes: std::sync::Arc<Vec<u8>>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SerializableOspfLsaType {
    RouterLinks,
    NetworkLinks,
    SummaryLinkIpNetwork,
    SummaryLinkAsbr,
    ASExternalLink,
    NSSAASExternal,
    OpaqueLinkLocalScope,
    OpaqueAreaLocalScope,
    OpaqueASWideScope
}

impl From<&OspfLinkStateAdvertisement> for SerializableOspfLsaType {
    fn from(value: &OspfLinkStateAdvertisement) -> Self {
        match value {
            OspfLinkStateAdvertisement::RouterLinks(_) => SerializableOspfLsaType::RouterLinks,
            OspfLinkStateAdvertisement::NetworkLinks(_) => SerializableOspfLsaType::NetworkLinks,
            OspfLinkStateAdvertisement::SummaryLinkIpNetwork(_) => SerializableOspfLsaType::SummaryLinkIpNetwork,
            OspfLinkStateAdvertisement::SummaryLinkAsbr(_) => SerializableOspfLsaType::SummaryLinkAsbr,
            OspfLinkStateAdvertisement::ASExternalLink(_) => SerializableOspfLsaType::ASExternalLink,
            OspfLinkStateAdvertisement::NSSAASExternal(_) => SerializableOspfLsaType::NSSAASExternal,
            OspfLinkStateAdvertisement::OpaqueLinkLocalScope(_) => SerializableOspfLsaType::OpaqueLinkLocalScope,
            OspfLinkStateAdvertisement::OpaqueAreaLocalScope(_) => SerializableOspfLsaType::OpaqueAreaLocalScope,
            OspfLinkStateAdvertisement::OpaqueASWideScope(_) => SerializableOspfLsaType::OpaqueASWideScope,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OspfDataWire {
    pub version: u32, // Increment this after each change, currently 1
    pub area_id: Ipv4Addr,
    pub link_state_id: Ipv4Addr,
    pub advertising_router: Ipv4Addr,
    pub checksum: Option<u16>,
    pub payload: OspfPayload,
    pub lsa_kind: SerializableOspfLsaType,
    pub lsa_hex: String
}

impl Serialize for OspfData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
            let lsa_type = SerializableOspfLsaType::from(self.advertisement.as_ref());
            let lsa_hex = hex::encode(self.raw_lsa_bytes.as_ref());
            
            let mut st = serializer.serialize_struct("OspfData", 8)?;
            st.serialize_field("version", &1u32)?;
            st.serialize_field("area_id", &self.area_id)?;
            st.serialize_field("link_state_id", &self.link_state_id)?;
            st.serialize_field("advertising_router", &self.advertising_router)?;
            st.serialize_field("checksum", &self.checksum)?;
            st.serialize_field("payload", &self.payload)?;
            st.serialize_field("lsa_kind", &lsa_type)?;
            st.serialize_field("lsa_hex", &lsa_hex)?;
            st.end()
    }
}

impl<'de> Deserialize<'de> for OspfData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de> {
            let wire = OspfDataWire::deserialize(deserializer)?;
            let raw = hex::decode(&wire.lsa_hex).map_err(serde::de::Error::custom)?;
            let parsed = ospf_parser::OspfLinkStateAdvertisement::parse(&raw)
                .map_err(|_| serde::de::Error::custom("failed to parse LSA bytes"))?
                .1;
            Ok(OspfData { 
                area_id: wire.area_id, 
                advertisement: std::sync::Arc::new(parsed), 
                link_state_id: wire.link_state_id, 
                advertising_router: wire.advertising_router, 
                checksum: wire.checksum, 
                payload: wire.payload, 
                raw_lsa_bytes: std::sync::Arc::new(raw) 
            })
    }
}



#[derive(Debug, Clone)]
pub struct OspfRouterData {
    pub is_abr: bool,
    pub is_asbr: bool,
    pub is_nssa_capable: bool,
    
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsIsData {
    // TODO
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum ProtocolData {
    Ospf(OspfData),
    IsIs(IsIsData),
    Other(String),
}

mod tests {
    use super::*;
    
    #[test]
    fn test_node_deserialization() {
        // Serialized R1
        let json = include_str!("../../test_data/test_node_deserialization.json"); 
        let node: Node = serde_json::from_str(json).expect("Failed to deserialize node");
        
        // Basic sanity checks against the fixture
        match &node.info {
            NodeInfo::Router(r) => {
                // RouterId string helper is used elsewhere (e.g., store), so this should work
                assert_eq!(r.id.as_string(), "172.21.0.1");
                assert!(matches!(r.protocol_data, Some(ProtocolData::Ospf(_))));
            }
            _ => panic!("expected Router node"),
        }
        assert_eq!(
            node.source_id.as_ref().map(|s| s.as_string()),
            Some("172.21.0.1".into())
        );
        assert_eq!(node.id.to_string(), "95dff25a-9c61-5d84-b2d8-15eacaa3fd06")
    }
}
