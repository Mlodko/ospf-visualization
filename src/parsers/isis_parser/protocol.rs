use std::{collections::HashSet, net::Ipv4Addr};

use ipnetwork::IpNetwork;
use uuid::Uuid;

use crate::{
    network::{
        node::{IsIsData, Network, Node, NodeInfo, ProtocolData},
        router::{Router, RouterId},
    },
    parsers::isis_parser::{
        core_lsp::{ExtendedIpReachabilityTlv, Lsp, LspError, LspId, NetAddress, SystemId, Tlv},
        frr_json_lsp::JsonLspdb,
        hostname::HostnameMap,
    },
    topology::protocol::{ProtocolParseError, ProtocolTopologyError, RoutingProtocol},
};

/*
Due to how bad FRR's LSPDB JSON output is, we need to get the VRF data to get the actual
System ID for all LSPs instead of hostnames.

*/
pub struct JsonIsisProtocol {
    hostname_map: HostnameMap,
}

impl JsonIsisProtocol {
    pub fn new(hostname_map: HostnameMap) -> Self {
        Self { hostname_map }
    }

    fn lsp_to_router(&self, lsp: Lsp) -> Result<Router, ProtocolTopologyError> {
        let id = RouterId::IsIs(lsp.system_id.clone());
        let net_address = lsp.get_net_address();
        let protocol_data = ProtocolData::IsIs(IsIsData {
            is_level: lsp.is_level,
            lsp_id: lsp.lsp_id,
            tlvs: lsp.tlvs,
            net_address: net_address,
        });

        Ok(Router {
            id,
            interfaces: Vec::new(), // We leave this empty since IS-IS works at the link layer
            protocol_data: Some(protocol_data),
        })
    }

    fn lsp_to_network(&self, lsp: Lsp) -> Result<Network, ProtocolTopologyError> {
        let protocol_data = ProtocolData::IsIs(IsIsData {
            net_address: lsp.get_net_address(),
            is_level: lsp.is_level,
            lsp_id: lsp.lsp_id,
            tlvs: lsp.tlvs,
        });

        // Moved to post_processing - pseudonode LSP doesn't hold the IP prefix
        let ip_prefix: IpNetwork = IpNetwork::new(
            std::net::IpAddr::V4(Ipv4Addr::from_octets([0, 0, 0, 0])),
            32,
        )
        .unwrap();

        Ok(Network {
            ip_address: ip_prefix,
            protocol_data: Some(protocol_data),
            attached_routers: vec![],
        })
    }
}

impl RoutingProtocol for JsonIsisProtocol {
    type RawRecord = JsonLspdb;

    type ParsedItem = Lsp;

    fn parse(
        &self,
        raw: Self::RawRecord,
    ) -> Result<Vec<Self::ParsedItem>, crate::topology::protocol::ProtocolParseError> {
        let mut lsps = Vec::new();
        for area in raw.areas {
            for level in area.levels {
                let level_no = level.id;
                for lsp in level.lsps {
                    let parsed = lsp
                        .try_into_lsp(level_no, &self.hostname_map)
                        .map_err(|e| e.into())?;
                    lsps.push(parsed);
                }
            }
        }
        Ok(lsps)
    }

    fn item_to_node(
        &self,
        item: Self::ParsedItem,
    ) -> Result<Option<crate::network::node::Node>, crate::topology::protocol::ProtocolTopologyError>
    {
        // Explicitly lean, draw edges in post_process (with context from other nodes)
        let label = if let Some(Tlv::Hostname(hostname)) =
            item.get_tlvs_by(|t| matches!(t, Tlv::Hostname(_))).first()
        {
            Some(hostname.clone())
        } else {
            None
        };

        println!("Processing LSP of ID: {}", &item.lsp_id);

        let node_info = if item.lsp_id.is_pseudonode() {
            NodeInfo::Network(Self::lsp_to_network(&self, item)?)
        } else {
            NodeInfo::Router(Self::lsp_to_router(&self, item)?)
        };

        println!("Processed successfully");

        Ok(Some(Node::new(node_info, label)))
    }

