use std::{net::{IpAddr, Ipv4Addr}, str::FromStr, sync::Arc};
use crate::{data_aquisition::{core::{LinkStateValue, RawRouterData}, snmp::SnmpTableRow}, network::node::{Node, NodeInfo, OspfData, ProtocolData}};
use ipnetwork::{IpNetwork, Ipv4Network};
use ospf_parser::{OspfLinkStateAdvertisement, OspfNetworkLinksAdvertisement, OspfRouterLink, OspfRouterLinksAdvertisement};
use snmp2::Oid;
use nom_derive::Parse;

use crate::network::{node::Network, router::{Router, RouterId}};

#[derive(Debug, Clone)]
pub enum LsaError {
    InvalidLsaType,
    InvalidNetworkMask(Ipv4Addr),
    ProtocolNotImplemented,
    WrongSnmpTable,
    IncompleteData,
    WrongDataType
}

#[derive(Debug)]
pub struct OspfLsdbEntry {
    area_id: Ipv4Addr,
    link_state_id: Ipv4Addr,
    router_id: Ipv4Addr,
    advertisement: Arc<OspfLinkStateAdvertisement>,
}

impl TryFrom<SnmpTableRow<'_>> for OspfLsdbEntry {
    type Error = LsaError;

    fn try_from(row: SnmpTableRow<'_>) -> Result<Self, Self::Error> {
        if row.table_oid_prefix != Oid::from_str("1.3.6.1.2.1.14.4.1").unwrap() {
            return Err(LsaError::WrongSnmpTable);
        }
        
        // Parse area id
        let area_id: Ipv4Addr = if let LinkStateValue::IpAddress(ip) = row.columns.get(&Oid::from_str("1.3.6.1.2.1.14.4.1.1").unwrap())
        .ok_or(LsaError::IncompleteData)? {
            *ip
        } else {
            println!("Wrong data type for area ID");
            return Err(LsaError::WrongDataType);
        };
        
        // Link state ID
        let link_state_id: Ipv4Addr = if let LinkStateValue::IpAddress(ip) = row.columns.get(&Oid::from_str("1.3.6.1.2.1.14.4.1.3").unwrap())
        .ok_or(LsaError::IncompleteData)? {
            *ip
        } else {
            println!("Wrong data type for link state ID");
            return Err(LsaError::WrongDataType);
        };
        
        // Router ID
        let router_id: Ipv4Addr = if let LinkStateValue::IpAddress(ip) = row.columns.get(&Oid::from_str("1.3.6.1.2.1.14.4.1.4").unwrap())
        .ok_or(LsaError::IncompleteData)? {
            *ip
        } else {
            println!("Wrong data type for router ID");
            return Err(LsaError::WrongDataType);
        };
        
        // Advertisement
        let advertisement: OspfLinkStateAdvertisement = 
        if let LinkStateValue::OctetString(bytes) = row.columns.get(&Oid::from_str("1.3.6.1.2.1.14.4.1.8").unwrap())
        .ok_or(LsaError::IncompleteData)? {
            OspfLinkStateAdvertisement::parse(bytes).map_err(|_| LsaError::WrongDataType)?.1
        } else {
            println!("Wrong data type for advertisement");
            return Err(LsaError::WrongDataType);
        };
        
        Ok(OspfLsdbEntry { area_id, link_state_id, router_id, advertisement: Arc::new(advertisement) })
    }
}

impl TryInto<Node> for OspfLsdbEntry {
    type Error = LsaError;

