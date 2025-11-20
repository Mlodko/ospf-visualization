use async_trait::async_trait;

use crate::data_aquisition::snmp::SnmpClient;
use crate::network::node::{Node, NodeInfo, OspfPayload, ProtocolData};
use crate::network::router::RouterId;
use crate::parsers::ospf_parser::lsa::{LsaError, OspfLsdbEntry};
use crate::parsers::ospf_parser::snmp_source::OspfSnmpSource;
use crate::parsers::ospf_parser::source::{OspfDataSource, OspfSourceError};
use crate::topology::source::{SnapshotSource, TopologyError, TopologySource};
use crate::topology::store::SourceId;
use std::collections::HashMap;

/// OSPF-over-SNMP implementation of the GUI-facing topology source.
/// Internally, this uses the protocol-centric OspfDataSource (implemented by OspfSnmpSource)
/// and converts parsed LSAs into protocol-agnostic `Node`s.
pub struct OspfTopology<S: OspfDataSource> {
    source: S,
}

/// Convenience alias for the SNMP-backed OSPF topology.
pub type OspfSnmpTopology = OspfTopology<OspfSnmpSource>;

impl OspfTopology<OspfSnmpSource> {
    /// Construct an SNMP-backed OSPF topology from an SNMP client.
    pub fn new(client: SnmpClient) -> Self {
        Self {
            source: OspfSnmpSource::new(client),
        }
    }
}

