use crate::{
    network::node::{Node, NodeInfo, OspfData, ProtocolData},
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
    let ospf_data: OspfData = OspfData {
        area_id: lsa.area_id,
        advertisement: lsa.advertisement.clone(),
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
    dbg!(advertisement);
    let network = IpNetwork::with_netmask(
        IpAddr::V4(lsa.link_state_id),
        IpAddr::V4(advertisement.network_mask()),
    )
    .map_err(|_| LsaError::InvalidNetworkMask(advertisement.network_mask()))?;

    let protocol_data = ProtocolData::Ospf(OspfData {
        area_id: lsa.area_id,
        advertisement: lsa.advertisement.clone(),
    });

    let attached_routers = advertisement
        .iter_attached_routers()
        .map(RouterId::Ipv4)
        .collect::<Vec<_>>();

    Ok(Network {
        ip_address: network,
        protocol_data: Some(protocol_data),
        attached_routers,
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
