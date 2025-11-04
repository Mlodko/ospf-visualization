use std::str::FromStr;

use snmp2::Oid;

use crate::{data_aquisition::snmp::{SnmpClient, SnmpClientError, SnmpTableRow}, parsers::ospf_parser::{OspfError, lsa::{LsaError, OspfLsdbEntry}}};

const COLUMN_ID_COMPONENT_LENGTH: usize = 1;

pub async fn query_router(client: &mut SnmpClient) -> Result<Vec<OspfLsdbEntry>, OspfError> {
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
    
    let query = client.query().await
        .map_err(|e| OspfError::Snmp(e))?
        .oids(column_oids)
        .get_bulk(0, 128);
    
    let raw_data = query.execute().await
        .map_err(|e| OspfError::Snmp(e))?;
    
    let rows = SnmpTableRow::group_into_rows(
        raw_data,
        &Oid::from_str("1.3.6.1.2.1.14.4.1").expect("Failed to parse OID, THIS SHOULD NEVER HAPPEN"),
        COLUMN_ID_COMPONENT_LENGTH
    ).map_err(|e| OspfError::Snmp(e))?;
    
    // Fail-fast collect: first LSA parse error will be returned as OspfError::Lsa(...)
    let entries: Vec<OspfLsdbEntry> = rows.into_iter()
        .map(|row| OspfLsdbEntry::try_from(row).map_err(|e| OspfError::Lsa(e)))
        .collect::<Result<Vec<_>, _>>()?;
    
    Ok(entries)
}
