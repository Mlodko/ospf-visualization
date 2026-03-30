use std::{collections::{HashSet, VecDeque}, net::Ipv4Addr};

use egui::ahash::{HashMap};
use ipnetwork::IpNetwork;
use uuid::Uuid;

use crate::{network::{node::{IsIsData, Network, Node, NodeInfo, ProtocolData}, router::{Router, RouterId}}, parsers::isis_parser::{core_lsp::{ExtendedIpReachabilityTlv, Lsp, LspError, LspId, SystemId, Tlv}, frr_json_lsp::JsonLspdb, hostname::HostnameMap}, topology::protocol::{ProtocolParseError, ProtocolTopologyError, RoutingProtocol}};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LspKey {
    system_id: SystemId,
    pseudonode_id: u8,
}

impl LspKey {
    fn new(system_id: SystemId, pseudonode_id: u8) -> Self {
        Self {
            system_id,
            pseudonode_id,
        }
    }
}

impl TryFrom<&LspId> for LspKey {
    type Error = LspError;

    fn try_from(value: &LspId) -> Result<Self, Self::Error> {
        Ok(Self {
            system_id: value.get_system_id()?,
            pseudonode_id: value.get_pseudonode_id(),
        })
    }
}

pub struct JsonIsisBfsProtocol {
    hostname_map: HostnameMap,
}

impl JsonIsisBfsProtocol {
    pub fn new(hostname_map: HostnameMap) -> Self {
        Self {
            hostname_map,
        }
    }
    
    fn lsp_to_router(&self, lsp: Lsp) -> Result<Router, ProtocolTopologyError> {
        let id = RouterId::IsIs(lsp.system_id.clone());
        let net_address = lsp.get_net_address();
        let protocol_data = ProtocolData::IsIs(IsIsData {
            lsp: lsp.clone(),
            is_level: lsp.is_level,
            lsp_id: lsp.lsp_id,
            tlvs: lsp.tlvs,
            net_address: net_address,
            owned: lsp.owned,
        });

        Ok(Router {
            id,
            interfaces: Vec::new(), // We leave this empty since IS-IS works at the link layer
            protocol_data: Some(protocol_data),
        })
    }

