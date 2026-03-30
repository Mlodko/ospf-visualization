use std::{collections::HashMap, net::Ipv4Addr, sync::Arc};

use crate::{
    network::router::{Router, RouterId},
    parsers::isis_parser::core_lsp::{IsLevel, Lsp, LspId, NetAddress, Tlv},
};
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
            NodeInfo::Network(network) => {
                let uuid_input = network.ip_address.to_string();
                // match &network.protocol_data {
                //     Some(ProtocolData::Ospf(data)) => {
                //         format!("{}@{}", network.ip_address, data.area_id)
                //     }
                //     _ => network.ip_address.to_string(),
                // };
                Uuid::new_v5(&Uuid::NAMESPACE_OID, uuid_input.as_bytes())
            }
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
                        data.base_advertisement.advertisement(),
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

impl NodeInfo {
    pub fn router(&self) -> Option<&Router> {
        match self {
            NodeInfo::Router(r) => Some(r),
            _ => None,
        }
    }
    
    pub fn router_mut(&mut self) -> Option<&mut Router> {
        match self {
            NodeInfo::Router(r) => Some(r),
            _ => None,
        }
    }
    
    pub fn network(&self) -> Option<&Network> {
        match self {
            NodeInfo::Network(n) => Some(n),
            _ => None,
        }
    }
    
