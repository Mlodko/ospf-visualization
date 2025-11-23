use crate::{
    data_aquisition::snmp::SnmpClient,
    network::{
        node::{Network as NetStruct, Node, NodeInfo, ProtocolData},
        router::RouterId,
    },
    parsers::ospf_parser::{
        lsa::{LsaError, OspfLsdbEntry},
        source::{OspfDataSource, OspfRawRow},
    },
    topology::protocol::{ProtocolParseError, ProtocolTopologyError},
};
use async_trait::async_trait;

use ipnetwork::IpNetwork;

/// Stateless OSPF protocol adapter. Parsing & node mapping are record-local;
/// consolidation and augmentation (summary folding & stub synthesis) happen in `post_process`.
pub struct OspfProtocol;

impl super::protocol::RoutingProtocol for OspfProtocol {
    type RawRecord = OspfRawRow;
    type ParsedItem = OspfLsdbEntry;

    fn parse(
        &self,
        raw: Self::RawRecord,
    ) -> Result<Vec<Self::ParsedItem>, super::protocol::ProtocolParseError> {
        let parsed = OspfLsdbEntry::try_from(raw)
            .map_err(|e| ProtocolParseError::Malformed(format!("{:?}", e)))?;
        Ok(vec![parsed])
    }

    fn item_to_node(
        &self,
        item: Self::ParsedItem,
    ) -> Result<Option<Node>, super::protocol::ProtocolTopologyError> {
        match item.try_into() as Result<Node, LsaError> {
            Ok(node) => Ok(Some(node)),
            Err(LsaError::InvalidLsaType) => Ok(None), // Skip unsupported LSA types
            Err(e) => Err(ProtocolTopologyError::Conversion(format!("{:?}", e))),
        }
    }

    fn post_process(
        &self,
        nodes: &mut Vec<Node>,
    ) -> Result<(), super::protocol::ProtocolTopologyError> {
        consolidate_networks(nodes)?;
        synthesize_stub_networks(nodes)?;
        Ok(())
    }
}

/* ---------------------- Consolidation (Summary ↔ Detailed) ---------------------- */

