use std::{collections::HashMap, str::FromStr};

use crate::data_acquisition::transport::grpc::prelude::{
    JsonIetfNotification, JsonIetfSubscriptionEvent,
};
use crate::{
    adapters::{
        packets::isis::{self, Tlv},
        RoutingMessage, RoutingProtocol, VendorAdapter,
    },
    data_acquisition::{TransportPayload, TransportProtocol},
};
use anyhow::anyhow;
use ipnetwork::IpNetwork;
use serde::Deserialize;

pub struct NokiaGnmiIsisAdapter;

impl NokiaGnmiIsisAdapter {
    fn lsp_json_to_lsp(
        lsp_json: serde_json::Value,
    ) -> Result<Vec<crate::adapters::packets::isis::Lsp>, anyhow::Error> {
        let mut levels = Vec::new();
        collect_levels(&lsp_json, &mut levels)?;

        let mut lsps = Vec::new();
        for level in levels {
            let is_level = match level.level_number {
                1 => isis::IsLevel::Level1,
                2 => isis::IsLevel::Level2,
                level => return Err(anyhow!("Unsupported IS-IS level {level}")),
            };

            for lsp in level.link_state_database.lsp {
                lsps.push(lsp.to_isis_lsp(is_level.clone())?);
            }
        }

        Ok(lsps)
    }
}

fn collect_levels(
    value: &serde_json::Value,
    levels: &mut Vec<NokiaLevel>,
) -> Result<(), anyhow::Error> {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(serde_json::Value::Array(level_values)) = object.get("level") {
                for level in level_values {
                    levels.push(serde_json::from_value(level.clone())?);
                }
                return Ok(());
            }

            for child in object.values() {
                collect_levels(child, levels)?;
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_levels(value, levels)?;
            }
        }
        _ => {}
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaLevel {
    level_number: u8,
    link_state_database: NokiaLinkStateDatabase,
}

#[derive(Debug, Deserialize)]
struct NokiaLinkStateDatabase {
    lsp: Vec<NokiaLsp>,
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

    fn parse_transport_payload(
        &self,
        payload: crate::data_acquisition::TransportPayload,
    ) -> Result<Vec<crate::adapters::RoutingMessage>, anyhow::Error> {
        match payload {
            TransportPayload::GnmiGet(notifications) => notifications
                .into_iter()
                .map(parse_notification)
                .collect::<Result<Vec<_>, _>>()
                .map(|messages| messages.into_iter().flatten().collect()),
            TransportPayload::GnmiSub(JsonIetfSubscriptionEvent::Notification(notification)) => {
                parse_notification(notification)
            }
            TransportPayload::GnmiSub(JsonIetfSubscriptionEvent::Sync) => Ok(Vec::new()),
            TransportPayload::GnmiSub(JsonIetfSubscriptionEvent::EventError(error)) => {
                Err(anyhow!("gNMI subscription error: {error:?}"))
            }
            _ => Err(anyhow!(
                "Unsupported transport payload for Nokia gNMI IS-IS adapter"
            )),
        }
    }
}