    fn post_process(
        &self,
        nodes: &mut Vec<crate::network::node::Node>,
    ) -> Result<(), crate::topology::protocol::ProtocolTopologyError> {
        // Link nodes based on their data (source-local)
        // NOTE: Added debug prints and optional processing limits for diagnostics.
        // Environment variables:
        //   ISIS_POST_MAX_NETWORKS - if set to a positive integer, only process up to that many network nodes.
        //   ISIS_POST_MAX_ROUTERS  - if set to a positive integer, only consider up to that many router nodes when resolving prefixes.
        use std::env;

        println!(
            "[JsonIsisProtocol::post_process] start: total nodes={}",
            nodes.len()
        );

        // Link nodes based on their data (source-local)
        let mut network_idxs: Vec<usize> = Vec::new();
        let mut router_idxs: Vec<usize> = Vec::new();

        for (idx, node) in nodes.iter().enumerate() {
            match &node.info {
                NodeInfo::Network(_) => network_idxs.push(idx),
                NodeInfo::Router(_) => router_idxs.push(idx),
            }
        }

        println!(
            "[JsonIsisProtocol::post_process] found {} networks and {} routers",
            network_idxs.len(),
            router_idxs.len()
        );

        // Optional limits for diagnostics
        let max_networks = env::var("ISIS_POST_MAX_NETWORKS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok());
        let max_routers = env::var("ISIS_POST_MAX_ROUTERS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok());

        if let Some(m) = max_networks {
            println!(
                "[JsonIsisProtocol::post_process] limiting network processing to {}",
                m
            );
        }
        if let Some(m) = max_routers {
            println!(
                "[JsonIsisProtocol::post_process] limiting router consideration to {}",
                m
            );
        }

        // If limits are present, produce truncated lists for iteration
        let network_iter_idxs: Vec<usize> = if let Some(m) = max_networks {
            network_idxs.iter().cloned().take(m).collect()
        } else {
            network_idxs.clone()
        };

        let router_iter_idxs_full = router_idxs.clone();

        for &net_idx in &network_iter_idxs {
            let router_subset: Vec<usize> = if let Some(m) = max_routers {
                router_iter_idxs_full.iter().cloned().take(m).collect()
            } else {
                router_iter_idxs_full.iter().cloned().collect()
            };

            println!(
                "[JsonIsisProtocol::post_process] processing network idx={} with {} router candidates",
                net_idx,
                router_subset.len()
            );

            /*
            // Figure out network's NET address, find the DIS and copy its area address
            let network_net_address = if let NodeInfo::Network(net) = &nodes[net_idx].info {
                if let Some(ProtocolData::IsIs(data)) = &net.protocol_data {
                    // Inline DIS lookup using router indices (avoid holding &Node refs across mutable borrows).
                    let dis_router = {
                        let mut found: Option<Router> = None;
                        for &r_idx in &router_subset {
                            if let NodeInfo::Router(router) = &nodes[r_idx].info {
                                if let Some(ProtocolData::IsIs(rdata)) = &router.protocol_data {
                                    if data.lsp_id.is_pseudonode_of(&rdata.lsp_id) {
                                        found = Some(router.clone());
                                        break;
                                    }
                                }
                            }
                        }
                        found
                    };

                    if let Some(dis_router) = dis_router {
                        println!("Found DIS router: {:?}", dis_router);
                        if let RouterId::IsIs(net_address) = dis_router.id {
                            let area_address = net_address.area_address.clone();
                            let system_id = data.lsp_id.get_system_id().map_err(|e| {
                                ProtocolTopologyError::Semantic(format!(
                                    "Couldn't infer system ID: {}",
                                    e
                                ))
                            })?;
                            let net_addr = NetAddress {
                                area_address,
                                system_id,
                            };
                            println!("Found NET address: {}", &net_addr);
                            Ok(net_addr)
                        } else {
                            Err(ProtocolTopologyError::Semantic(
                                "Couldn't find ISIS data in DIS".to_string(),
                            ))
                        }
                    } else {
                        Err(ProtocolTopologyError::Semantic(
                            "Couldn't find DIS router".to_string(),
                        ))
                    }
                } else {
                    Err(ProtocolTopologyError::Semantic(
                        "Couldn't find ISIS data in network node".to_string(),
                    ))
                }
            } else {
                Err(ProtocolTopologyError::Semantic(
                    "Node is not a network node".to_string(),
                ))
            }?;
            */

            // Resolve prefix with diagnostics: capture call and any error.
            // Build a temporary slice of &Node from router indices so we can pass the expected type
            // to `resolve_network_prefix` without holding mutable borrows of `nodes`.
            let router_subset_refs: Vec<&crate::network::node::Node> =
                router_subset.iter().map(|&i| &nodes[i]).collect();
            let prefix_result = {
                let net_ref: &crate::network::node::Node = &nodes[net_idx];
                resolve_network_prefix(net_ref, &router_subset_refs)
            };

            match prefix_result {
                Ok(prefix) => {
                    let node = &mut nodes[net_idx];
                    if let NodeInfo::Network(net) = &mut node.info {
                        net.ip_address = prefix;

                        // Recompute node UUID so the network node identity reflects the resolved prefix.
                        // This keeps Node.id consistent with Node::new() behavior for Network nodes
                        // (which uses the prefix string to compute a v5 UUID).
                        node.id = Uuid::new_v5(
                            &Uuid::NAMESPACE_OID,
                            net.ip_address.to_string().as_bytes(),
                        );

                        println!(
                            "[JsonIsisProtocol::post_process] set prefix for network idx={} to {} (recomputed id={})",
                            net_idx, net.ip_address, node.id
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[JsonIsisProtocol::post_process] failed to resolve prefix for network idx={} due to {:?}",
                        net_idx, e
                    );
                }
            }

            let attached_routers: Vec<SystemId> =
                if let NodeInfo::Network(net) = &nodes[net_idx].info {
                    if let Some(ProtocolData::IsIs(data)) = &net.protocol_data {
                        if let Some(Tlv::ExtendedReachability(tlv)) = data
                            .tlvs
                            .iter()
                            .find(|tlv| matches!(tlv, Tlv::ExtendedReachability(_)))
                        {
                            // For each neighbor, prefer the area_address from the actual router node
                            // (matched by SystemId) so RouterId::IsIs(NetAddress) matches the router node.
                            tlv.neighbors
                                .iter()
                                .map(|neighbor| neighbor.neighbor_id.clone())
                                .collect()
                        } else {
                            println!("Missing extended reachability TLV");
                            return Err(ProtocolTopologyError::Semantic(
                                "Missing extended reachability TLV".to_string(),
                            ));
                        }
                    } else {
                        println!("Missing ISIS protocol data");
                        return Err(ProtocolTopologyError::Semantic(
                            "Missing ISIS protocol data".to_string(),
                        ));
                    }
                } else {
                    println!("Missing ISIS protocol data");
                    return Err(ProtocolTopologyError::Semantic(
                        "Missing ISIS protocol data".to_string(),
                    ));
                };

            if let NodeInfo::Network(net) = &mut nodes[net_idx].info {
                net.attached_routers = attached_routers
                    .into_iter()
                    .map(|net| RouterId::IsIs(net))
                    .collect();
                println!("Attached routers set: {:?}", net.attached_routers);
            }
        }

        println!("[JsonIsisProtocol::post_process] complete");
        Ok(())
    }
}

impl Into<ProtocolParseError> for LspError {
    fn into(self) -> ProtocolParseError {
        match self {
            _ => ProtocolParseError::Malformed(self.to_string()),
        }
    }
}

fn find_dis_router(network_lsp_id: &LspId, router_nodes: &[&Node]) -> Option<Router> {
    router_nodes.iter().find_map(|node| {
        if let NodeInfo::Router(router) = &node.info {
            if let Some(ProtocolData::IsIs(data)) = &router.protocol_data {
                if network_lsp_id.is_pseudonode_of(&data.lsp_id) {
                    return Some(router.clone());
                }
            }
        }
        None
    })
}

fn resolve_network_prefix(
    network_node: &Node,
    router_nodes: &[&Node],
) -> Result<IpNetwork, ProtocolTopologyError> {
    // Debug-enabled resolver: emits progress logs and provides short-circuiting for diagnostics.
    use std::env;
    println!("[resolve_network_prefix] start");

    // Step 0: Extract data and check if network is a pseudonode
    let network = if let NodeInfo::Network(net) = &network_node.info {
        net
    } else {
        eprintln!("[resolve_network_prefix] provided non-network node");
        return Err(ProtocolTopologyError::Semantic(
            "Non-network node provided to resolve_network_prefix".to_string(),
        ));
    };

    let isis_data = if let Some(ProtocolData::IsIs(data)) = &network.protocol_data {
        data
    } else {
        eprintln!("[resolve_network_prefix] network has no IS-IS protocol data");
        return Err(ProtocolTopologyError::Semantic(
            "Non-IS-IS node provided to resolve_network_prefix".to_string(),
        ));
    };

    if !isis_data.lsp_id.is_pseudonode() {
        eprintln!("[resolve_network_prefix] not a pseudonode LSP id");
        return Err(ProtocolTopologyError::Semantic(
            "Non-IS-IS pseudonode provided to resolve_network_prefix".to_string(),
        ));
    }

    // Optional diagnostics limits
    let max_router_consider = env::var("ISIS_RESOLVE_MAX_ROUTERS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok());
    if let Some(m) = max_router_consider {
        println!("[resolve_network_prefix] limiting router scan to {}", m);
    }

    // Step 1: try to find and check DIS (diagnostic only; no single-prefix shortcut)
    let dis_data: Option<IsIsData> =
        find_dis_router(&isis_data.lsp_id, router_nodes).and_then(|router| {
            if let Some(ProtocolData::IsIs(data)) = router.protocol_data {
                Some(data)
            } else {
                None
            }
        });

    if let Some(dis_data) = &dis_data {
        println!("[resolve_network_prefix] found candidate DIS data");
        if let Some(Tlv::ExtendedIpReachability(ext_ip_reach)) = dis_data
            .tlvs
            .iter()
            .find(|t| matches!(t, Tlv::ExtendedIpReachability(_)))
        {
            println!(
                "[resolve_network_prefix] DIS has ExtendedIpReachability with {} neighbors",
                ext_ip_reach.neighbors.len()
            );
            if ext_ip_reach.neighbors.len() == 1 {
                println!(
                    "[resolve_network_prefix] DIS ext-ip-reach has 1 entry; not using DIS-only shortcut"
                );
            }
        }
    } else {
        println!("[resolve_network_prefix] no DIS candidate found");
    }

    // Step 2: find all routers connected to our network, if they all advertise the same prefix
    // we can be reasonably sure that it's ours
    if let Some(Tlv::ExtendedReachability(ext_reach)) = isis_data
        .tlvs
        .iter()
        .find(|t| matches!(t, Tlv::ExtendedReachability(_)))
    {
        println!(
            "[resolve_network_prefix] ExtendedReach found with {} neighbors",
            ext_reach.neighbors.len()
        );

        let neighbor_lsp_ids: Vec<LspId> = ext_reach
            .neighbors
            .iter()
            .map(|n| LspId::new_from(&n.neighbor_id, n.pseudonode_id, 0))
            .collect();

        let neighbor_isis_data: Vec<&IsIsData> = router_nodes
            .iter()
            .cloned()
            .take(max_router_consider.unwrap_or(usize::MAX))
            .filter_map(|node| {
                if let NodeInfo::Router(router) = &node.info {
                    if let Some(ProtocolData::IsIs(data)) = &router.protocol_data {
                        return Some(data);
                    }
                }
                None
            })
            // Enforce level match to avoid mixing L1/L2 data
            .filter(|data| data.is_level == isis_data.is_level)
            .filter(|data| neighbor_lsp_ids.contains(&data.lsp_id))
            .collect();

        println!(
            "[resolve_network_prefix] collected {} neighbor IS-IS data entries",
            neighbor_isis_data.len()
        );

        // If same-level neighbors are insufficient, try a cross-level fallback:
        // gather neighbor IS-IS data ignoring level, and attempt intersection there.
        if neighbor_isis_data.len() < 2 {
            println!(
                "[resolve_network_prefix] insufficient same-level neighbors ({}); trying cross-level fallback",
                neighbor_isis_data.len()
            );

            // Build cross-level candidate set: any level, but still restricted to neighbor_lsp_ids.
            let neighbor_isis_data_any_level: Vec<&IsIsData> = router_nodes
                .iter()
                .cloned()
                .take(max_router_consider.unwrap_or(usize::MAX))
                .filter_map(|node| {
                    if let NodeInfo::Router(router) = &node.info {
                        if let Some(ProtocolData::IsIs(data)) = &router.protocol_data {
                            return Some(data);
                        }
                    }
                    None
                })
                .filter(|data| neighbor_lsp_ids.contains(&data.lsp_id))
                .collect();

            // If DIS data exists and is part of the neighbor set but missing, include it.
            let mut neighbor_isis_data_fallback: Vec<&IsIsData> = neighbor_isis_data_any_level;
            if let Some(dis) = dis_data.as_ref() {
                if neighbor_lsp_ids.contains(&dis.lsp_id) {
                    // ensure uniqueness by lsp_id
                    let has_dis = neighbor_isis_data_fallback
                        .iter()
                        .any(|d| d.lsp_id == dis.lsp_id);
                    if !has_dis {
                        neighbor_isis_data_fallback.push(dis);
                    }
                }
            }

            println!(
                "[resolve_network_prefix] cross-level candidates: {}",
                neighbor_isis_data_fallback.len()
            );

            if neighbor_isis_data_fallback.len() >= 2 {
                let neighbor_ext_ip_reaches: Vec<_> = neighbor_isis_data_fallback
                    .iter()
                    .filter_map(|data| {
                        if let Some(Tlv::ExtendedIpReachability(reach)) = data
                            .tlvs
                            .iter()
                            .find(|t| matches!(t, Tlv::ExtendedIpReachability(_)))
                        {
                            Some(reach)
                        } else {
                            None
                        }
                    })
                    .collect();

                println!(
                    "[resolve_network_prefix] cross-level ext-ip-reach TLVs: {}",
                    neighbor_ext_ip_reaches.len()
                );

                if neighbor_ext_ip_reaches.len() >= 2 {
                    // Compute intersection across TLVs
                    let mut iter = neighbor_ext_ip_reaches.iter();
                    if let Some(first) = iter.next() {
                        let mut prefix_set: HashSet<&IpNetwork> =
                            first.neighbors.iter().map(|n| &n.prefix).collect();
                        for reach in iter {
                            let new_set: HashSet<&IpNetwork> =
                                reach.neighbors.iter().map(|n| &n.prefix).collect();
                            prefix_set = prefix_set.intersection(&new_set).copied().collect();
                            if prefix_set.is_empty() {
                                break;
                            }
                        }
                        if !prefix_set.is_empty() {
                            // Choose best candidate: longest prefix length, then lexicographically smallest.
                            let mut best: Option<IpNetwork> = None;
                            for p in prefix_set.into_iter() {
                                match &best {
                                    None => best = Some(p.clone()),
                                    Some(curr) => {
                                        let p_len = p.prefix();
                                        let c_len = curr.prefix();
                                        if p_len > c_len
                                            || (p_len == c_len && p.to_string() < curr.to_string())
                                        {
                                            best = Some(p.clone());
                                        }
                                    }
                                }
                            }
                            if let Some(prefix) = best {
                                println!(
                                    "[resolve_network_prefix] cross-level common prefix: {}",
                                    prefix
                                );
                                return Ok(prefix);
                            }
                        }
                    }
                }
            }

            println!(
                "[resolve_network_prefix] cross-level fallback failed; leaving prefix unresolved"
            );
            return Err(ProtocolTopologyError::Semantic(
                "Couldn't resolve network's prefix (insufficient corroboration)".to_string(),
            ));
        }

        let neighbor_ext_ip_reaches: Vec<_> = neighbor_isis_data
            .iter()
            .filter_map(|data| {
                if let Some(Tlv::ExtendedIpReachability(reach)) = data
                    .tlvs
                    .iter()
                    .find(|t| matches!(t, Tlv::ExtendedIpReachability(_)))
                {
                    Some(reach)
                } else {
                    None
                }
            })
            .collect();

        println!(
            "[resolve_network_prefix] collected {} neighbor ExtendedIpReach TLVs",
            neighbor_ext_ip_reaches.len()
        );

        let common_prefix = find_common_prefix(&neighbor_ext_ip_reaches);
        if let Some(prefix) = common_prefix {
            println!(
                "[resolve_network_prefix] found common prefix among neighbors: {}",
                prefix
            );
            return Ok(prefix);
        } else {
            println!("[resolve_network_prefix] no common prefix among neighbors");
        }

        fn find_common_prefix(reaches: &[&ExtendedIpReachabilityTlv]) -> Option<IpNetwork> {
            let mut iter = reaches.iter();
            let first = iter.next()?;
            let mut prefix_set: HashSet<&IpNetwork> =
                first.neighbors.iter().map(|n| &n.prefix).collect();
            if prefix_set.is_empty() {
                return None;
            }

            for reach in iter {
                let new_prefix_set: HashSet<&IpNetwork> =
                    reach.neighbors.iter().map(|n| &n.prefix).collect();
                prefix_set = prefix_set.intersection(&new_prefix_set).copied().collect();
                if prefix_set.is_empty() {
                    return None;
                }
            }

            // Choose the best candidate: longest prefix length, then lexicographically smallest.
            let mut best: Option<IpNetwork> = None;
            for p in prefix_set.into_iter() {
                match &best {
                    None => best = Some(p.clone()),
                    Some(curr) => {
                        let p_len = p.prefix();
                        let c_len = curr.prefix();
                        if p_len > c_len || (p_len == c_len && p.to_string() < curr.to_string()) {
                            best = Some(p.clone());
                        }
                    }
                }
            }
            best
        }
    }

    // If everything above failed, return error.
    eprintln!("[resolve_network_prefix] could not determine prefix for network node");
    Err(ProtocolTopologyError::Semantic(
        "Couldn't resolve network's prefix".to_string(),
    ))
}

mod tests {
    #[allow(unused)]
    use super::*;
    #[allow(unused)]
    use crate::parsers::isis_parser::frr_json_lsp::JsonLsp;
    #[allow(unused)]
    use serde_json::json;

    #[test]
    fn test_lsp_to_router() {
        let json = json!(
            {
              "lsp":{
                "id":"r1.00-00",
                "own":"*",
                "ownLSP":true
              },
              "pduLen":101,
              "seqNumber":"0x00000002",
              "chksum":"0xb9a3",
              "holdtime":1115,
              "attPOl":"0/0/0",
              "supportedProtocols":{
                "0":"IPv4"
              },
              "areaAddr":"49.0001",
              "hostname":"r1",
              "teRouterId":"172.21.123.11",
              "routerCapability":{
                "id":"172.21.123.11",
                "flagD":false,
                "flagS":false
              },
              "segmentRoutingAlgorithm":{
                "0":"SPF"
              },
              "extReach":[
                {
                  "mtId":"Extended",
                  "id":"0000.0000.0001.64",
                  "metric":10
                },
                {
                  "mtId":"Extended",
                  "id":"0000.0000.0001.5a",
                  "metric":10
                }
              ],
              "ipv4":"172.21.123.11",
              "extIpReach":[
                {
                  "mtId":"Extended",
                  "ipReach":"172.21.123.0/24",
                  "ipReachMetric":10,
                  "down":false
                },
                {
                  "mtId":"Extended",
                  "ipReach":"172.21.14.0/24",
                  "ipReachMetric":10,
                  "down":false
                }
              ]
            }
        );

        let json_lsp: JsonLsp = serde_json::from_value(json).unwrap();
        let map_input = include_str!("../../../test_data/isis_hostname_map_input.txt");

        let hostname_map = HostnameMap::build_map_from_lines(map_input.lines());

        let proto = JsonIsisProtocol { hostname_map };
        let lsp = json_lsp.try_into_lsp(1, &proto.hostname_map).unwrap();
        let parsed = proto.item_to_node(lsp).unwrap();

        assert!(parsed.is_some());
        let parsed = parsed.unwrap();
        assert!(matches!(parsed.info, NodeInfo::Router(_)));

        println!("Parsed ISIS node: {:#?}", parsed);
    }

    #[test]
    fn test_lsp_to_network() {
        let json = json!(
            {
              "lsp":{
                "id":"r1.5a-00",
                "own":"*",
                "ownLSP":true
              },
              "pduLen":51,
              "seqNumber":"0x00000001",
              "chksum":"0x462b",
              "holdtime":1058,
              "attPOl":"0/0/0",
              "extReach":[
                {
                  "mtId":"Extended",
                  "id":"0000.0000.0001.00",
                  "metric":0
                },
                {
                  "mtId":"Extended",
                  "id":"0000.0000.0004.00",
                  "metric":0
                }
              ]
            }
        );

        let json_lsp: JsonLsp = serde_json::from_value(json).unwrap();
        let map_input = include_str!("../../../test_data/isis_hostname_map_input.txt");

        let hostname_map = HostnameMap::build_map_from_lines(map_input.lines());

        let proto = JsonIsisProtocol { hostname_map };
        let lsp = json_lsp.try_into_lsp(1, &proto.hostname_map).unwrap();
        let parsed = proto.item_to_node(lsp).unwrap();

        assert!(parsed.is_some());
        let parsed = parsed.unwrap();

        assert!(matches!(parsed.info, NodeInfo::Network(_)));

        println!("Parsed ISIS network: {:#?}", parsed);
    }
}
