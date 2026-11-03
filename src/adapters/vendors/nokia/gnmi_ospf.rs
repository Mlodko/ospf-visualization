use crate::{adapters::{RoutingMessage, VendorAdapter, packets::ospf::lsa::Lsa}, data_acquisition::TransportPayload};
use anyhow::anyhow;


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NokiaGnmiOspfAdapter;

impl NokiaGnmiOspfAdapter {
    fn lsdb_json_to_lsa(lsdb_json: serde_json::Value) -> Result<Vec<Lsa>, anyhow::Error> {
        let mut lsas = Vec::new();
        todo!("Implement parsing Nokia's lsdb JSON to LSAs.");
        Ok(lsas)
    }
}

impl VendorAdapter for NokiaGnmiOspfAdapter {
    fn vendor_name(&self) -> &'static str {
        "Nokia"
    }

    fn routing_protocol(&self) -> crate::adapters::RoutingProtocol {
        crate::adapters::RoutingProtocol::Ospf
    }

    fn transport_protocol(&self) -> crate::data_acquisition::TransportProtocol {
        crate::data_acquisition::TransportProtocol::Grpc
    }

    fn subscribtion_paths(&self) -> Vec<String> {
        vec![
            "/network-instance[name=*]/protocols/ospf/instance[name=*]/lsdb".to_string()
        ]
    }

    fn parse_transport_payload(&self, payload: crate::data_acquisition::TransportPayload) -> Result<Vec<crate::adapters::RoutingMessage>, anyhow::Error> {
        let payload = match payload {
            TransportPayload::Grpc(p) => p,
            _ => return Err(anyhow!("Unsupported transport payload for Nokia Gnmi OSPF adapter"))
        };
        let lsdb_json: serde_json::Value = serde_json::from_str(&payload)?;
        let lsas = Self::lsdb_json_to_lsa(lsdb_json)?;

        Ok(
            lsas.into_iter()
            .map(|lsa| RoutingMessage::Ospf(lsa))
            .collect()
        )
    }
}