fn parse_notification(
    notification: JsonIetfNotification,
) -> Result<Vec<RoutingMessage>, anyhow::Error> {
    let mut messages = Vec::new();

    for update in notification.updates() {
        let json: serde_json::Value = update.value().try_into()?;
        for lsp in NokiaGnmiIsisAdapter::lsp_json_to_lsp(json)? {
            messages.push(RoutingMessage::IsIs(lsp));
        }
    }

    Ok(messages)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaLsp {
    pub lsp_id: String,
    pub checksum: u16,
    #[serde(default)]
    pub flags: u8,
    pub id_length: u8,
    pub is_type: u8,
    pub maximum_area_addresses: u8,
    pub pdu_length: u16,
    pub pdu_type: String,
    pub remaining_lifetime: u16,
    pub sequence_number: u32,
    pub tlvs: NokiaTlvs,
    pub version: u8,
    #[serde(default)]
    pub version2: u8,
}

impl NokiaLsp {
    fn to_isis_lsp(self, is_level: isis::IsLevel) -> Result<isis::Lsp, anyhow::Error> {
        let lsp_id = isis::LspId::from_string(&self.lsp_id)
            .map_err(|e| anyhow!("Failed to parse LSP ID: {}", e))?;

        let system_id = lsp_id
            .system_id()
            .map_err(|e| anyhow!("Failed to extract system ID from LSP ID: {}", e))?;

        let sequence_number = Some(format!("{:08X}", self.sequence_number));
        let holdtime = Some(format!("{:04X}", self.remaining_lifetime));
        let area_address = None;
        let owned = None;
        let tlvs = self.tlvs.tlv.into_iter().try_fold(
            Vec::new(),
            |mut acc, tlv| -> Result<Vec<Tlv>, anyhow::Error> {
                let mut tlvs = tlv.to_isis_tlvs(lsp_id.get_pseudonode_id())?;
                acc.append(&mut tlvs);
                Ok(acc)
            },
        )?;
        Ok(isis::Lsp {
            lsp_id,
            system_id,
            is_level,
            sequence_number,
            holdtime,
            area_addr: area_address,
            owned,
            tlvs,
        })
    }
}

#[derive(Debug, Deserialize)]
struct NokiaTlvs {
    tlv: Vec<NokiaTlv>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaTlv {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "area-address")]
    area_addresses: Option<NokiaAreaAddress>,
    authentication: Option<NokiaAuthentication>,
    extended_ipv4_reachability: Option<NokiaExtV4Reachability>,
    extended_is_reachability: Option<NokiaExtendedIsReachability>,
    hostname: Option<NokiaHostname>,
    is_reachability: Option<NokiaIsReachability>,
    router_capabilities: Option<NokiaRouterCapabilities>,
}

impl NokiaTlv {
    fn to_isis_tlvs(self, pseudonode_id: u8) -> Result<Vec<isis::Tlv>, anyhow::Error> {
        let mut tlvs = Vec::new();

        if let Some(area_addr) = self.area_addresses {
            let tlv: isis::AreaAddressesTlv = area_addr.into();
            tlvs.push(isis::Tlv::AreaAddresses(tlv));
        }

        if let Some(ext_ip_reach) = self.extended_ipv4_reachability {
            let tlv: isis::ExtendedIpReachabilityTlv = ext_ip_reach.into();
            tlvs.push(isis::Tlv::ExtendedIpReachability(tlv));
        }

        if let Some(ext_is_reach) = self.extended_is_reachability {
            let tlv = ext_is_reach.to_ext_is_reachability(pseudonode_id)?;
            tlvs.push(isis::Tlv::ExtendedReachability(tlv));
        }

        if let Some(hostname) = self.hostname {
            let tlv = hostname.into();
            tlvs.push(tlv);
        }

        if let Some(is_reach) = self.is_reachability {
            let tlv: isis::IsReachabilityTlv = is_reach.into();
            tlvs.push(isis::Tlv::IsReachability(tlv));
        }

        if let Some(router_cap) = self.router_capabilities {
            let tlv: isis::RouterCapabilityTlv = router_cap.into();
            tlvs.push(isis::Tlv::RouterCapability(tlv));
        }

        Ok(tlvs)
    }
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
    address: Vec<String>,
}

