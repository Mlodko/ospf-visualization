use std::str::FromStr;

use async_trait::async_trait;
use snmp2::Oid;

use crate::{data_aquisition::{
    core::{LinkStateValue, RawRouterData},
    snmp::{SnmpClient, SnmpTableRow},
}, network::router::RouterId};
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
