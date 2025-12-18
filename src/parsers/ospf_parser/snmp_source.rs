use std::{collections::HashMap, net::Ipv4Addr, str::FromStr};

use async_trait::async_trait;
use egui::Link;
use snmp2::Oid;

use crate::{data_aquisition::{
    core::{LinkStateValue, RawRouterData},
    snmp::{SnmpClient, SnmpTableRow},
}, network::router::{InterfaceStats, RouterId}};
use crate::parsers::ospf_parser::source::{OspfDataSource, OspfRawRow, OspfSourceError};

/// OSPF-over-SNMP adapter that implements the protocol-centric OspfDataSource.
/// This maps SNMP table rows from the OSPF LSDB MIB into transport-neutral OspfRawRow.

pub struct OspfSnmpSource {
    client: SnmpClient,
}

impl OspfSnmpSource {
    pub fn new(client: SnmpClient) -> Self {
        Self { client }
    }
    
    pub async fn fetch_source_id(&mut self) -> Result<RouterId, OspfSourceError> {
        let oid = Oid::from_str("1.3.6.1.2.1.14.1.1.0").unwrap();
        let response = self.client
            .query().await.map_err(|e| OspfSourceError::Acquisition(format!("{e:?}")))?
            .get().oid(oid)
            .execute().await
            .map_err(|e| OspfSourceError::Acquisition(format!("{e:?}")))?;
        match response.first() {
            Some(RawRouterData::Snmp { value: LinkStateValue::IpAddress(ip), .. }) => {
                Ok(RouterId::Ipv4(*ip))
            }
            Some(other) => Err(OspfSourceError::Invalid(format!("unexpected ospfRouterId: {other:?}"))),
            None => Err(OspfSourceError::Invalid("missing ospfRouterId".into())),
        }
    }
    