    fn lsp_to_network(&self, lsp: Lsp) -> Result<Network, ProtocolTopologyError> {
        let protocol_data = ProtocolData::IsIs(IsIsData {
            lsp: lsp.clone(),
            net_address: lsp.get_net_address(),
            is_level: lsp.is_level,
            lsp_id: lsp.lsp_id,
            tlvs: lsp.tlvs,
            owned: lsp.owned,
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

impl RoutingProtocol for JsonIsisBfsProtocol {
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
        let nodes_map: HashMap<LspKey, Node> = nodes.drain(..)
            .map(|node|  {
                let key = match &node.info {
                    NodeInfo::Router(router) => {
                        if let Some(ProtocolData::IsIs(data)) = &router.protocol_data {
                            LspKey::try_from(&data.lsp_id).unwrap()
                        } else {
                            unreachable!()
                        }
                    }
                    NodeInfo::Network(network) => {
                        if let Some(ProtocolData::IsIs(data)) = &network.protocol_data {
                            LspKey::try_from(&data.lsp_id).unwrap()
                        } else {
                            unreachable!()
                        }
                    }
                };
                (key, node)
            })
            .collect();
        
        let mut visited: HashSet<LspKey> = HashSet::new();
        let (source_key, source_node) = nodes_map.iter().find(|(_, node)| {
            if let NodeInfo::Router(router) = &node.info {
                if let Some(ProtocolData::IsIs(data)) = &router.protocol_data {
                    if let Some(owned) = data.owned {
                        return owned;
                    }
                }
            }
            false 
        })
        .map(|(key, node)| (key.clone(), node.clone()))
        .ok_or(ProtocolTopologyError::Semantic("Missing source node".to_string()))?;
        
        let mut queue = VecDeque::new();
        visited.insert(source_key);
        queue.push_back(source_node);
        
        while let Some(mut current_node) = queue.pop_front() {
            
            let mut net_to_replace: Option<Network> = None;
            
            // Query only extended IS/IP reachability for now (for simplicity)
            
            match &current_node.info {
                NodeInfo::Router(Router { protocol_data: Some(ProtocolData::IsIs(data)), .. }) => {
                    if let Some(Tlv::ExtendedReachability(tlv)) = data.tlvs.iter()
                        .find(|tlv| matches!(tlv, Tlv::ExtendedReachability(_))) {
                            let neighbor_keys = tlv.neighbors.iter()
                                .map(|neighbor| LspKey::new(neighbor.neighbor_id.clone(), neighbor.pseudonode_id));
                            for neighbor_key in neighbor_keys {
                                if visited.contains(&neighbor_key) {
                                    continue;
                                }
                                let neighbor_node = nodes_map.get(&neighbor_key).ok_or(ProtocolTopologyError::Semantic(format!("Missing neighbor node {}", neighbor_key.system_id)))?;
                                visited.insert(neighbor_key);
                                queue.push_back(neighbor_node.clone());
                            }
                        }
                }
                NodeInfo::Network(net) => {
                    println!("Processing network node");
                    // For networks we have to do a bit more work since the pseudonode doesn't hold the network's prefix
                    let mut net = net.clone();
                    let prefix_result = resolve_network_prefix(&current_node, &nodes_map.values().collect::<Vec<_>>());
                    match prefix_result {
                        Ok(prefix) => {
                            println!("Prefix resolved: {}", &prefix);
                            net.ip_address = prefix;
                            // Recompute node's UUID since the network's prefix has changed
                            current_node.id = Uuid::new_v5(&Uuid::NAMESPACE_OID, net.ip_address.to_string().as_bytes());
                        }
                        Err(e) => {
                            eprintln!(
                                "[JsonIsisProtocol::post_process] failed to resolve prefix for node due to {:?}",
                                e
                            );
                        }
                    }
                    
                    if let Some(ProtocolData::IsIs(IsIsData { is_level, lsp_id, net_address, tlvs, owned, .. })) = &net.protocol_data {
                        if let Some(Tlv::ExtendedReachability(tlv)) = tlvs.iter().find(|tlv| matches!(tlv, Tlv::ExtendedReachability(_))) {
                            let neighbor_keys: Vec<_> = tlv.neighbors.iter().map(|neighbor| LspKey::new(neighbor.neighbor_id.clone(), neighbor.pseudonode_id)).collect();
                            
                            let neighbor_system_ids: Vec<SystemId> = neighbor_keys.iter()
                                .filter_map(|key| {
                                    if let Some(neighbor_node @ Node { 
                                        info: NodeInfo::Router(Router { 
                                            protocol_data: Some(ProtocolData::IsIs(neighbor_data)), 
                                            .. 
                                        }), 
                                        .. 
                                    }) = nodes_map.get(&key) {
                                        // Check if the neighbor router has the network in its TLV
                                        let current_key = LspKey::try_from(lsp_id).unwrap();
                                        if let Some(Tlv::ExtendedReachability(neighbor_tlv)) = neighbor_data.tlvs.iter()
                                            .find(|tlv| matches!(tlv, Tlv::ExtendedReachability(_))) {
                                                if neighbor_tlv.neighbors.iter().any(|n| {
                                                    let n_key = LspKey::new(n.neighbor_id.clone(), n.pseudonode_id);
                                                    println!("Checking\nLeft: {:?}\n Right: {:?}", &n_key, current_key);
                                                    n_key == current_key
                                                }) {
                                                    if !visited.contains(&key) {
                                                        visited.insert(key.clone());
                                                        queue.push_back(neighbor_node.clone());
                                                    }
                                                    return Some(key.system_id.clone());
                                                } else {
                                                    println!("Neighbor router does not have the network in its TLV");
                                                }
                                            }
                                    }
                                    None
                                }).collect();
                            
                            net.attached_routers = neighbor_system_ids.into_iter()
                                .map(|sid| RouterId::IsIs(sid))
                                .collect();
                        }
                    }
                    
                    net_to_replace = Some(net);
                }
                _ => {
                    println!("[WARN] ISIS_BFS_post_process: Unknown node type\n{:?}", current_node);
                }
            }
            
            if let Some(net) = net_to_replace {
                current_node.info = NodeInfo::Network(net);
            }
            nodes.push(current_node);
        }
        
        Ok(())
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
    use crate::{network::node::NodeInfo, parsers::isis_parser::protocol::JsonIsisProtocol};
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

        let proto = JsonIsisBfsProtocol { hostname_map };
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

        let proto = JsonIsisBfsProtocol { hostname_map };
        let lsp = json_lsp.try_into_lsp(1, &proto.hostname_map).unwrap();
        let parsed = proto.item_to_node(lsp).unwrap();

        assert!(parsed.is_some());
        let parsed = parsed.unwrap();

        assert!(matches!(parsed.info, NodeInfo::Network(_)));

        println!("Parsed ISIS network: {:#?}", parsed);
    }
    
    #[test]
    fn test_key() {
        let key1 = LspKey::new(SystemId::new(&[0, 0, 0, 0, 0, 5]).unwrap(), 18);
        let key2 = LspKey::new(SystemId::new(&[0, 0, 0, 0, 0, 5]).unwrap(), 19);
        let key3 = LspKey::new(SystemId::new(&[0, 0, 0, 0, 0, 5]).unwrap(), 18);
        
        assert_eq!(key1, key3);
        assert_ne!(key1, key2);
        
    }
}