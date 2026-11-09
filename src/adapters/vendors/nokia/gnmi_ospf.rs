use crate::{
    adapters::{packets::ospf::lsa::Lsa, RoutingMessage, VendorAdapter},
    data_acquisition::TransportPayload,
};
use anyhow::anyhow;
use std::net::Ipv4Addr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NokiaGnmiOspfAdapter;

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
            "/network-instance[name=*]/protocols/ospf/instance[name=*]/lsdb/lsa-types/lsa-type[type=*]/lsas/lsa[advertising-router=*][link-state-id=*]".to_string(),
            "/network-instance[name=*]/protocols/ospf/instance[name=*]/area[area-id=*]/lsdb/lsa-types/lsa-type[type=*]/lsas/lsa[advertising-router=*][link-state-id=*]".to_string()
        ]
    }

    fn parse_transport_payload(
        &self,
        payload: crate::data_acquisition::TransportPayload,
    ) -> Result<Vec<crate::adapters::RoutingMessage>, anyhow::Error> {
        match payload {
            TransportPayload::GnmiGet(notifications) => {
                let mut messages = Vec::new();

                for notification in notifications {
                    for update in notification.updates() {
                        let value = update.value();

                        let json: serde_json::Value = value.try_into()?;
                        let advertisements = extract_advertisements(&json)?;
                        for advertisement in advertisements {
                            let (_, lsa) = Lsa::parse(&advertisement.bytes).map_err(|error| {
                                anyhow!("Failed to parse OSPF LSA advertisement: {error:?}")
                            })?;
                            messages.push(RoutingMessage::Ospf {
                                area_id: advertisement.area_id,
                                lsa,
                            });
                        }
                    }
                }

                Ok(messages)
            }
            _ => Err(anyhow!(
                "Unsupported transport payload for Nokia gNMI OSPF adapter"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExtractedAdvertisement {
    area_id: Option<Ipv4Addr>,
    bytes: Vec<u8>,
}

fn extract_advertisements(
    value: &serde_json::Value,
) -> Result<Vec<ExtractedAdvertisement>, anyhow::Error> {
    let mut advertisements = Vec::new();
    visit(value, None, &mut advertisements)?;
    Ok(advertisements)
}

fn visit(
    value: &serde_json::Value,
    area_id: Option<Ipv4Addr>,
    output: &mut Vec<ExtractedAdvertisement>,
) -> Result<(), anyhow::Error> {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(serde_json::Value::Array(areas)) = object.get("area") {
                for area in areas {
                    let area_id = area
                        .get("area-id")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| anyhow!("Missing OSPF area-id"))?
                        .parse()
                        .map_err(|error| anyhow!("Invalid OSPF area-id: {error}"))?;
                    visit(area, Some(area_id), output)?;
                }
            }

            if let Some(serde_json::Value::Array(lsas)) = object.get("lsa") {
                for (index, lsa) in lsas.iter().enumerate() {
                    let advertisement = lsa
                        .get("advertisement")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| anyhow!("Missing advertisement at index {index}"))?;

                    output.push(ExtractedAdvertisement {
                        area_id,
                        bytes: hex::decode(advertisement)?,
                    });
                }
            }

            for (key, child) in object {
                if key != "lsa" && key != "area" {
                    visit(child, area_id, output)?;
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                visit(value, area_id, output)?;
            }
        }
        _ => {}
    }

    Ok(())
}

mod tests {
    use crate::{
        adapters::{vendors::nokia::gnmi_ospf::extract_advertisements, VendorAdapter},
        data_acquisition::{transport::grpc::prelude::*, TransportPayload},
    };

    #[test]
    fn extracts_area_context_from_lsdb_json() {
        let documents = serde_json::Deserializer::from_str(include_str!("../../../../lsas.json"))
            .into_iter::<serde_json::Value>()
            .take(2)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let advertisements = documents
            .iter()
            .map(extract_advertisements)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        assert!(!advertisements.is_empty());
        assert!(advertisements
            .iter()
            .any(|advertisement| { advertisement.area_id.is_none() }));
        assert!(advertisements
            .iter()
            .any(|advertisement| { advertisement.area_id == Some("0.0.0.0".parse().unwrap()) }));
    }

    #[ignore = "requires a live SR Linux lab"]
    #[test]
    fn test_nokia_gnmi_ospf_adapter() {
        let adapter = super::NokiaGnmiOspfAdapter;

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut client = GnmiClient::new("http://200.1.0.1:57401", "admin", "NokiaSrl1!")
                .await
                .unwrap();

            let paths = adapter.subscribtion_paths();
            let paths_str: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
            let response = client.get(&paths_str).await.unwrap();
            let notifications: Vec<JsonIetfNotification> = (&response).try_into().unwrap();
            let transport_payload = TransportPayload::GnmiGet(notifications);
            let routing_messages = adapter.parse_transport_payload(transport_payload).unwrap();
            for message in routing_messages {
                println!("{:?}", message);
            }
        });
    }
}
