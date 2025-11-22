use crate::{
    network::node::{
        Node, NodeInfo, OspfData, OspfRouterPayload, PerAreaRouterFacet, ProtocolData,
    },
    parsers::ospf_parser::source::OspfRawRow,
};
use ipnetwork::IpNetwork;
use ospf_parser::{
    OspfLinkStateAdvertisement, OspfNetworkLinksAdvertisement, OspfRouterLinksAdvertisement,
};
use std::{
    net::{IpAddr, Ipv4Addr},
    sync::Arc,
};

use nom_derive::Parse;

use crate::network::{
    node::Network,
    router::{Router, RouterId},
};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum LsaError {
    InvalidLsaType,
    InvalidNetworkMask(Ipv4Addr),
    ProtocolNotImplemented,
    WrongSnmpTable,
    IncompleteData,
    WrongDataType,
}

#[derive(Debug)]
pub struct OspfLsdbEntry {
    area_id: Ipv4Addr,
    link_state_id: Ipv4Addr,
    router_id: Ipv4Addr,
    advertisement: Arc<OspfLinkStateAdvertisement>,
}

impl TryFrom<OspfRawRow> for OspfLsdbEntry {
    type Error = LsaError;

    fn try_from(row: OspfRawRow) -> Result<Self, Self::Error> {
        let advertisement: OspfLinkStateAdvertisement =
            OspfLinkStateAdvertisement::parse(&row.lsa_bytes)
                .map_err(|_| LsaError::WrongDataType)?
                .1;

        Ok(OspfLsdbEntry {
            area_id: row.area_id,
            link_state_id: row.link_state_id,
            router_id: row.router_id,
            advertisement: Arc::new(advertisement),
        })
    }
}

impl TryInto<Node> for OspfLsdbEntry {
    type Error = LsaError;

    fn try_into(self) -> Result<Node, Self::Error> {
        let node_info = match *self.advertisement {
            OspfLinkStateAdvertisement::RouterLinks(_) => {
                NodeInfo::Router(parse_lsa_type_1_to_router(&self)?)
            }
            OspfLinkStateAdvertisement::NetworkLinks(_) => {
                NodeInfo::Network(parse_lsa_type_2_to_network(&self)?)
            }
            OspfLinkStateAdvertisement::SummaryLinkIpNetwork(_) => {
                NodeInfo::Network(parse_lsa_type_3(&self)?)
            }
            _ => {
                println!("Unsupported advertisement type");
                return Err(LsaError::InvalidLsaType);
            }
        };

        Ok(Node::new(node_info, None))
    }
}

pub fn parse_lsa_type_1_to_router(lsa: &OspfLsdbEntry) -> Result<Router, LsaError> {
    let advertisement: &OspfRouterLinksAdvertisement =
        if let OspfLinkStateAdvertisement::RouterLinks(ad) = &*lsa.advertisement {
            ad
        } else {
            return Err(LsaError::InvalidLsaType);
        };

    let router_id = RouterId::Ipv4(lsa.router_id);
    let interfaces: Vec<IpAddr> = advertisement
        .links
        .iter()
        .map(|link| IpAddr::V4(link.link_data()))
        .collect();
    // Compute link counts and per-link metrics from Router-LSA links
    let mut p2p_link_count = 0usize;
    let mut transit_link_count = 0usize;
    let mut stub_link_count = 0usize;
    let mut link_metrics: std::collections::HashMap<Ipv4Addr, u16> =
        std::collections::HashMap::new();

    for link in &advertisement.links {
        // Use link_data as a stable IPv4 key (for p2p this is the local interface IP).
        // If you prefer LSID/neighbor RouterID, change to link.link_id() if exposed by the crate.
        link_metrics.insert(link.link_data(), link.tos_0_metric);
        match link.link_type {
            ospf_parser::OspfRouterLinkType::PointToPoint => p2p_link_count += 1,
            ospf_parser::OspfRouterLinkType::Transit => transit_link_count += 1,
            ospf_parser::OspfRouterLinkType::Stub => stub_link_count += 1,
            ospf_parser::OspfRouterLinkType::Virtual => {}
            _ => {}
        }
    }
    const ABR_BIT: u16 = 0b0000_0001_0000_0000;
    const ASBR_BIT: u16 = 0b0000_0010_0000_0000;
    const VIRTUAL_BIT: u16 = 0b0000_0100_0000_0000;

    let is_abr = advertisement.flags & ABR_BIT != 0;
    let is_asbr = advertisement.flags & ASBR_BIT != 0;
    let is_virtual_link_endpoint = advertisement.flags & VIRTUAL_BIT != 0;

    let payload = OspfRouterPayload {
        // If the parser exposes Router-LSA flags (ABR/ASBR/NSSA), wire them here.
        // Defaults keep behavior sane until flags are exposed.
        is_abr,
        is_asbr,
        is_virtual_link_endpoint,
        is_nssa_capable: false,
        p2p_link_count,
        transit_link_count,
        stub_link_count,
        link_metrics,
        per_area_facets: vec![PerAreaRouterFacet {
            area_id: lsa.area_id,
            p2p_link_count,
            transit_link_count,
            stub_link_count,
        }],
        virtual_links: vec![],
    };

    let checksum = Some(advertisement.header.ls_checksum);

    let ospf_data: OspfData = OspfData {
        area_id: lsa.area_id,
        advertisement: lsa.advertisement.clone(),
        link_state_id: lsa.link_state_id,
        advertising_router: lsa.router_id,
        checksum,
        payload: crate::network::node::OspfPayload::Router(payload),
    };

    let router = Router {
        id: router_id,
        interfaces,
        protocol_data: Some(ProtocolData::Ospf(ospf_data)),
    };

    Ok(router)
}