fn consolidate_networks(nodes: &mut Vec<Node>) -> Result<(), ProtocolTopologyError> {
    use std::collections::{HashMap, HashSet};

    // Classification of OSPF network nodes based on advertisement variant.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    enum NetKind {
        Detailed, // Type-2 Network-LSA
        Summary,  // Type-3 Summary IP Network LSA
        Other,
    }

    fn classify(node: &Node) -> NetKind {
        match &node.info {
            NodeInfo::Network(net) => {
                if let Some(ProtocolData::Ospf(data)) = &net.protocol_data {
                    use ospf_parser::OspfLinkStateAdvertisement::*;
                    match *data.advertisement {
                        NetworkLinks(_) => NetKind::Detailed,
                        SummaryLinkIpNetwork(_) => NetKind::Summary,
                        _ => NetKind::Other,
                    }
                } else {
                    NetKind::Other
                }
            }
            _ => NetKind::Other,
        }
    }

    // Separate routers; build per-prefix map merging summary & detailed.
    let mut routers: Vec<Node> = Vec::new();
    let mut by_prefix: HashMap<IpNetwork, Node> = HashMap::new();

    // Drain original vec to avoid double borrow issues.
    let original = std::mem::take(nodes);
    for node in original.into_iter() {
        match &node.info {
            NodeInfo::Router(_) => routers.push(node),
            NodeInfo::Network(net) => {
                let key = net.ip_address;
                let kind = classify(&node);
                match by_prefix.remove(&key) {
                    None => {
                        by_prefix.insert(key, node);
                    }
                    Some(existing) => {
                        let existing_kind = classify(&existing);
                        // Decide which becomes base:
                        let (mut base, extra, base_kind, extra_kind) = match (existing_kind, kind) {
                            (NetKind::Detailed, NetKind::Summary) => {
                                (existing, node, existing_kind, kind)
                            }
                            (NetKind::Summary, NetKind::Detailed) => {
                                (node, existing, kind, existing_kind)
                            }
                            // Same kind or other cases - keep first as base
                            _ => (existing, node, existing_kind, kind),
                        };

                        // Merge attached routers if any side is detailed OR both summary.
                        if let (NodeInfo::Network(base_net), NodeInfo::Network(extra_net)) =
                            (&mut base.info, &extra.info)
                        {
                            let should_union_attached = matches!(base_kind, NetKind::Detailed)
                                || matches!(extra_kind, NetKind::Detailed)
                                || (matches!(base_kind, NetKind::Summary)
                                    && matches!(extra_kind, NetKind::Summary));
                            if should_union_attached {
                                let mut seen: HashSet<uuid::Uuid> = base_net
                                    .attached_routers
                                    .iter()
                                    .map(|r| r.to_uuidv5())
                                    .collect();
                                for r in &extra_net.attached_routers {
                                    if seen.insert(r.to_uuidv5()) {
                                        base_net.attached_routers.push(r.clone());
                                    }
                                }
                            }

                            // Merge summary metrics (OspfPayload::Network summaries).
                            if let (
                                Some(ProtocolData::Ospf(base_pd)),
                                Some(ProtocolData::Ospf(extra_pd)),
                            ) = (&mut base_net.protocol_data, &extra_net.protocol_data)
                            {
                                use crate::network::node::OspfPayload;
                                match (&mut base_pd.payload, &extra_pd.payload) {
                                    (
                                        OspfPayload::Network(base_payload),
                                        OspfPayload::Network(extra_payload),
                                    ) => {
                                        if !extra_payload.summaries.is_empty() {
                                            let mut seen: HashSet<(u32, uuid::Uuid)> = base_payload
                                                .summaries
                                                .iter()
                                                .map(|s| (s.metric, s.origin_abr.to_uuidv5()))
                                                .collect();
                                            for s in &extra_payload.summaries {
                                                let sig = (s.metric, s.origin_abr.to_uuidv5());
                                                if seen.insert(sig) {
                                                    // Accept if:
                                                    // - base is detailed and extra is summary
                                                    // - both summary (union)
                                                    // - base summary & extra summary (already above)
                                                    if matches!(base_kind, NetKind::Detailed)
                                                        || (matches!(base_kind, NetKind::Summary)
                                                            && matches!(
                                                                extra_kind,
                                                                NetKind::Summary
                                                            ))
                                                    {
                                                        base_payload.summaries.push(s.clone());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    _ => {
                                        // Non-network OSPF payload patterns ignored.
                                    }
                                }
                            }
                        }

                        by_prefix.insert(key, base);
                    }
                }
            }
        }
    }

    // Rebuild consolidated node list
    let mut consolidated: Vec<Node> = routers;
    consolidated.extend(by_prefix.into_values());
    *nodes = consolidated;
    Ok(())
}

/* ---------------------- Stub Network Synthesis ---------------------- */

fn synthesize_stub_networks(nodes: &mut Vec<Node>) -> Result<(), ProtocolTopologyError> {
    use std::collections::HashSet;
    use std::net::IpAddr;

    // Collect existing prefixes
    let mut existing_prefixes: HashSet<IpNetwork> = nodes
        .iter()
        .filter_map(|n| {
            if let NodeInfo::Network(net) = &n.info {
                Some(net.ip_address)
            } else {
                None
            }
        })
        .collect();

    // Snapshot router advertisements (Router-LSAs) for stub link extraction.
    let mut router_advs: Vec<(
        RouterId,
        Option<std::sync::Arc<ospf_parser::OspfLinkStateAdvertisement>>,
    )> = Vec::new();

    for n in nodes.iter() {
        if let NodeInfo::Router(r) = &n.info {
            let adv = r.protocol_data.as_ref().and_then(|pd| {
                if let ProtocolData::Ospf(ospf) = pd {
                    Some(ospf.advertisement.clone())
                } else {
                    None
                }
            });
            router_advs.push((r.id.clone(), adv));
        }
    }

    // Phase 1: identify new stub prefixes & attachments for existing ones.
    let mut new_stub_prefixes: Vec<(IpNetwork, RouterId)> = Vec::new();
    let mut attach_existing: Vec<(IpNetwork, RouterId)> = Vec::new();

    for (rid, adv_opt) in &router_advs {
        let Some(adv_arc) = adv_opt else { continue };
        if let ospf_parser::OspfLinkStateAdvertisement::RouterLinks(router_links) = &**adv_arc {
            for link in &router_links.links {
                if matches!(link.link_type, ospf_parser::OspfRouterLinkType::Stub) {
                    let net_addr_v4 = link.link_id();
                    let mask_v4 = link.link_data();
                    if let Ok(stub_prefix) =
                        IpNetwork::with_netmask(IpAddr::V4(net_addr_v4), IpAddr::V4(mask_v4))
                    {
                        if !existing_prefixes.contains(&stub_prefix) {
                            new_stub_prefixes.push((stub_prefix, rid.clone()));
                        } else {
                            attach_existing.push((stub_prefix, rid.clone()));
                        }
                    }
                }
            }
        }
    }

    // Phase 2: apply synthetic creations
    let mut _synthetic_added = 0usize;
    for (stub_prefix, rid) in new_stub_prefixes {
        let synthetic_net = NetStruct {
            ip_address: stub_prefix,
            protocol_data: None,
            attached_routers: vec![rid.clone()],
        };
        nodes.push(Node::new(NodeInfo::Network(synthetic_net), None));
        existing_prefixes.insert(stub_prefix);
        _synthetic_added += 1;
    }

    // Phase 3: attach routers to existing stub networks if missing
    if !attach_existing.is_empty() {
        for (stub_prefix, rid) in attach_existing {
            for n in nodes.iter_mut() {
                let NodeInfo::Network(net) = &mut n.info else {
                    continue;
                };
                if net.ip_address == stub_prefix
                    && !net
                        .attached_routers
                        .iter()
                        .any(|r_existing| r_existing == &rid)
                {
                    net.attached_routers.push(rid.clone());
                }
            }
        }
    }

    // Optional: could log synthetic_added; skipped here to keep post_process lean.
    Ok(())
}

/* ---------------------- (Optional future: helper extraction) ----------------------
The consolidation & stub synthesis are kept local for clarity. If multiple protocols
need similar patterns (e.g., summary/detailed merging), factor them into a shared
`topology::algorithms` module later.
----------------------------------------------------------------------------------- */

/// OSPF-over-SNMP acquisition wrapper implementing the generic AcquisitionSource for OspfProtocol.
pub struct OspfSnmpAcquisition {
    inner: crate::parsers::ospf_parser::snmp_source::OspfSnmpSource,
}

impl OspfSnmpAcquisition {
    pub fn new(client: crate::data_aquisition::snmp::SnmpClient) -> Self {
        Self {
            inner: crate::parsers::ospf_parser::snmp_source::OspfSnmpSource::new(client),
        }
    }
}

#[async_trait]
impl super::protocol::AcquisitionSource<OspfProtocol> for OspfSnmpAcquisition {
    async fn fetch_raw(&mut self) -> Result<Vec<OspfRawRow>, super::protocol::AcquisitionError> {
        self.inner.fetch_lsdb_rows().await.map_err(|e| match e {
            crate::parsers::ospf_parser::source::OspfSourceError::Acquisition(s) => {
                super::protocol::AcquisitionError::Transport(s)
            }
            crate::parsers::ospf_parser::source::OspfSourceError::Invalid(s) => {
                super::protocol::AcquisitionError::Invalid(s)
            }
        })
    }

    async fn fetch_source_id(
        &mut self,
    ) -> Result<crate::topology::store::SourceId, super::protocol::AcquisitionError> {
        self.inner.fetch_source_id().await.map_err(|e| match e {
            crate::parsers::ospf_parser::source::OspfSourceError::Acquisition(s) => {
                super::protocol::AcquisitionError::Transport(s)
            }
            crate::parsers::ospf_parser::source::OspfSourceError::Invalid(s) => {
                super::protocol::AcquisitionError::Invalid(s)
            }
        })
    }
}

/// Convenience alias matching previous API style.
pub type OspfSnmpTopology = super::protocol::Topology<OspfProtocol, OspfSnmpAcquisition>;

pub fn new_ospf_snmp_topology(client: SnmpClient) -> OspfSnmpTopology {
    super::protocol::Topology::new(OspfProtocol, OspfSnmpAcquisition::new(client))
}