#[async_trait]
impl<S: OspfDataSource + Send + Sync> TopologySource for OspfTopology<S> {
    async fn fetch_nodes(&mut self) -> Result<Vec<Node>, TopologyError> {
        let rows = self
            .source
            .fetch_lsdb_rows()
            .await
            .map_err(map_ospf_source_err)?;

        let mut nodes = Vec::with_capacity(rows.len());
        for row in rows {
            let entry = OspfLsdbEntry::try_from(row)
                .map_err(|e| TopologyError::Protocol(format!("{:?}", e)))?;
            match entry.try_into() as Result<Node, LsaError> {
                Ok(node) => nodes.push(node),
                Err(LsaError::InvalidLsaType) => {
                    // Skip unsupported LSA types for topology nodes
                }
                Err(e) => return Err(TopologyError::Protocol(format!("{:?}", e))),
            }
        }

        // Consolidation: merge summary (Type-3) networks into detailed (Type-2) networks.
        use std::collections::HashSet;
        let mut routers: Vec<Node> = Vec::new();
        let mut by_prefix: HashMap<IpNetwork, Node> = HashMap::new();

        let classify = |n: &Node| -> (&'static str, bool) {
            if let NodeInfo::Network(net) = &n.info {
                if let Some(ProtocolData::Ospf(data)) = &net.protocol_data {
                    match *data.advertisement {
                        ospf_parser::OspfLinkStateAdvertisement::NetworkLinks(_) => {
                            ("detailed", true)
                        }
                        ospf_parser::OspfLinkStateAdvertisement::SummaryLinkIpNetwork(_) => {
                            ("summary", false)
                        }
                        _ => ("other", false),
                    }
                } else {
                    ("no-proto", false)
                }
            } else {
                ("router", false)
            }
        };

        for node in nodes.into_iter() {
            match &node.info {
                NodeInfo::Router(_) => routers.push(node),
                NodeInfo::Network(net) => {
                    let key = net.ip_address;
                    let (kind, is_detailed) = classify(&node);
                    println!(
                        "[OSPF consolidate] observe prefix={} kind={} attached_routers={}",
                        key,
                        kind,
                        net.attached_routers.len()
                    );
                    if let Some(mut existing) = by_prefix.remove(&key) {
                        let (existing_kind, existing_detailed) = classify(&existing);
                        let (mut base, extra, base_kind, extra_kind) =
                            if existing_detailed && !is_detailed {
                                (existing, node, existing_kind, kind)
                            } else if is_detailed && !existing_detailed {
                                (node, existing, kind, existing_kind)
                            } else {
                                (existing, node, existing_kind, kind)
                            };

                        println!(
                            "[OSPF consolidate] merge prefix={} base_kind={} extra_kind={}",
                            key, base_kind, extra_kind
                        );

                        if let (NodeInfo::Network(base_net), NodeInfo::Network(extra_net)) =
                            (&mut base.info, &extra.info)
                        {
                            let should_union_attached = base_kind == "detailed"
                                || extra_kind == "detailed"
                                || (base_kind == "summary" && extra_kind == "summary");
                            if should_union_attached {
                                let mut seen: HashSet<uuid::Uuid> = base_net
                                    .attached_routers
                                    .iter()
                                    .map(|r| r.to_uuidv5())
                                    .collect();
                                for r in &extra_net.attached_routers {
                                    let id = r.to_uuidv5();
                                    if seen.insert(id) {
                                        base_net.attached_routers.push(r.clone());
                                    }
                                }
                                println!(
                                    "[OSPF consolidate] prefix={} attached_union_size={}",
                                    key,
                                    base_net.attached_routers.len()
                                );
                            }

                            if let (
                                Some(ProtocolData::Ospf(base_pd)),
                                Some(ProtocolData::Ospf(extra_pd)),
                            ) = (&mut base_net.protocol_data, &extra_net.protocol_data)
                            {
                                match (&mut base_pd.payload, &extra_pd.payload) {
                                    (
                                        OspfPayload::Network(base_payload),
                                        OspfPayload::Network(extra_payload),
                                    ) => {
                                        // Merge summary metrics (extra_payload.summaries) into base if:
                                        // - base is detailed and extra is summary
                                        // - both are summary (union)
                                        // Dedupe by (metric, origin_abr UUID).
                                        use std::collections::HashSet;
                                        if !extra_payload.summaries.is_empty() {
                                            let mut seen: HashSet<(u32, uuid::Uuid)> = base_payload
                                                .summaries
                                                .iter()
                                                .map(|s| (s.metric, s.origin_abr.to_uuidv5()))
                                                .collect();
                                            let mut added = 0usize;
                                            for s in &extra_payload.summaries {
                                                let key_sig = (s.metric, s.origin_abr.to_uuidv5());
                                                if seen.insert(key_sig) {
                                                    // Accept into base
                                                    if base_kind == "detailed"
                                                        || (base_kind == "summary"
                                                            && extra_kind == "summary")
                                                    {
                                                        base_payload.summaries.push(s.clone());
                                                        added += 1;
                                                    }
                                                }
                                            }
                                            if added > 0 {
                                                println!(
                                                    "[OSPF consolidate] prefix={} merged_summary_entries_added={} (base_kind={}, extra_kind={})",
                                                    key, added, base_kind, extra_kind
                                                );
                                            } else {
                                                println!(
                                                    "[OSPF consolidate] prefix={} no new summary entries merged (all duplicates)",
                                                    key
                                                );
                                            }
                                        }
                                        if base_kind == "detailed" && extra_kind == "summary" {
                                            println!(
                                                "[OSPF consolidate] prefix={} summary folded into detailed network (total summaries={})",
                                                key,
                                                base_payload.summaries.len()
                                            );
                                        }
                                    }
                                    _ => {
                                        println!(
                                            "[OSPF consolidate] prefix={} payload pattern not merged (base_kind={}, extra_kind={})",
                                            key, base_kind, extra_kind
                                        );
                                    }
                                }
                            }
                        }
                        by_prefix.insert(key, base);
                    } else {
                        by_prefix.insert(key, node);
                    }
                }
            }
        }

        let mut consolidated: Vec<Node> = routers;
        consolidated.extend(by_prefix.into_values());

        // Augmentation replaced: use OSPF semantics.
        // - Do NOT add routers to detailed Network-LSAs (they already enumerate membership).
        // - Do NOT add routers to summary networks (inter-area reachability only).
        // - Perform stub network synthesis: create synthetic network nodes for stub links
        //   without an existing Network-LSA, attaching only the advertising router.
        //
        // Stub logic:
        // For each Router-LSA:
        //   For each stub link (link_type == Stub):
        //     link_id => network address, link_data => mask
        //     Construct IpNetwork(prefix). If not present among consolidated networks,
        //     create synthetic network node.
        //
        // Transit / P2P augmentation is skipped because Network-LSA coverage is authoritative.
        //
        // Detailed logging retained for synthesis.

        use ipnetwork::IpNetwork;

        // Collect existing network prefixes for quick lookup.
        let mut existing_prefixes: HashSet<IpNetwork> = consolidated
            .iter()
            .filter_map(|n| {
                if let NodeInfo::Network(net) = &n.info {
                    Some(net.ip_address)
                } else {
                    None
                }
            })
            .collect();

        // Snapshot routers with their Router-LSA advertisements.
        let router_lsa_snapshots: Vec<(
            &RouterId,
            crate::network::router::Router,
            Option<std::sync::Arc<ospf_parser::OspfLinkStateAdvertisement>>,
        )> = consolidated
            .iter()
            .filter_map(|n| {
                if let NodeInfo::Router(r) = &n.info {
                    // Extract advertisement (RouterLinks) if present
                    let adv = r.protocol_data.as_ref().and_then(|pd| {
                        if let ProtocolData::Ospf(ospf) = pd {
                            Some(ospf.advertisement.clone())
                        } else {
                            None
                        }
                    });
                    Some((&r.id, r.clone(), adv))
                } else {
                    None
                }
            })
            .collect();

        // Synthetic stub network generation
        let mut synthetic_added = 0usize;

        // Two-phase stub processing to satisfy borrow checker:
        // 1) Collect candidate new stub prefixes and existing prefix attachments without mutating `consolidated`.
        // 2) Apply mutations (create synthetic networks / attach routers) after iteration.
        let mut new_stub_prefixes: Vec<(IpNetwork, RouterId)> = Vec::new();
        let mut attach_existing: Vec<(IpNetwork, RouterId)> = Vec::new();

        for (rid, _router, adv_opt) in &router_lsa_snapshots {
            if let Some(adv_arc) = adv_opt {
                if let ospf_parser::OspfLinkStateAdvertisement::RouterLinks(router_links) =
                    &**adv_arc
                {
                    for link in &router_links.links {
                        if matches!(link.link_type, ospf_parser::OspfRouterLinkType::Stub) {
                            let net_addr_v4 = link.link_id();
                            let mask_v4 = link.link_data();
                            if let Ok(stub_prefix) = IpNetwork::with_netmask(
                                std::net::IpAddr::V4(net_addr_v4),
                                std::net::IpAddr::V4(mask_v4),
                            ) {
                                if !existing_prefixes.contains(&stub_prefix) {
                                    new_stub_prefixes.push((stub_prefix, (*rid).clone()));
                                } else {
                                    attach_existing.push((stub_prefix, (*rid).clone()));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Apply synthetic creations
        use crate::network::node::{Network as NetStruct, NodeInfo as NI};
        for (stub_prefix, rid) in new_stub_prefixes {
            let synthetic_net = NetStruct {
                ip_address: stub_prefix,
                protocol_data: None,
                attached_routers: vec![rid.clone()],
            };
            consolidated.push(Node::new(NI::Network(synthetic_net), None));
            existing_prefixes.insert(stub_prefix);
            synthetic_added += 1;
            println!(
                "[OSPF consolidate][synthetic-stub] created prefix={} router={} (from stub link)",
                stub_prefix, rid
            );
        }

        // Attach routers to existing stub networks if missing
        for (stub_prefix, rid) in attach_existing {
            for n in consolidated.iter_mut() {
                if let NodeInfo::Network(net) = &mut n.info {
                    if net.ip_address == stub_prefix
                        && !net
                            .attached_routers
                            .iter()
                            .any(|r_existing| r_existing == &rid)
                    {
                        net.attached_routers.push(rid.clone());
                        println!(
                            "[OSPF consolidate][synthetic-stub] attach router={} to existing stub prefix={}",
                            rid, stub_prefix
                        );
                    }
                }
            }
        }

        if synthetic_added > 0 {
            println!(
                "[OSPF consolidate][synthetic] stub_networks_added={} (generated from Router-LSA stub links)",
                synthetic_added
            );
        } else {
            println!("[OSPF consolidate][synthetic] no stub networks generated");
        }

        // Removed obsolete pseudo /32 synthetic generation block (stub synthesis now handled earlier with proper Router-LSA semantics).

        println!(
            "[OSPF consolidate] final counts: routers={} networks={}",
            consolidated
                .iter()
                .filter(|n| matches!(n.info, NodeInfo::Router(_)))
                .count(),
            consolidated
                .iter()
                .filter(|n| matches!(n.info, NodeInfo::Network(_)))
                .count()
        );

        let total_attached: usize = consolidated
            .iter()
            .filter_map(|n| {
                if let NodeInfo::Network(net) = &n.info {
                    Some(net.attached_routers.len())
                } else {
                    None
                }
            })
            .sum();
        println!(
            "[OSPF consolidate] aggregate attached router refs across networks={}",
            total_attached
        );
        
        for node in consolidated.iter() {
            if let NodeInfo::Network(net) = &node.info {
                let router_ips: Vec<String> = net.attached_routers.iter()
                    .map(|r| {
                        if let RouterId::Ipv4(addr) = r {
                            Some(addr.to_string())
                        } else {
                            None
                        }
                    })
                    .filter_map(|addr| addr)
                    .collect();
                let routers = router_ips.join(", ");
                println!("{}: {}", net.ip_address, routers);
            }
        }

        Ok(consolidated)
    }
}

#[async_trait]
impl SnapshotSource for OspfTopology<OspfSnmpSource> {
    async fn fetch_source_id(&mut self) -> Result<SourceId, TopologyError> {
        self.source
            .fetch_source_id()
            .await
            .map_err(map_ospf_source_err)
    }
}

fn map_ospf_source_err(err: OspfSourceError) -> TopologyError {
    match err {
        OspfSourceError::Acquisition(s) => TopologyError::Acquisition(s),
        OspfSourceError::Invalid(s) => TopologyError::Protocol(s),
    }
}
