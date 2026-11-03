use std::collections::HashMap;

use crate::{adapters::{RoutingMessage, RoutingProtocol, VendorAdapter}, data_acquisition::{TransportPayload, TransportProtocol}};
use anyhow::anyhow;
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
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaTlvWire {
    #[serde(rename = "type")]
    kind: String,
    area_adresses: Option<NokiaAreaAddress>,
    authentication: Option<NokiaAuthentication>,
    extended_ipv4_reachability: Option<NokiaExtV4Reachability>,
    extended_is_reachability: Option<ExtendedIsReachability>,
    hostname: Option<NokiaHostname>,
    

}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaAreaAddress {
    address: String
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
struct ExtendedIsReachability {
    neighbors: Vec<NokiaNeighbor>
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaNeighbor {
    system_id: String,
    instances: Vec<NokiaNeighborInstance>
}

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