    pub fn network_mut(&mut self) -> Option<&mut Network> {
        match self {
            NodeInfo::Network(n) => Some(n),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Network {
    pub ip_address: IpNetwork,
    pub protocol_data: Option<ProtocolData>,
    pub attached_routers: Vec<RouterId>,
}

impl Network {
    pub fn protocol_data(&self) -> Option<&ProtocolData> {
        self.protocol_data.as_ref()
    }
    
    pub fn protocol_data_mut(&mut self) -> Option<&mut ProtocolData> {
        self.protocol_data.as_mut()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OspfPayload {
    Router(OspfRouterPayload),
    Network(OspfNetworkPayload),
    SummaryNetwork(OspfSummaryNetPayload),
}

impl OspfPayload {
    pub fn router(&self) -> Option<&OspfRouterPayload> {
        match self {
            OspfPayload::Router(r) => Some(r),
            _ => None,
        }
    }
    
    pub fn router_mut(&mut self) -> Option<&mut OspfRouterPayload> {
        match self {
            OspfPayload::Router(r) => Some(r),
            _ => None,
        }
    }

    pub fn network(&self) -> Option<&OspfNetworkPayload> {
        match self {
            OspfPayload::Network(n) => Some(n),
            _ => None,
        }
    }
    
    pub fn network_mut(&mut self) -> Option<&mut OspfNetworkPayload> {
        match self {
            OspfPayload::Network(n) => Some(n),
            _ => None,
        }
    }

    pub fn summary_network(&self) -> Option<&OspfSummaryNetPayload> {
        match self {
            OspfPayload::SummaryNetwork(n) => Some(n),
            _ => None,
        }
    }
    
    pub fn summary_network_mut(&mut self) -> Option<&mut OspfSummaryNetPayload> {
        match self {
            OspfPayload::SummaryNetwork(n) => Some(n),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OspfSummaryAsbrPayload {
    
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
    pub virtual_link_count: usize,
    pub link_metrics: HashMap<Ipv4Addr, u16>,
    pub per_area_facets: Vec<PerAreaRouterFacet>,
    pub virtual_links: Vec<OspfVirtualLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct OspfVirtualLink {
    pub peer_router_id: crate::network::router::RouterId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerAreaRouterFacet {
    pub area_id: Ipv4Addr,
    pub p2p_link_count: usize,
    pub transit_link_count: usize,
    pub stub_link_count: usize,
    pub virtual_link_count: usize,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OspfExternalNetPayload {
    pub origin_asbr: RouterId,
    pub metric: u32,
    pub route_tag: Option<u32>,
    pub forwarding_address: Option<Ipv4Addr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OspfSummaryNetPayload {
    pub metric: u32,
    pub origin_abr: RouterId,
}

#[derive(Debug, Clone)]
pub struct OspfAdvertisement {
    advertisement: std::sync::Arc<OspfLinkStateAdvertisement>,
    raw: std::sync::Arc<Vec<u8>>,
}

impl OspfAdvertisement {
    pub fn new(advertisement: std::sync::Arc<OspfLinkStateAdvertisement>, raw: std::sync::Arc<Vec<u8>>) -> Self {
        Self {
            advertisement,
            raw,
        }
    }
    
    pub fn advertisement(&self) -> &OspfLinkStateAdvertisement {
        &self.advertisement
    }
    
    pub fn advertisement_arc(&self) -> Arc<OspfLinkStateAdvertisement> {
        self.advertisement.clone()
    }
    
    pub fn raw(&self) -> &Vec<u8> {
        &self.raw
    }
    
    pub fn raw_arc(&self) -> Arc<Vec<u8>> {
        self.raw.clone()
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct OspfData {
    pub area_id: Ipv4Addr,
    pub base_advertisement: OspfAdvertisement,
    pub merged_advertisements: Vec<OspfAdvertisement>,
    pub ls_age: u16,
    pub ls_seq_no: u32,
    pub link_state_id: Ipv4Addr,
    pub advertising_router: Ipv4Addr,
    pub checksum: Option<u16>,
    pub payload: OspfPayload,
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
    OpaqueASWideScope,
}

impl From<&OspfLinkStateAdvertisement> for SerializableOspfLsaType {
    fn from(value: &OspfLinkStateAdvertisement) -> Self {
        match value {
            OspfLinkStateAdvertisement::RouterLinks(_) => SerializableOspfLsaType::RouterLinks,
            OspfLinkStateAdvertisement::NetworkLinks(_) => SerializableOspfLsaType::NetworkLinks,
            OspfLinkStateAdvertisement::SummaryLinkIpNetwork(_) => {
                SerializableOspfLsaType::SummaryLinkIpNetwork
            }
            OspfLinkStateAdvertisement::SummaryLinkAsbr(_) => {
                SerializableOspfLsaType::SummaryLinkAsbr
            }
            OspfLinkStateAdvertisement::ASExternalLink(_) => {
                SerializableOspfLsaType::ASExternalLink
            }
            OspfLinkStateAdvertisement::NSSAASExternal(_) => {
                SerializableOspfLsaType::NSSAASExternal
            }
            OspfLinkStateAdvertisement::OpaqueLinkLocalScope(_) => {
                SerializableOspfLsaType::OpaqueLinkLocalScope
            }
            OspfLinkStateAdvertisement::OpaqueAreaLocalScope(_) => {
                SerializableOspfLsaType::OpaqueAreaLocalScope
            }
            OspfLinkStateAdvertisement::OpaqueASWideScope(_) => {
                SerializableOspfLsaType::OpaqueASWideScope
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OspfDataWire {
    pub ls_age: u16,
    pub ls_seq_no: u32,
    #[allow(dead_code)]
    pub version: u32, // Increment in Serialize impl this after each change, currently 2
    pub area_id: Ipv4Addr,
    pub link_state_id: Ipv4Addr,
    pub advertising_router: Ipv4Addr,
    pub checksum: Option<u16>,
    pub payload: OspfPayload,
    #[allow(dead_code)]
    pub lsa_kind: SerializableOspfLsaType,
    pub lsa_hex: String,
}

impl Serialize for OspfData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let lsa_type = SerializableOspfLsaType::from(self.base_advertisement.advertisement());
        let lsa_hex = hex::encode(self.base_advertisement.raw());

        let mut st = serializer.serialize_struct("OspfData", 8)?;
        st.serialize_field("ls_age", &self.ls_age)?;
        st.serialize_field("ls_seq_no", &self.ls_seq_no)?;
        st.serialize_field("version", &4u32)?; // CHANGE HERE
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
        D: serde::Deserializer<'de>,
    {
        let wire = OspfDataWire::deserialize(deserializer)?;
        let raw = hex::decode(&wire.lsa_hex).map_err(serde::de::Error::custom)?;
        let parsed = ospf_parser::OspfLinkStateAdvertisement::parse(&raw)
            .map_err(|_| serde::de::Error::custom("failed to parse LSA bytes"))?
            .1;
        let advertisement = OspfAdvertisement::new(Arc::new(parsed), Arc::new(raw));
        Ok(OspfData {
            ls_seq_no: wire.ls_seq_no,
            ls_age: wire.ls_age,
            area_id: wire.area_id,
            base_advertisement: advertisement,
            merged_advertisements: vec![],
            link_state_id: wire.link_state_id,
            advertising_router: wire.advertising_router,
            checksum: wire.checksum,
            payload: wire.payload,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsIsData {
    pub is_level: IsLevel,
    pub lsp_id: LspId,
    pub net_address: Option<NetAddress>,
    pub tlvs: Vec<Tlv>,
    pub owned: Option<bool>,
    pub lsp: Lsp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum ProtocolData {
    Ospf(OspfData),
    IsIs(IsIsData),
    Other(String),
}

impl ProtocolData {
    pub fn ospf(&self) -> Option<&OspfData> {
        match self {
            ProtocolData::Ospf(data) => Some(data),
            _ => None,
        }
    }
    
    pub fn ospf_mut(&mut self) -> Option<&mut OspfData> {
        match self {
            ProtocolData::Ospf(data) => Some(data),
            _ => None,
        }
    }
    
    pub fn isis(&self) -> Option<&IsIsData> {
        match self {
            ProtocolData::IsIs(data) => Some(data),
            _ => None,
        }
    }
    
    pub fn other(&self) -> Option<&String> {
        match self {
            ProtocolData::Other(data) => Some(data),
            _ => None,
        }
    }
}

mod tests {
    #[allow(unused)]
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