pub fn parse_lsa_type_2_to_network(lsa: &OspfLsdbEntry) -> Result<Network, LsaError> {
    let advertisement: &OspfNetworkLinksAdvertisement =
        if let OspfLinkStateAdvertisement::NetworkLinks(ad) = &*lsa.advertisement {
            ad
        } else {
            return Err(LsaError::InvalidLsaType);
        };
    let network_addr =
        Ipv4Addr::from_bits(lsa.link_state_id.to_bits() & advertisement.network_mask);
    let network = IpNetwork::with_netmask(
        IpAddr::V4(network_addr),
        IpAddr::V4(advertisement.network_mask()),
    )
    .map_err(|_| LsaError::InvalidNetworkMask(advertisement.network_mask()))?;

    let protocol_data = ProtocolData::Ospf(OspfData {
        area_id: lsa.area_id,
        advertisement: lsa.advertisement.clone(),
        link_state_id: lsa.link_state_id,
        advertising_router: lsa.router_id,
        checksum: Some(advertisement.header.ls_checksum),
        payload: crate::network::node::OspfPayload::Network(
            crate::network::node::OspfNetworkPayload {
                designated_router_id: Some(RouterId::Ipv4(lsa.link_state_id)),
                summaries: Vec::new(),
                externals: vec![],
            },
        ),
    });

    let attached_routers = advertisement
        .iter_attached_routers()
        .map(RouterId::Ipv4)
        .collect::<Vec<_>>();

    Ok(Network {
        ip_address: network,
        protocol_data: Some(protocol_data),
        attached_routers: attached_routers,
    })
}

pub fn parse_lsa_type_3(lsa: &OspfLsdbEntry) -> Result<Network, LsaError> {
    let adv = if let OspfLinkStateAdvertisement::SummaryLinkIpNetwork(ad) = &*lsa.advertisement {
        ad
    } else {
        return Err(LsaError::InvalidLsaType);
    };
    let net_addr = IpNetwork::with_netmask(
        IpAddr::V4(lsa.link_state_id),
        IpAddr::V4(adv.network_mask()),
    )
    .map_err(|_| LsaError::InvalidNetworkMask(adv.network_mask()))?;

    let protocol_data = ProtocolData::Ospf(OspfData {
        area_id: lsa.area_id,
        advertisement: lsa.advertisement.clone(),
        link_state_id: lsa.link_state_id,
        advertising_router: lsa.router_id,
        checksum: Some(adv.header.ls_checksum),
        // Represent Type-3 summary network as a Network payload with a single summary entry collected.
        payload: crate::network::node::OspfPayload::Network(
            crate::network::node::OspfNetworkPayload {
                designated_router_id: None,
                summaries: vec![crate::network::node::OspfSummaryNetPayload {
                    metric: adv.metric as u32,
                    origin_abr: RouterId::Ipv4(lsa.router_id),
                }],
                externals: vec![],
            },
        ),
    });

    Ok(Network {
        ip_address: net_addr,
        protocol_data: Some(protocol_data),
        // Attach the originating ABR so the summary network is connected;
        // later consolidation will fold this into a detailed Type-2 if present.
        attached_routers: vec![RouterId::Ipv4(lsa.router_id)],
    })
}

#[cfg(test)]
mod tests {

    use crate::data_aquisition::snmp::SnmpClient;
    use crate::parsers::ospf_parser::snmp_source::OspfSnmpSource;
    use crate::parsers::ospf_parser::source::OspfDataSource;

    use super::*;

    #[tokio::test]
    async fn test_parse_lsas_to_lsdb_entries() {
        let client = SnmpClient::new(
            "127.0.0.1:1161".parse().unwrap(),
            "public",
            snmp2::Version::V2C,
            None,
        );
        let mut source = OspfSnmpSource::new(client);

        let rows = source.fetch_lsdb_rows().await.unwrap();

        for row in &rows {
            // Inspect raw rows if needed
            let _ = row;
        }

        let lsdb_entries: Vec<OspfLsdbEntry> = rows
            .into_iter()
            .map(|row| OspfLsdbEntry::try_from(row).unwrap())
            .collect();

        assert_ne!(lsdb_entries.len(), 0);

        for entry in lsdb_entries {
            dbg!(entry);
        }
    }

    #[tokio::test]
    async fn test_parse_to_nodes() {
        let client = SnmpClient::new(
            "127.0.0.1:1161".parse().unwrap(),
            "public",
            snmp2::Version::V2C,
            None,
        );
        let mut source = OspfSnmpSource::new(client);

        let rows = source.fetch_lsdb_rows().await.unwrap();

        let lsdb_entries: Vec<OspfLsdbEntry> = rows
            .into_iter()
            .map(|row| OspfLsdbEntry::try_from(row).unwrap())
            .collect();

        for entry in lsdb_entries {
            let node: Result<Node, LsaError> = entry.try_into();
            if let Err(LsaError::InvalidLsaType) = node {
                continue;
            }
            assert!(node.is_ok());
            dbg!(node.unwrap());
        }
    }
}