impl From<NokiaAreaAddress> for crate::adapters::packets::isis::AreaAddressesTlv {
    fn from(value: NokiaAreaAddress) -> Self {
        use crate::adapters::packets::isis::AreaAddress;
        let string_to_area_address = |s: String| -> Option<AreaAddress> {
            // Expecting area addresses to be in the format of "49.0001" or similar.
            let normalized = s.replace(".", "").to_lowercase();
            let bytes = hex::decode(normalized).ok()?;

            if bytes.is_empty() || bytes.len() > 13 {
                return None;
            }
            Some(AreaAddress { raw_address: bytes })
        };

        let addresses = value
            .address
            .into_iter()
            .filter_map(string_to_area_address)
            .collect::<Vec<AreaAddress>>();

        Self { addresses }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaAuthentication {
    authentication_key: String,
    crypto_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaExtV4Reachability {
    prefixes: NokiaPrefixes,
}

#[derive(Debug, Deserialize)]
struct NokiaPrefixes {
    prefix: Vec<NokiaPrefix>,
}

impl From<NokiaExtV4Reachability> for crate::adapters::packets::isis::ExtendedIpReachabilityTlv {
    fn from(value: NokiaExtV4Reachability) -> Self {
        let neighbors = value
            .prefixes
            .prefix
            .into_iter()
            .map(|prefix| prefix.into())
            .collect();

        Self { neighbors }
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
    subtlvs: Option<NokiaSubTlvs>,
}

impl From<NokiaPrefix> for crate::adapters::packets::isis::ExtendedIpReachabilityNeighbor {
    fn from(value: NokiaPrefix) -> Self {
        let prefix: IpNetwork = value
            .prefix
            .parse()
            .expect("Failed to parse prefix into IpNetwork");
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
    payload: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct NokiaSubTlvs {
    subtlv: Vec<NokiaSubTlv>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaExtendedIsReachability {
    neighbors: NokiaNeighbors,
}

#[derive(Debug, Deserialize)]
struct NokiaNeighbors {
    neighbor: Vec<NokiaIsNeighbor>,
}

impl NokiaExtendedIsReachability {
    fn to_ext_is_reachability(
        &self,
        pseudonode_id: u8,
    ) -> Result<isis::IsExtendedReachabilityTlv, anyhow::Error> {
        let mut neighbors = Vec::with_capacity(self.neighbors.neighbor.len());
        for neighbor in &self.neighbors.neighbor {
            let ext_neighbor = neighbor.to_ext_is_neighbor(pseudonode_id)?;
            neighbors.push(ext_neighbor);
        }
        Ok(isis::IsExtendedReachabilityTlv { neighbors })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaIsNeighbor {
    system_id: String,
    instances: NokiaInstances,
}

#[derive(Debug, Deserialize)]
struct NokiaInstances {
    instance: Vec<NokiaNeighborInstance>,
}

impl NokiaIsNeighbor {
    fn to_ext_is_neighbor(
        &self,
        pseudonode_id: u8,
    ) -> Result<isis::ExtendedIsNeighbor, anyhow::Error> {
        let system_id_bytes = hex::decode(self.system_id.replace(".", "").to_lowercase())
            .map_err(|e| anyhow!("Failed to decode system ID: {}", e))?;

        if system_id_bytes.len() != 6 {
            return Err(anyhow!(
                "Invalid system ID length: expected 6 bytes, got {} bytes",
                system_id_bytes.len()
            ));
        }

        let system_id = isis::SystemId::new(&system_id_bytes)
            .map_err(|e| anyhow!("Failed to create SystemId: {}", e))?;

        let metric = self
            .instances
            .instance
            .get(0)
            .map(|instance| instance.metric)
            .unwrap_or(0);

        Ok(isis::ExtendedIsNeighbor {
            neighbor_id: system_id,
            pseudonode_id,
            metric,
        })
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
    id: String,
    #[serde(default)]
    metric: u32,

    #[serde(default)]
    subtlvs: Option<NokiaSubTlvs>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaHostname {
    hostname: Vec<String>,
}

impl From<NokiaHostname> for isis::Tlv {
    fn from(value: NokiaHostname) -> Self {
        Self::Hostname(value.hostname.into_iter().next().unwrap_or_default())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaIsReachability {
    neighbors: NokiaSimpleNeighbors,
}

#[derive(Debug, Deserialize)]
struct NokiaSimpleNeighbors {
    neighbor: Vec<NokiaSimpleNeighbor>,
}

impl From<NokiaIsReachability> for isis::IsReachabilityTlv {
    fn from(value: NokiaIsReachability) -> Self {
        let neighbors = value
            .neighbors
            .neighbor
            .into_iter()
            .filter_map(|neighbor| neighbor.try_into().ok())
            .collect();

        isis::IsReachabilityTlv { neighbors }
    }
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

impl TryFrom<NokiaSimpleNeighbor> for isis::IsNeighbor {
    type Error = anyhow::Error;

    fn try_from(value: NokiaSimpleNeighbor) -> Result<Self, Self::Error> {
        let system_id_bytes = hex::decode(value.system_id.replace(".", "").to_lowercase())
            .map_err(|e| anyhow!("Failed to decode system ID: {}", e))?;
        let system_id = isis::SystemId::new(&system_id_bytes)
            .map_err(|e| anyhow!("Failed to create SystemId: {}", e))?;
        let metric = value.default_metric.metric as u32;

        Ok(isis::IsNeighbor { system_id, metric })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaSimpleMetric {
    flags: u8,
    metric: u8,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NokiaRouterCapabilities {
    instance_number: u32,
    flags: u8,
    router_id: String,
    subtlvs: Option<NokiaSubTlvs>,
}

impl From<NokiaRouterCapabilities> for isis::RouterCapabilityTlv {
    fn from(value: NokiaRouterCapabilities) -> Self {
        let router_id = std::net::Ipv4Addr::from_str(&value.router_id).ok();
        let flood_bit = value.flags & 0x01 != 0;
        let down_bit = value.flags & 0x02 != 0;
        let mut flags = HashMap::new();
        flags.insert("Flood".to_string(), flood_bit);
        flags.insert("Down".to_string(), down_bit);

        isis::RouterCapabilityTlv {
            te_router_id: router_id,
            flags,
        }
    }
}

mod test {
    use super::*;
    use crate::{
        data_acquisition::transport::grpc::prelude::*,
        parsers::isis_parser::{hostname::HostnameMap, protocol::JsonIsisProtocol},
        topology::protocol::RoutingMessageProtocol,
    };

    #[test]
    fn parses_isis_dump() {
        let json: serde_json::Value =
            serde_json::from_str(include_str!("../../packets/resources/isis_dump.json")).unwrap();

        let lsps = NokiaGnmiIsisAdapter::lsp_json_to_lsp(json).unwrap();

        assert!(!lsps.is_empty());
        assert!(lsps
            .iter()
            .all(|lsp| { matches!(lsp.is_level, isis::IsLevel::Level1 | isis::IsLevel::Level2) }));
    }

    #[test]
    fn canonical_lsp_converts_to_topology_node() {
        let json: serde_json::Value =
            serde_json::from_str(include_str!("../../packets/resources/isis_dump.json")).unwrap();
        let lsp = NokiaGnmiIsisAdapter::lsp_json_to_lsp(json)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let protocol = JsonIsisProtocol::new(HostnameMap::empty());
        let nodes = protocol
            .message_to_nodes(RoutingMessage::IsIs(lsp))
            .unwrap();

        assert_eq!(nodes.len(), 1);
        let has_isis_data = match &nodes[0].info {
            crate::network::node::NodeInfo::Router(router) => router
                .protocol_data()
                .and_then(|data| data.isis())
                .is_some(),
            crate::network::node::NodeInfo::Network(network) => network
                .protocol_data()
                .and_then(|data| data.isis())
                .is_some(),
        };
        assert!(has_isis_data);
    }

    #[ignore = "requires a reachable SR Linux gNMI lab"]
    #[test]
    fn test_nokia_gnmi_isis_adapter() {
        let adapter = super::NokiaGnmiIsisAdapter;

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut client = GnmiClient::new("http://200.2.0.1:57401", "admin", "NokiaSrl1!")
                .await
                .unwrap();

            let paths = adapter.subscribtion_paths();
            let paths_str = paths.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
            let response = client.get(&paths_str).await.unwrap();
            let notifications: Vec<JsonIetfNotification> = (&response).try_into().unwrap();

            let messages = adapter
                .parse_transport_payload(TransportPayload::GnmiGet(notifications))
                .unwrap();

            assert!(!messages.is_empty());
            assert!(messages
                .iter()
                .all(|message| matches!(message, RoutingMessage::IsIs(_))));

            dbg!(&messages);
        });
    }
}