    fn try_into(self) -> Result<Node, Self::Error> {
        let node_info = match *self.advertisement {
            OspfLinkStateAdvertisement::RouterLinks(_) => NodeInfo::Router(parse_lsa_type_1_to_router(&self)?),
            OspfLinkStateAdvertisement::NetworkLinks(_) => NodeInfo::Network(parse_lsa_type_2_to_network(&self)?),
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
    let interfaces: Vec<IpAddr> = advertisement.links.iter()
        .map(|link| IpAddr::V4(link.link_data()))
        .collect();
    let ospf_data: OspfData = OspfData {
        area_id: lsa.area_id,
        advertisement: lsa.advertisement.clone(),
    };
    
    let router = Router {
        id: router_id,
        interfaces,
        protocol_data: Some(ProtocolData::Ospf(ospf_data))
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
        IpAddr::V4(advertisement.network_mask())
    )
    .map_err(|_| LsaError::InvalidNetworkMask(advertisement.network_mask()))?;
    
    let protocol_data = ProtocolData::Ospf(
        OspfData {
            area_id: lsa.area_id,
            advertisement: lsa.advertisement.clone()
        }
    );
    
    let attached_routers = advertisement.iter_attached_routers()
        .map(|router_id| RouterId::Ipv4(router_id))
        .collect::<Vec<_>>();
    
    
    Ok(Network {
        ip_address: network,
        protocol_data: Some(protocol_data),
        attached_routers
    })
}

mod tests {
    use std::net::SocketAddr;

    use crate::data_aquisition::snmp::SnmpClient;

    use super::*;
    
    #[tokio::test]
    async fn test_parse_lsas_to_lsdb_entries() {
        let mut client = SnmpClient::new("127.0.0.1:1161".parse().unwrap(), "public", snmp2::Version::V2C, None);
        let column_oids
         = [
            "1.3.6.1.2.1.14.4.1.1",
            "1.3.6.1.2.1.14.4.1.2",
            "1.3.6.1.2.1.14.4.1.3",
            "1.3.6.1.2.1.14.4.1.4",
            "1.3.6.1.2.1.14.4.1.5",
            "1.3.6.1.2.1.14.4.1.6",
            "1.3.6.1.2.1.14.4.1.7",
            "1.3.6.1.2.1.14.4.1.8"
        ].into_iter().map(|oid| Oid::from_str(oid).unwrap()).collect();
        
        let query = client.query().await.unwrap()
            .oids(column_oids)
            .get_bulk(0, 32);
        
        let raw_data = query.execute().await.unwrap();
        
        let rows = SnmpTableRow::group_into_rows(
            raw_data, 
            &Oid::from_str("1.3.6.1.2.1.14.4.1").unwrap(), 
            1
        ).unwrap();
        
        for row in &rows {
                dbg!(row);
        }
        
        let lsdb_entries: Vec<OspfLsdbEntry> = rows.into_iter().map(|row| {
            OspfLsdbEntry::try_from(row).unwrap()
        }).collect();
        
        assert_ne!(lsdb_entries.len(), 0);
        
        for entry in lsdb_entries {
            dbg!(entry);
        }
    }
    
    #[tokio::test]
    async fn test_parse_to_nodes() {
        let mut client = SnmpClient::new("127.0.0.1:1161".parse().unwrap(), "public", snmp2::Version::V2C, None);
        let column_oids = [
            "1.3.6.1.2.1.14.4.1.1",
            "1.3.6.1.2.1.14.4.1.2",
            "1.3.6.1.2.1.14.4.1.3",
            "1.3.6.1.2.1.14.4.1.4",
            "1.3.6.1.2.1.14.4.1.5",
            "1.3.6.1.2.1.14.4.1.6",
            "1.3.6.1.2.1.14.4.1.7",
            "1.3.6.1.2.1.14.4.1.8"
        ].into_iter().map(|oid| Oid::from_str(oid).unwrap()).collect();
        
        let query = client.query().await.unwrap()
            .oids(column_oids)
            .get_bulk(0, 32);
        
        let raw_data = query.execute().await.unwrap();
        
        let rows = SnmpTableRow::group_into_rows(
            raw_data, 
            &Oid::from_str("1.3.6.1.2.1.14.4.1").unwrap(), 
            1
        ).unwrap();
        
        /*
        for row in &rows {
                dbg!(row);
        }
        */
        
        let lsdb_entries: Vec<OspfLsdbEntry> = rows.into_iter().map(|row| {
            OspfLsdbEntry::try_from(row).unwrap()
        }).collect();
        
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