    pub async fn fetch_stats(&mut self) -> Result<Vec<InterfaceStats>, OspfSourceError> {
        
        // Firstly we need to get an IF index -> stats mapping for all interfaces
        
        let rx_packets_oid = Oid::from_str("1.3.6.1.2.1.2.2.1.11").unwrap();
        let tx_packets_oid = Oid::from_str("1.3.6.1.2.1.2.2.1.17").unwrap();
        let rx_bytes_oid = Oid::from_str("1.3.6.1.2.1.2.2.1.10").unwrap();
        let tx_bytes_oid = Oid::from_str("1.3.6.1.2.1.2.2.1.16").unwrap();
        
        let rx_packets = self.client.query().await.map_err(|e| OspfSourceError::Acquisition(e.to_string()))?
            .oid(rx_packets_oid)
            .walk()
            .execute()
            .await
            .map_err(|e| OspfSourceError::Acquisition(e.to_string()))?
            .iter()
            .map(|raw| {
                if let RawRouterData::Snmp { oid, value } = raw {
                    let if_id = oid.iter().ok_or(OspfSourceError::Invalid("fetch_stats: sub OID doesn't fit into u64".to_string()))?
                        .last().ok_or(OspfSourceError::Invalid("fetch_stats: couldn't extract IF ID".to_string()))?;
                    
                    let rx_packets = if let LinkStateValue::Counter32(v) = value {
                        *v as u64
                    } else {
                        return Err(OspfSourceError::Invalid("fetch_stats: unexpected data type".to_string()));
                    };
                    
                    Ok((if_id, rx_packets))
                } else {
                    Err(OspfSourceError::Invalid("fetch_stats: unexpected data type".to_string()))
                }
            })
            .collect::<Result<HashMap<u64, u64>, _>>()?;
        let tx_packets = self.client.query().await.map_err(|e| OspfSourceError::Acquisition(e.to_string()))?
            .oid(tx_packets_oid)
            .walk()
            .execute()
            .await
            .map_err(|e| OspfSourceError::Acquisition(e.to_string()))?
            .iter()
            .map(|raw| {
                if let RawRouterData::Snmp { oid, value } = raw {
                    let if_id = oid.iter().ok_or(OspfSourceError::Invalid("fetch_stats: sub OID doesn't fit into u64".to_string()))?
                        .last().ok_or(OspfSourceError::Invalid("fetch_stats: couldn't extract IF ID".to_string()))?;
                    
                    let tx_packets = if let LinkStateValue::Counter32(v) = value {
                        *v as u64
                    } else {
                        return Err(OspfSourceError::Invalid("fetch_stats: unexpected data type".to_string()));
                    };
                    
                    Ok((if_id, tx_packets))
                } else {
                    return Err(OspfSourceError::Invalid("fetch_stats: unexpected data type".to_string()));
                }
            })
            .collect::<Result<HashMap<u64, u64>, _>>()?;
        let rx_bytes = self.client.query().await.map_err(|e| OspfSourceError::Acquisition(e.to_string()))?
            .oid(rx_bytes_oid)
            .walk()
            .execute()
            .await
            .map_err(|e| OspfSourceError::Acquisition(e.to_string()))?
            .iter()
            .map(|raw| {
                if let RawRouterData::Snmp { oid, value } = raw {
                    let if_id = oid.iter().ok_or(OspfSourceError::Invalid("fetch_stats: sub OID doesn't fit into u64".to_string()))?
                        .last().ok_or(OspfSourceError::Invalid("fetch_stats: couldn't extract IF ID".to_string()))?;
                    
                    let rx_bytes = if let LinkStateValue::Counter32(v) = value {
                        *v as u64
                    } else {
                        return Err(OspfSourceError::Invalid("fetch_stats: unexpected data type".to_string()));
                    };
                    
                    Ok((if_id, rx_bytes))
                } else {
                    return Err(OspfSourceError::Invalid("fetch_stats: unexpected data type".to_string()));
                }
            })
            .collect::<Result<HashMap<u64, u64>, _>>()?;
        let tx_bytes = self.client.query().await.map_err(|e| OspfSourceError::Acquisition(e.to_string()))?
            .oid(tx_bytes_oid)
            .walk()
            .execute()
            .await
            .map_err(|e| OspfSourceError::Acquisition(e.to_string()))?
            .iter()
            .map(|raw| {
                if let RawRouterData::Snmp { oid, value } = raw {
                    let if_id = oid.iter().ok_or(OspfSourceError::Invalid("fetch_stats: sub OID doesn't fit into u64".to_string()))?
                        .last().ok_or(OspfSourceError::Invalid("fetch_stats: couldn't extract IF ID".to_string()))?;
                    
                    let tx_bytes = if let LinkStateValue::Counter32(v) = value {
                        *v as u64
                    } else {
                        return Err(OspfSourceError::Invalid("fetch_stats: unexpected data type".to_string()));
                    };
                    
                    Ok((if_id, tx_bytes))
                } else {
                    return Err(OspfSourceError::Invalid("fetch_stats: unexpected data type".to_string()));
                }
            })
            .collect::<Result<HashMap<u64, u64>, _>>()?;
        
        // Now combine the results into a Hashmap<IF ID, Stats>
        
        struct Stats {
            tx_bytes: u64,
            rx_bytes: u64,
            tx_packets: u64,
            rx_packets: u64,
        }
        
        let if_ids = rx_packets.iter().map(|(id, _)| *id);
        
        let stats_per_if = if_ids
            .map(|id| {
                let tx_bytes = tx_bytes.get(&id).ok_or_else(|| OspfSourceError::Invalid(format!("fetch_stats: missing tx_bytes for interface {}", id)))?;
                let rx_bytes = rx_bytes.get(&id).ok_or_else(|| OspfSourceError::Invalid(format!("fetch_stats: missing rx_bytes for interface {}", id)))?;
                let tx_packets = tx_packets.get(&id).ok_or_else(|| OspfSourceError::Invalid(format!("fetch_stats: missing tx_packets for interface {}", id)))?;
                let rx_packets = rx_packets.get(&id).ok_or_else(|| OspfSourceError::Invalid(format!("fetch_stats: missing rx_packets for interface {}", id)))?;
                let stats = Stats {
                    tx_bytes: *tx_bytes,
                    rx_bytes: *rx_bytes,
                    tx_packets: *tx_packets,
                    rx_packets: *rx_packets,
                };
                Ok((id, stats))
            })
            .collect::<Result<HashMap<u64, Stats>, _>>()?;
        
        // Now we grab ip -> if id mapping from 1.3.6.1.2.1.4.20.1.2
        
        let ip_map_oid = Oid::from_str("1.3.6.1.2.1.4.20.1.2").unwrap();
        
        let if_id_to_ip: HashMap<u64, Ipv4Addr> = self.client.query().await.map_err(|e| OspfSourceError::Invalid(format!("fetch_stats: failed to query ip -> if id mapping: {}", e)))?
            .oid(ip_map_oid)
            .walk()
            .execute()
            .await
            .map_err(|e| OspfSourceError::Invalid(format!("fetch_stats: failed to execute ip -> if id mapping query: {}", e)))?
            .into_iter()
            .map(|raw| {
                if let RawRouterData::Snmp { oid, value } = raw {
                    let if_id = if let LinkStateValue::Integer(i) = value {
                        i as u64
                    } else {
                        return Err(OspfSourceError::Invalid(format!("fetch_stats: invalid if id value: {:?}", value)));
                    };
                    
                    // IP is the last 4 sub-OIDs
                    let sub_oids: Vec<u64> = oid.iter().ok_or(OspfSourceError::Invalid("fetch_stats: invalid ip address".to_string()))?
                        .collect();
                    
                    let ip_addr_parts = sub_oids.iter()
                        .skip(sub_oids.len() - 4)
                        .map(|oid| {
                            *oid as u8
                        })
                        .collect::<Vec<u8>>();
                    
                    let ip_addr = Ipv4Addr::new(ip_addr_parts[0], ip_addr_parts[1], ip_addr_parts[2], ip_addr_parts[3]);
                    
                    Ok((if_id, ip_addr))
                } else {
                    Err(OspfSourceError::Invalid("fetch_stats: invalid data type".to_string()))
                }
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
            
        if_id_to_ip.into_iter()
            .map(|(if_id, ip_addr)| {
                let stats = stats_per_if.get(&if_id).ok_or(OspfSourceError::Invalid(format!("No stats for interface {}", if_id)))?;
                Ok(InterfaceStats {
                    ip_address: std::net::IpAddr::V4(ip_addr),
                    rx_bytes: Some(stats.rx_bytes),
                    tx_bytes: Some(stats.tx_bytes),
                    rx_packets: Some(stats.rx_packets),
                    tx_packets: Some(stats.tx_packets),
                })
            })
            .collect()
    }
}

#[async_trait]
impl OspfDataSource for OspfSnmpSource {
    async fn fetch_lsdb_rows(&mut self) -> Result<Vec<OspfRawRow>, OspfSourceError> {
        // Only fetch the columns we actually need to build OspfRawRow
        // 1 -> ospfLsdbAreaId
        // 3 -> ospfLsdbLsid (link state id)
        // 4 -> ospfLsdbRouterId
        // 8 -> ospfLsdbAdvertisement
        let column_oids = [
            "1.3.6.1.2.1.14.4.1.1",
            "1.3.6.1.2.1.14.4.1.3",
            "1.3.6.1.2.1.14.4.1.4",
            "1.3.6.1.2.1.14.4.1.8",
        ]
        .into_iter()
        .map(|oid| Oid::from_str(oid).unwrap())
        .collect();

        let query = self
            .client
            .query()
            .await
            .map_err(|e| OspfSourceError::Acquisition(format!("{e:?}")))?
            .oids(column_oids)
            .get_bulk(0, 128);

        let raw_data = query
            .execute()
            .await
            .map_err(|e| OspfSourceError::Acquisition(format!("{e:?}")))?;

        let table_oid = Oid::from_str("1.3.6.1.2.1.14.4.1").unwrap();
        let rows = SnmpTableRow::group_into_rows(raw_data, &table_oid, 1)
            .map_err(|e| OspfSourceError::Acquisition(format!("{e:?}")))?;

        let area_oid = Oid::from_str("1.3.6.1.2.1.14.4.1.1").unwrap();
        let lsid_oid = Oid::from_str("1.3.6.1.2.1.14.4.1.3").unwrap();
        let rid_oid = Oid::from_str("1.3.6.1.2.1.14.4.1.4").unwrap();
        let adv_oid = Oid::from_str("1.3.6.1.2.1.14.4.1.8").unwrap();

        rows.into_iter()
            .map(|row| {
                let area_id = match row.columns.get(&area_oid) {
                    Some(LinkStateValue::IpAddress(ip)) => *ip,
                    other => {
                        return Err(OspfSourceError::Invalid(format!(
                            "area_id: unexpected value {:?}",
                            other
                        )));
                    }
                };
                let link_state_id = match row.columns.get(&lsid_oid) {
                    Some(LinkStateValue::IpAddress(ip)) => *ip,
                    other => {
                        return Err(OspfSourceError::Invalid(format!(
                            "link_state_id: unexpected value {:?}",
                            other
                        )));
                    }
                };
                let router_id = match row.columns.get(&rid_oid) {
                    Some(LinkStateValue::IpAddress(ip)) => *ip,
                    other => {
                        return Err(OspfSourceError::Invalid(format!(
                            "router_id: unexpected value {:?}",
                            other
                        )));
                    }
                };
                let lsa_bytes = match row.columns.get(&adv_oid) {
                    Some(LinkStateValue::OctetString(bytes)) => bytes.clone(),
                    other => {
                        return Err(OspfSourceError::Invalid(format!(
                            "advertisement bytes: unexpected value {:?}",
                            other
                        )));
                    }
                };

                Ok(OspfRawRow {
                    area_id,
                    link_state_id,
                    router_id,
                    lsa_bytes,
                })
            })
            .collect()
    }
}

mod tests {
    use std::net::SocketAddr;

    use super::*;
    
    #[tokio::test]
    async fn test_fetch_stats() {
        let client = SnmpClient::new("127.0.0.1:1161".parse().unwrap(), "public", snmp2::Version::V2C, None);
        let mut source = OspfSnmpSource::new(client);
        
        let stats = source.fetch_stats().await;
        assert!(stats.is_ok());
        
        assert!(stats.as_ref().unwrap().len() > 0);
        
        if let Ok(stats) = stats {
            for stat in stats {
                dbg!(stat);
            }
        }
    }
}