use std::collections::HashMap;

use crate::{adapters::{RoutingMessage, RoutingProtocol, VendorAdapter}, data_acquisition::{TransportPayload, TransportProtocol}};
use anyhow::anyhow;
use egui::Area;
use ipnetwork::IpNetwork;
use serde::Deserialize;

pub struct NokiaGnmiIsisAdapter;

impl NokiaGnmiIsisAdapter {
    fn lsp_json_to_lsp(lsp_json: serde_json::Value) -> Result<Vec<crate::adapters::packets::isis::Lsp>, anyhow::Error> {
        let mut lsps = Vec::new();
        todo!("Implement parsing Nokia's LSP JSON to LSPs.");
        Ok(lsps)
    }
}

impl VendorAdapter for NokiaGnmiIsisAdapter {
    fn vendor_name(&self) -> &'static str {
        "Nokia"
    }

    fn routing_protocol(&self) -> crate::adapters::RoutingProtocol {
        RoutingProtocol::IsIs
    }

    fn transport_protocol(&self) -> crate::data_acquisition::TransportProtocol {
        TransportProtocol::Grpc
    }

    fn subscribtion_paths(&self) -> Vec<String> {
        vec![
            "/network-instance[name=*]/protocols/isis/instance[name=*]/level[level-number=*]/link-state-database".to_string()
        ]
    }

    fn parse_transport_payload(&self, payload: crate::data_acquisition::TransportPayload) -> Result<Vec<crate::adapters::RoutingMessage>, anyhow::Error> {
        let payload = match payload {
            TransportPayload::Grpc(p) => p,
            _ => return Err(anyhow!("Unsupported transport payload for Nokia Gnmi ISIS adapter"))
        };

        let lsp_json: serde_json::Value = serde_json::from_str(&payload)?;
        let lsps = Self::lsp_json_to_lsp(lsp_json)?;

        Ok(
            lsps.into_iter()
            .map(|lsp| RoutingMessage::IsIs(lsp))
            .collect()
        )
    }
}

struct NokiaLsp {
    pub lsp_id: String,
    pub checksum: u16,
    pub flags: u8,
    pub id_length: u8,
    pub is_type: u8,
    pub maximum_area_addresses: u8,
    pub pdu_length: u16,
    pub pdu_type: u8,
    pub remaining_lifetime: u16,
    pub sequence_number: u32,
    pub tlvs: Vec<NokiaTlv>,
    pub version: u8,
    pub version2: u8
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaTlv {
    #[serde(rename = "type")]
    kind: String,
    area_adresses: Option<NokiaAreaAddress>,
    authentication: Option<NokiaAuthentication>,
    extended_ipv4_reachability: Option<NokiaExtV4Reachability>,
    extended_is_reachability: Option<NokiaExtendedIsReachability>,
    hostname: Option<NokiaHostname>,
    is_reachability: Option<NokiaIsReachability>,
    router_capabilities: Option<NokiaRouterCapabilities>
}

/*

- [x] /// TLV #1: Area Addresses — identifies the IS-IS areas this router belongs to.
AreaAddresses(AreaAddressesTlv),
- [x] /// TLV #2: IS Reachability — describes neighbors and link metrics.
IsReachability(IsReachabilityTlv),
- [x]/// TLV #22: Extended IS Reachability — describes neighbors and link metrics.
ExtendedReachability(IsExtendedReachabilityTlv),
- [skip] /// TLV #128: IP Reachability — IPv4 prefixes directly connected or redistributed by this router.
IpReachability(IpReachabilityTlv),
- [x] /// TLV #135: 
ExtendedIpReachability(ExtendedIpReachabilityTlv),
- [x] /// TLV #137: Hostname — the router's human-readable name.
Hostname(String),
- [x] /// TLV #242: Router Capability — router's capabilities (e.g., TE Router ID, flags).
RouterCapability(RouterCapabilityTlv),

*/

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaAreaAddress {
    address: Vec<String>
}

impl From<NokiaAreaAddress> for crate::adapters::packets::isis::AreaAddressesTlv {
    fn from(value: NokiaAreaAddress) -> Self {
        use crate::adapters::packets::isis::{AreaAddress};
        let string_to_area_address = |s: String| -> Option<AreaAddress> {
            // Expecting area addresses to be in the format of "49.0001" or similar.
            let normalized = s.replace(".", "").to_lowercase();
            let bytes = hex::decode(normalized).ok()?;

            if bytes.is_empty() || bytes.len() > 13 {
                return None;
            }
            Some(AreaAddress { raw_address: bytes })
        };

        let addresses = value.address.into_iter()
        .filter_map(string_to_area_address)
        .collect::<Vec<AreaAddress>>();
        
        Self {
            addresses
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaAuthentication {
    authentication_key: String,
    crypto_type: String
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaExtV4Reachability {
    prefixes: Vec<NokiaPrefix>,
}

impl From<NokiaExtV4Reachability> for crate::adapters::packets::isis::ExtendedIpReachabilityTlv {
    fn from(value: NokiaExtV4Reachability) -> Self {
        let neighbors = value.prefixes.into_iter()
        .map(|prefix| prefix.into())
        .collect();

        Self {
            neighbors
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaPrefix {
    prefix: String,
    metric: u32,
    s_bit: bool,
    up_down: bool,

    #[serde(default)]
    subtlvs: Option<Vec<NokiaSubTlv>>
}

impl From<NokiaPrefix> for crate::adapters::packets::isis::ExtendedIpReachabilityNeighbor {
    fn from(value: NokiaPrefix) -> Self {
        let prefix: IpNetwork = value.prefix.parse().expect("Failed to parse prefix into IpNetwork");
        Self {
            prefix,
            metric: value.metric,
            up_down: value.up_down,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaSubTlv {
    #[serde(rename = "type")]
    kind: String,
    #[serde(flatten)]
    payload: HashMap<String, serde_json::Value>
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaExtendedIsReachability {
    neighbors: Vec<NokiaIsNeighbor>
}

impl From<NokiaExtendedIsReachability> for crate::adapters::packets::isis::IsExtendedReachabilityTlv {
    fn from(value: NokiaExtendedIsReachability) -> Self {
        todo!()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaIsNeighbor {
    system_id: String,
    instances: Vec<NokiaNeighborInstance>
}

impl From<NokiaIsNeighbor> for crate::adapters::packets::isis::ExtendedIsNeighbor {
    fn from(value: NokiaIsNeighbor) -> Self {
        todo!()
    }
}

/*
"neighbors": [
    {
        "system-id": "0000.0000.0001",
        "instances": [
            {
                "id": 0,
                "metric": 10,
                "subtlvs": [
                    {
                        "type": "admin-group",
                        "admin-group": 10
                    }
                ]
            }
        ]
    }
]
*/

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaNeighborInstance {
    id: u64,
    metric: u32,
    
    #[serde(default)]
    subtlvs: Option<Vec<NokiaSubTlv>>
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaHostname {
    hostname: String
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaIsReachability {
    neighbors: Vec<NokiaSimpleNeighbor>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaSimpleNeighbor {
    system_id: String,
    default_metric: NokiaSimpleMetric,
    delay_metric: Option<NokiaSimpleMetric>,
    error_metric: Option<NokiaSimpleMetric>,
    expense_metric: Option<NokiaSimpleMetric>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaSimpleMetric {
    flags: u8,
    metric: u8
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaRouterCapabilities {
    instance_number: u32,
    flags: u8,
    router_id: String,
    subtlvs: Option<Vec<NokiaSubTlv>>
}
