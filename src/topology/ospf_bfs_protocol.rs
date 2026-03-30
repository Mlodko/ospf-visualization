use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::{IpAddr, Ipv4Addr},
};

use ipnetwork::IpNetwork;
use ospf_parser::{OspfLinkStateAdvertisement, OspfRouterLinkType};

use crate::{
    network::{
        node::{Network, Node, NodeInfo, OspfData, OspfPayload, OspfVirtualLink, PerAreaRouterFacet, ProtocolData},
        router::{Router, RouterId},
    },
    parsers::ospf_parser::{
        lsa::{LsaError, OspfLsdbEntry},
        source::OspfRawRow,
    },
    topology::protocol::{ProtocolParseError, ProtocolTopologyError, RoutingProtocol},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum LsaType {
    Router,
    Network,
    Summary,
    External,
    Other,
}

impl From<&OspfLinkStateAdvertisement> for LsaType {
    fn from(value: &OspfLinkStateAdvertisement) -> Self {
        match value {
            OspfLinkStateAdvertisement::RouterLinks(_) => LsaType::Router,
            OspfLinkStateAdvertisement::NetworkLinks(_) => LsaType::Network,
            OspfLinkStateAdvertisement::SummaryLinkIpNetwork(_) => LsaType::Summary,
            OspfLinkStateAdvertisement::SummaryLinkAsbr(_) => LsaType::External,
            _ => LsaType::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LsaKey {
    lsa_type: LsaType,
    ls_id: Ipv4Addr,
}

impl LsaKey {
    pub fn new(lsa_type: LsaType, ls_id: Ipv4Addr) -> Self {
        LsaKey { lsa_type, ls_id }
    }
}

impl TryFrom<&Node> for LsaKey {
    type Error = String;

    fn try_from(value: &Node) -> Result<Self, Self::Error> {
        match &value.info {
            NodeInfo::Router(router) => {
                if let Some(ProtocolData::Ospf(data)) = &router.protocol_data {
                    let lsa_type = LsaType::from(data.base_advertisement.advertisement());
                    return Ok(LsaKey {
                        lsa_type,
                        ls_id: data.link_state_id,
                    });
                }
                Err("Invalid OSPF data".to_string())
            }
            NodeInfo::Network(net) => {
                if let Some(ProtocolData::Ospf(data)) = &net.protocol_data {
                    let lsa_type = LsaType::from(data.base_advertisement.advertisement());
                    return Ok(LsaKey {
                        lsa_type,
                        ls_id: data.link_state_id,
                    });
                }
                Err("Invalid OSPF data".to_string())
            }
        }
    }
}

pub struct OspfBfsProtocol {
    source_id: RouterId,
}

impl OspfBfsProtocol {
    pub fn new(source_id: RouterId) -> Self {
        OspfBfsProtocol { source_id }
    }

    pub fn source_id(&self) -> &RouterId {
        &self.source_id
    }

    pub fn set_source_id(&mut self, source_id: RouterId) {
        self.source_id = source_id;
    }
}

impl RoutingProtocol for OspfBfsProtocol {
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
    ) -> Result<Option<crate::network::node::Node>, super::protocol::ProtocolTopologyError> {
        match item.try_into() as Result<Node, LsaError> {
            Ok(node) => Ok(Some(node)),
            Err(LsaError::InvalidLsaType) => Ok(None), // Skip unsupported LSA types
            Err(e) => Err(ProtocolTopologyError::Conversion(format!("{:?}", e))),
        }
    }

    fn post_process(
        &self,
        nodes: &mut Vec<crate::network::node::Node>,
    ) -> Result<(), super::protocol::ProtocolTopologyError> {
        let mut nodes_map: HashMap<LsaKey, Node> = HashMap::new();
        let mut summaries_map: HashMap<Ipv4Addr, Vec<Node>> = HashMap::new();
        
        self.consolidate_routers(nodes);
        self.consolidate_networks(nodes);

        for node in nodes.drain(..) {
            match &node.info {
                NodeInfo::Router(router) => {
                    if let Some(ProtocolData::Ospf(data)) = &router.protocol_data {
                        use OspfLinkStateAdvertisement::*;
                        match data.base_advertisement.advertisement() {
                            RouterLinks(_) => {
                                let key = LsaKey::new(LsaType::Router, data.link_state_id);
                                nodes_map.insert(key, node);
                            }
                            SummaryLinkAsbr(_) => {
                                summaries_map
                                    .entry(data.advertising_router)
                                    .or_insert(Vec::new())
                                    .push(node);
                            }
                            _ => {}
                        }
                    }
                }
                NodeInfo::Network(net) => {
                    if let Some(ProtocolData::Ospf(data)) = &net.protocol_data {
                        use OspfLinkStateAdvertisement::*;
                        match data.base_advertisement.advertisement() {
                            NetworkLinks(_) => {
                                let key = LsaKey::new(LsaType::Network, data.link_state_id);
                                nodes_map.insert(key, node);
                            }
                            SummaryLinkIpNetwork(_) => {
                                summaries_map
                                    .entry(data.advertising_router)
                                    .or_insert(Vec::new())
                                    .push(node);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        let mut visited: HashSet<LsaKey> = HashSet::new();

        // Find source node, inefficient but necessary for source in current impl
        let source_uuid = self.source_id().to_uuidv5();
        let source_node = nodes_map
            .iter()
            .find(|(_, node)| node.id == source_uuid)
            .map(|(_, node)| node.clone())
            .ok_or(ProtocolTopologyError::Semantic(
                "Missing source node".to_string(),
            ))?;
        let source_key = LsaKey::try_from(&source_node).unwrap();
        visited.insert(source_key);

        // Initialize BFS queue with source node
        let mut queue = VecDeque::new();
        queue.push_back(source_node);
        
        fn debug_print_link(link: &ospf_parser::OspfRouterLink) {
            println!("Current link: {{ Link Data: {}, Link Id: {}, Link Type: {}, TOS 0 metric: {}}}",
                link.link_data(), link.link_id(), link.link_type, link.tos_0_metric)
        }
        
        while let Some(current_node) = queue.pop_front() {
            match &current_node.info {
                NodeInfo::Router(router) => {
                    println!("Current router: {}", &router.id);
                    if let Some(ProtocolData::Ospf(data)) = &router.protocol_data {
                        let mut all_advertisements = vec![data.base_advertisement.clone()];
                        all_advertisements.extend(data.merged_advertisements.iter().cloned());
                        for adv in all_advertisements {
                            if let OspfLinkStateAdvertisement::RouterLinks(adv) = adv.advertisement()
                            {
                                for link in adv.links.iter() {
                                    debug_print_link(link);
                                    match link.link_type {
                                        OspfRouterLinkType::PointToPoint => {
                                            let neighbor_router_id = link.link_id();
                                            let neighbor_key =
                                                LsaKey::new(LsaType::Router, neighbor_router_id.clone());
                                            if visited.contains(&neighbor_key) {
                                                println!("Skipping already visited neighbor {}", &neighbor_router_id);
                                                continue;
                                            }
                                            let neighbor_node = nodes_map.get(&neighbor_key).ok_or(
                                                ProtocolTopologyError::Semantic(format!(
                                                    "Missing neighbor node {}",
                                                    neighbor_router_id
                                                )),
                                            )?;
    
                                            if let NodeInfo::Router(neighbor_router) = &neighbor_node.info {
                                                if let Some(ProtocolData::Ospf(neighbor_data)) =
                                                    &neighbor_router.protocol_data
                                                {
                                                    if let OspfLinkStateAdvertisement::RouterLinks(
                                                        neighbor_adv,
                                                    ) = neighbor_data.base_advertisement.advertisement()
                                                    {
                                                        let neighbor_link_ids: Vec<_> = neighbor_adv
                                                            .links
                                                            .iter()
                                                            .map(|link| link.link_id())
                                                            .collect();
    
                                                        if !neighbor_link_ids.contains(&data.link_state_id) {
                                                            println!("Skipping, neighbor {} doesn't have a link back to {}", neighbor_router_id, &router.id);
                                                            continue;
                                                        }
                                                    }
                                                }
                                            }
                                            
                                            println!("Adding neighbor {} to queue", neighbor_router_id);
                                            visited.insert(neighbor_key);
                                            queue.push_back(neighbor_node.clone());
                                        }
                                        OspfRouterLinkType::Stub => {
                                            let net_addr = link.link_id();
                                            let net_mask = link.link_data();
                                            let net_key = LsaKey::new(LsaType::Network, net_addr);
                                            if visited.contains(&net_key) {
                                                println!("Skipping already visited network {}", net_addr);
                                                continue;
                                            }
                                            let network = Network {
                                                ip_address: IpNetwork::with_netmask(
                                                    net_addr.into(),
                                                    net_mask.into(),
                                                )
                                                .unwrap(),
                                                protocol_data: router.protocol_data.clone(),
                                                attached_routers: vec![router.id.clone()],
                                            };
                                            let node_info = NodeInfo::Network(network.clone());
                                            let node = Node::new(node_info, None);
                                            println!("Created node for stub network {} and added to queue", &network.ip_address);
                                            visited.insert(net_key);
                                            nodes.push(node);
                                        }
                                        OspfRouterLinkType::Transit => {
                                            let dr_if_addr = link.link_id();
                                            let net_key = LsaKey::new(LsaType::Network, dr_if_addr.clone());
                                            if visited.contains(&net_key) {
                                                println!("Skipping already visited network node of DR: {}", dr_if_addr);
                                                continue;
                                            }
                                            let net_node = nodes_map.get(&net_key).ok_or_else( || {
                                                //dbg!(dr_if_addr);
                                                //dbg!(nodes_map.iter().filter(|(k, v)| matches!(k.lsa_type, LsaType::Network)));
                                                ProtocolTopologyError::Semantic(format!(
                                                    "Missing network node {}", dr_if_addr
                                                ))
                                            },
                                            )?;
                                            let net_addr = &net_node.info.network().unwrap().ip_address;
                                            println!("Adding network {} to queue", net_addr);
                                            visited.insert(net_key);
                                            queue.push_back(net_node.clone());
                                        }
                                        OspfRouterLinkType::Virtual => {
                                            // Same as P2P
                                            let neighbor_router_id = link.link_id();
                                            let neighbor_key =
                                                LsaKey::new(LsaType::Router, neighbor_router_id);
                                            if visited.contains(&neighbor_key) {
                                                println!("Virtual neighbor {} already visited", neighbor_router_id);
                                                continue;
                                            }
                                            let neighbor_node = nodes_map.get(&neighbor_key).ok_or(
                                                ProtocolTopologyError::Semantic(format!(
                                                    "Missing neighbor node {}",
                                                    neighbor_router_id
                                                )),
                                            )?;
                                            println!("Adding virtual neighbor {} to queue", neighbor_router_id);
                                            visited.insert(neighbor_key);
                                            queue.push_back(neighbor_node.clone());
                                        }
                                        _ => {
                                            println!("Unsupported link type")
                                        }
                                    }
                                }
                            }
                        }

                        
                        if let RouterId::Ipv4(advertising_id) = &router.id {
                            println!("Entered Summary Branch");
                            println!("Advertising Router ID: {}", advertising_id);
                            if let Some(summaries) = summaries_map.get(advertising_id) {
                                for summary in summaries {
                                    match &summary.info {
                                        NodeInfo::Network(net) => {
                                            //println!("Current Summary Network:\n{:#?}", net);
                                            if let IpAddr::V4(ip) = net.ip_address.ip() {
                                                println!("Summary Network IP: {}", ip);
                                                let key = LsaKey::new(LsaType::Network, ip);
                                                if visited.contains(&key) {
                                                    println!("Network {} already visited", ip);
                                                    continue;
                                                }
                                                println!("Adding Network {} to queue", ip);
                                                visited.insert(key);
                                                queue.push_back(summary.clone());
                                            }
                                        }

                                        NodeInfo::Router(router) => {
                                            todo!() // No ASBRs for now
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                NodeInfo::Network(net) => {
                    println!("Current network: {}", &net.ip_address);
                    for router_id in &net.attached_routers {
                        if let RouterId::Ipv4(ip) = router_id {
                            let key = LsaKey::new(LsaType::Router, *ip);
                            if visited.contains(&key) {
                                println!("Attached router {} already visited", ip);
                                continue;
                            }
                            let neighbor_node =
                                nodes_map.get(&key).ok_or(ProtocolTopologyError::Semantic(
                                    format!("Missing neighbor node {}", ip),
                                ))?;
                            if let Some(ProtocolData::Ospf(data)) = &net.protocol_data {
                                if let NodeInfo::Router(n_router) = &neighbor_node.info {
                                    if let Some(ProtocolData::Ospf(n_data)) =
                                        &n_router.protocol_data
                                    {
                                        if let OspfLinkStateAdvertisement::RouterLinks(n_adv) =
                                            n_data.base_advertisement.advertisement()
                                        {
                                            if !n_adv
                                                .links
                                                .iter()
                                                .map(|link| link.link_id())
                                                .any(|link_id| link_id == data.link_state_id)
                                            {
                                                println!("Skipping, neighbor router {} does not have a back link to {}", ip, &net.ip_address);
                                                continue;
                                            }
                                        }
                                    }
                                }
                            }
                            println!("Adding neighbor {} to queue", ip);
                            visited.insert(key);
                            queue.push_back(neighbor_node.clone());
                        }
                    }
                }
            }
            nodes.push(current_node);
        }

        Ok(())
    }
}

impl OspfBfsProtocol {
    /// Consolidates multiple Router Nodes representing the same router into a single node.
    /// This is needed since e.g. ABRs generate multiple type 1 LSAs that are turned into Router Nodes.
    fn consolidate_routers(&self, nodes: &mut Vec<crate::network::node::Node>) {
        let mut result: Vec<Node> = Vec::new();
        let mut router_by_id_map: HashMap<RouterId, Vec<Node>> = HashMap::new();
        
        for node in nodes.drain(..) {
            match &node.info {
                NodeInfo::Router(r) => {
                    if let Some(existing_nodes) = router_by_id_map.get_mut(&r.id) {
                        existing_nodes.push(node);
                    } else {
                        router_by_id_map.insert(r.id.clone(), vec![node]);
                    }
                }
                NodeInfo::Network(_) => result.push(node)
            }
        }
        
        for (id, mut nodes) in router_by_id_map {
            /*
             * Some rules for consolidation:
             * 1. [x] Prefer area_id == 0.0.0.0, this goes for the UUID as well. You can read it from OspfData.
             * 2. [x] RouterId stays the same (it's the key after all)
             * 3. [x] Interface lists are unioned (without dupes).
             * 4. [x] For flags like is_abr, is_asbr, etc. Use logical OR on all facets.
             * 5. [x] Store link counts in PerAreaRouterFacets
             * 6. [x] Union the virtual links
             * 7. [x] If no backbone in area_ids then prefer the one with the most p2p + transit links.
             * 8. [x] Merge advertisements into base node's merged_advertisements
             */
             
            // Rule #5 setup
            let per_area_facets: Vec<PerAreaRouterFacet> = nodes.iter().map(|node| {
                let router = node.info.router().unwrap();
                let ospf_data = router.protocol_data().unwrap().ospf().unwrap();
                let area_id = ospf_data.area_id;
                
                let pd = ospf_data.payload.router().unwrap();
                PerAreaRouterFacet {
                    area_id,
                    p2p_link_count: pd.p2p_link_count,
                    transit_link_count: pd.transit_link_count,
                    virtual_link_count: pd.virtual_link_count,
                    stub_link_count: pd.stub_link_count
                }
            }).collect();
            
            let mut base_node = {
                if let Some(idx) = nodes.iter().enumerate()
                    // Rule #1
                    .find(|(_, node)| {
                        if let NodeInfo::Router(r) = &node.info {
                            if let Some(ProtocolData::Ospf(data)) = &r.protocol_data {
                                return data.area_id == Ipv4Addr::new(0, 0, 0, 0);
                            }
                        }
                        false
                    })
                    .map(|(idx, _)| idx)
                {
                    nodes.swap_remove(idx)
                } else {
                    // Rule #7
                    let node_idx = nodes.iter().enumerate()
                        .max_by_key(|(_, node)| {
                            node.info.router()
                                .map(|r| r.protocol_data())
                                .flatten()
                                .map(|pd| pd.ospf())
                                .flatten()
                                .map(|ospf| ospf.payload.router())
                                .flatten()
                                .map(|rp| rp.p2p_link_count + rp.transit_link_count)
                                .unwrap_or(0)
                        })
                        .map_or(0, |(idx, _)| idx);
                    nodes.swap_remove(node_idx)
                }
            };
            
            // Rule #8
            let other_advertisements = nodes.iter()
                .map(|node| node.info.router().unwrap().protocol_data().unwrap().ospf().unwrap().base_advertisement.clone());
            base_node.info.router_mut().unwrap().protocol_data_mut().unwrap().ospf_mut().unwrap().merged_advertisements.extend(other_advertisements);
            
            // Rule #5
            base_node.info.router_mut().unwrap()
                .protocol_data_mut().unwrap()
                .ospf_mut().unwrap()
                .payload.router_mut().unwrap()
                .per_area_facets = per_area_facets;
             
             // Rule #3
            {
                 let base_interfaces = &mut base_node.info.router_mut().unwrap().interfaces;
                 
                 let mut interface_set: HashSet<IpAddr> = base_interfaces.iter().cloned().collect();
                 nodes.iter().for_each(|node| {
                     node.info.router()
                         .map(|r| r.interfaces.iter())
                         .unwrap_or_default()
                         .for_each(|interface| {
                             interface_set.insert(*interface);
                         })
                 });
                 
                 *base_interfaces = interface_set.into_iter().collect();
            }
            // Rule #4 and #6
            {
                let base_payload = &mut base_node.info.router_mut().unwrap()
                    .protocol_data_mut().unwrap().ospf_mut().unwrap()
                    .payload.router_mut().unwrap();
                
                let other_payloads = nodes.iter().map(|node| {
                    node.info.router().unwrap()
                        .protocol_data().unwrap()
                        .ospf().unwrap()
                        .payload.router()
                })
                .filter_map(|p| p);
                
                let mut virtual_link_set: HashSet<OspfVirtualLink> = base_payload.virtual_links.iter().cloned().collect();
                
                other_payloads.for_each(|pd| {
                    base_payload.is_abr |= pd.is_abr;
                    base_payload.is_asbr |= pd.is_asbr;
                    base_payload.is_nssa_capable |= pd.is_nssa_capable;
                    base_payload.is_virtual_link_endpoint |= pd.is_virtual_link_endpoint;
                    virtual_link_set.extend(pd.virtual_links.iter().cloned())
                });
                
                base_payload.virtual_links = virtual_link_set.into_iter().collect();
            }
            
            result.push(base_node);
        }
        
        nodes.extend(result);
    }
    
    fn consolidate_networks(&self, nodes: &mut Vec<Node>) {
        let mut result = Vec::new();
        let mut networks_by_prefix_map: HashMap<IpNetwork, Vec<Node>> = HashMap::new();
        
        for node in nodes.drain(..) {
            match &node.info {
                NodeInfo::Router(_) => {
                    result.push(node);
                }
                NodeInfo::Network(network) => {
                    let prefix = network.ip_address;
                    if let Some(networks) = networks_by_prefix_map.get_mut(&prefix) {
                        networks.push(node);
                    } else {
                        networks_by_prefix_map.insert(prefix, vec![node]);
                    }
                }
            }
        }
        
        for (prefix, mut nodes) in networks_by_prefix_map {
            println!("Processing network prefix: {}", prefix);
            // Prefer concrete network over summary
            let mut base_node = {
                if let Some(idx) = nodes.iter().enumerate()
                    .find(|(_, node)| matches!(
                        node.info.network().unwrap().protocol_data().unwrap().ospf().unwrap().payload,
                        OspfPayload::Network(_)
                    )).map(|(idx, _)| idx) {
                        nodes.swap_remove(idx)
                    } else {
                        nodes.pop().unwrap()
                    }
            };
            println!("Base node: {:#?}", base_node);
            
            // Merge attached routers
            {
                let mut attached_router_set: HashSet<RouterId> = base_node.info.network().unwrap().attached_routers.iter().cloned().collect();
                nodes.iter().for_each(|node| {
                    let attached = node.info.network().unwrap().attached_routers.iter().cloned();
                    attached_router_set.extend(attached);
                });
                base_node.info.network_mut().unwrap().attached_routers = attached_router_set.clone().into_iter().collect();
                println!("Merged attached routers: {:#?}", attached_router_set);
            }
            
            // Merge advertisements
            {
                let other_advertisements = nodes.iter()
                    .map(|node| node.info.network().unwrap().protocol_data().unwrap().ospf().unwrap().base_advertisement.clone());
                base_node.info.network_mut().unwrap().protocol_data_mut().unwrap().ospf_mut().unwrap().merged_advertisements.extend(other_advertisements);
            }
            
            // Merge normal payloads
            {
                let mut base_payload = &mut base_node.info.network_mut().unwrap().protocol_data_mut().unwrap()
                    .ospf_mut().unwrap().payload;
                println!("Base payload: {:#?}", &base_payload);
                
                let other_payloads = nodes.iter()
                    .map(|node| &node.info.network().unwrap().protocol_data().unwrap().ospf().unwrap().payload);
                
                /*
                    Now there will be 4 different scenarios for each (base, other) pair:
                    
                    Legend:
                    N - Concrete Network
                    S - Summary Network
                    
                    Other\Base | N | S |
                    --------------------
                            N | 1 | 2 |
                            S | 3 | 4 |
                    
                    1. N <- N 
                        Merge 2 concrete networks as equals.
                    2. S <- N
                        Should never happen. If an N exists, it will be selected as base.
                    3. N <- S
                        Add the summary to NetworkPayload::summaries
                    4. S <- S
                        Merge 2 summary networks as equals.
                */
                
                for other_payload in other_payloads {
                    match (&mut base_payload, other_payload) {
                        (OspfPayload::Network(base), OspfPayload::Network(other)) => {
                            println!("Merging two networks, other:\n{:#?}", &other);
                            // Merge other's summaries and externals into base
                            let summary_set: HashSet<_> = base.summaries.iter().cloned().collect();
                            for summary in &other.summaries {
                                if !summary_set.contains(summary) {
                                    base.summaries.push(summary.clone());
                                }
                            }
                            
                            let external_set: HashSet<_> = base.externals.iter().cloned().collect();
                            for external in &other.externals {
                                if !external_set.contains(external) {
                                    base.externals.push(external.clone());
                                }
                            }
                        }
                        (OspfPayload::SummaryNetwork(base), OspfPayload::Network(other)) => {
                            unreachable!("If the base payload is a summary network, the other payload should be a summary network as well.")
                        }
                        (OspfPayload::Network(base), OspfPayload::SummaryNetwork(other)) => {
                            println!("Merging summary into network, other\n:{:#?}", &other);
                            let summary_set: HashSet<_> = base.summaries.iter().cloned().collect();
                            if !summary_set.contains(&other) {
                                println!("Summary not found in network, adding");
                                base.summaries.push(other.clone());
                            } else {
                                println!("Summary already exists in network");
                            }
                        }
                        (OspfPayload::SummaryNetwork(base), OspfPayload::SummaryNetwork(other)) => {
                            println!("Two summary networks, selecting the one with lower metric, other\n:{:#?}", &other);
                            if other.metric < base.metric {
                                println!("Switching to other payload");
                                *base = other.clone();
                            }
                        }
                        _ => {
                            unreachable!("Invalid payload combination")
                        }
                    }
                }
            }
            
            println!("Merged node:\n{:#?}", &base_node);
            result.push(base_node);
        }
        *nodes = result;
    }
}
