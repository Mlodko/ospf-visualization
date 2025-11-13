#![allow(dead_code, deprecated)]
use std::str::FromStr;

use snmp2::Oid;

use crate::{
    data_aquisition::{
        core::{LinkStateValue},
        snmp::{SnmpClient, SnmpTableRow},
    },
    parsers::ospf_parser::{
        OspfError,
        lsa::{LsaError, OspfLsdbEntry},
        source::OspfRawRow,
    },
};

const COLUMN_ID_COMPONENT_LENGTH: usize = 1;

#[deprecated]
pub async fn query_router(client: &mut SnmpClient) -> Result<Vec<OspfLsdbEntry>, OspfError> {
    let column_oids = [
        "1.3.6.1.2.1.14.4.1.1",
        "1.3.6.1.2.1.14.4.1.2",
        "1.3.6.1.2.1.14.4.1.3",
        "1.3.6.1.2.1.14.4.1.4",
        "1.3.6.1.2.1.14.4.1.5",
        "1.3.6.1.2.1.14.4.1.6",
        "1.3.6.1.2.1.14.4.1.7",
        "1.3.6.1.2.1.14.4.1.8",
    ]
    .into_iter()
    .map(|oid| Oid::from_str(oid).unwrap())
    .collect();

    let query = client
        .query()
        .await
        .map_err(|e| OspfError::Snmp(e))?
        .oids(column_oids)
        .get_bulk(0, 128);

    let raw_data = query.execute().await.map_err(|e| OspfError::Snmp(e))?;

    let rows = SnmpTableRow::group_into_rows(
        raw_data,
        &Oid::from_str("1.3.6.1.2.1.14.4.1")
            .expect("Failed to parse OID, THIS SHOULD NEVER HAPPEN"),
        COLUMN_ID_COMPONENT_LENGTH,
    )
    .map_err(|e| OspfError::Snmp(e))?;

    // Map SNMP rows into transport-neutral OspfRawRow
    let area_oid = Oid::from_str("1.3.6.1.2.1.14.4.1.1").unwrap();
    let lsid_oid = Oid::from_str("1.3.6.1.2.1.14.4.1.3").unwrap();
    let rid_oid = Oid::from_str("1.3.6.1.2.1.14.4.1.4").unwrap();
    let adv_oid = Oid::from_str("1.3.6.1.2.1.14.4.1.8").unwrap();

    let raw_rows: Vec<OspfRawRow> = rows
        .into_iter()
        .map(|row| {
            let area_id = match row.columns.get(&area_oid) {
                Some(LinkStateValue::IpAddress(ip)) => *ip,
                None => return Err(OspfError::Lsa(LsaError::IncompleteData)),
                _ => return Err(OspfError::Lsa(LsaError::WrongDataType)),
            };
            let link_state_id = match row.columns.get(&lsid_oid) {
                Some(LinkStateValue::IpAddress(ip)) => *ip,
                None => return Err(OspfError::Lsa(LsaError::IncompleteData)),
                _ => return Err(OspfError::Lsa(LsaError::WrongDataType)),
            };
            let router_id = match row.columns.get(&rid_oid) {
                Some(LinkStateValue::IpAddress(ip)) => *ip,
                None => return Err(OspfError::Lsa(LsaError::IncompleteData)),
                _ => return Err(OspfError::Lsa(LsaError::WrongDataType)),
            };
            let lsa_bytes = match row.columns.get(&adv_oid) {
                Some(LinkStateValue::OctetString(bytes)) => bytes.clone(),
                None => return Err(OspfError::Lsa(LsaError::IncompleteData)),
                _ => return Err(OspfError::Lsa(LsaError::WrongDataType)),
            };
            Ok(OspfRawRow {
                area_id,
                link_state_id,
                router_id,
                lsa_bytes,
            })
        })
        .collect::<Result<Vec<_>, OspfError>>()?;

    // Convert raw rows into LSDB entries (parsing bytes -> LSA)
    let entries: Vec<OspfLsdbEntry> = raw_rows
        .into_iter()
        .map(|raw| OspfLsdbEntry::try_from(raw).map_err(OspfError::Lsa))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(entries)
}
