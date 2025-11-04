#![allow(dead_code)]

use crate::data_aquisition::core::LinkStateValue;

use super::core::{NetworkClient, RawRouterData};
use ospf_parser::OspfRouterLinksAdvertisement;
use snmp2::{AsyncSession, MessageType, Oid, Version, v3::Security};
use std::{collections::HashMap, error::Error, fmt::Display, net::SocketAddr, str::FromStr, sync::Arc, time::Duration};
use tokio::sync::Mutex;

/// SNMP client for retrieving data from a network device.
pub struct SnmpClient {
    address: SocketAddr,
    community: String,
    snmp_version: Version,
    session: Option<Arc<Mutex<AsyncSession>>>,
    security: Option<Security>,
}

impl Default for SnmpClient {
    fn default() -> Self {
        Self::new(
            SocketAddr::from_str("127.0.0.1:1161").unwrap(),
            "public",
            Version::V2C,
            None,
        )
    }
}

impl SnmpClient {
    /// Creates a new SNMP client for a single network device.
    pub fn new(
        address: SocketAddr,
        community: &str,
        snmp_version: Version,
        security: Option<Security>,
    ) -> Self {
        Self {
            address,
            community: community.to_string(),
            snmp_version,
            session: None,
            security,
        }
    }

    /// Retrieves an SNMP session for the client.
    pub async fn get_session(&mut self) -> Result<Arc<Mutex<AsyncSession>>, SnmpClientError> {
        if self.session.is_none() {
            let session = match self.snmp_version {
                Version::V1 => {
                    AsyncSession::new_v1(self.address, self.community.as_bytes(), 0).await
                }
                Version::V2C => {
                    AsyncSession::new_v2c(self.address, self.community.as_bytes(), 0).await
                }
                Version::V3 => {
                    if let Some(security) = &self.security {
                        AsyncSession::new_v3(self.address, 0, security.clone()).await
                    } else {
                        return Err(SnmpClientError::NoV3Security);
                    }
                }
            };
            match session {
                Ok(s) => {
                    self.session = Some(Arc::new(Mutex::new(s)));
                    Ok(self.session.as_ref().unwrap().clone())
                }
                Err(e) => Err(SnmpClientError::IoError(e)),
            }
        } else {
            Ok(self.session.as_ref().unwrap().clone())
        }
    }
    
    /// Start building a new query.
    pub async fn query(&mut self) -> Result<QueryBuilder<'_>, SnmpClientError> {
        let session = self.get_session().await?;
        Ok(QueryBuilder {
            session,
            oids: Vec::new(),
            operation: None,
            timeout: None,
            max_repetitions: None,
            non_repeaters: None,
        })
    }
}

pub struct QueryBuilder<'a> {
    session: Arc<Mutex<AsyncSession>>,
    oids: Vec<Oid<'a>>,
    operation: Option<MessageType>,
    timeout: Option<Duration>,
    non_repeaters: Option<u32>,
    max_repetitions: Option<u32>,
}

impl<'a> QueryBuilder<'a> {
    pub fn get(mut self) -> Self {
        self.operation = Some(MessageType::GetRequest);
        self
    }

    pub fn get_next(mut self) -> Self {
        self.operation = Some(MessageType::GetNextRequest);
        self
    }

    pub fn get_bulk(mut self, non_repeaters: u32, max_repetitions: u32) -> Self {
        self.operation = Some(MessageType::GetBulkRequest);
        self.non_repeaters = Some(non_repeaters);
        self.max_repetitions = Some(max_repetitions);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn oid(mut self, oid: Oid<'static>) -> Self {
        self.oids.push(oid);
        self
    }

    pub fn oids(mut self, oids: Vec<Oid<'static>>) -> Self {
        self.oids.extend(oids);
        self
    }

    pub fn oid_str(self, oid_str: &str) -> Result<Self, SnmpClientError> {
        let oid = Oid::from_str(oid_str).map_err(|_| SnmpClientError::OidParseError)?;
        Ok(self.oid(oid))
    }

    pub async fn execute(self) -> Result<Vec<super::core::RawRouterData<'a>>, SnmpClientError> {
        if self.operation.is_none() || self.oids.is_empty() {
            return Err(SnmpClientError::InvalidQuery);
        }

        // Get all the data we need upfront
        let operation = self.operation.unwrap();
        let non_repeaters = self.non_repeaters.unwrap_or(0);
        let max_repetitions = self.max_repetitions.unwrap_or(0);

        let session_arc = Arc::clone(&self.session);

        let mut session = session_arc.lock().await;

        let response = match operation {
            MessageType::GetRequest => {
                if self.oids.len() != 1 {
                    return Err(SnmpClientError::MultipleOidsOnGet);
                }
                // Clone the oid to avoid borrowing
                let oid = self.oids[0].clone();
                Ok(session
                    .get(&oid)
                    .await
                    .map_err(SnmpClientError::Snmp2Error)?)
            }
            MessageType::GetNextRequest => {
                if self.oids.len() != 1 {
                    return Err(SnmpClientError::MultipleOidsOnGet);
                }
                let oid = self.oids[0].clone();
                Ok(session
                    .getnext(&oid)
                    .await
                    .map_err(SnmpClientError::Snmp2Error)?)
            }
            MessageType::GetBulkRequest => {
                // Clone all oids to avoid lifetime issues
                let oids: Vec<Oid<'a>> = self.oids.iter().cloned().collect();
                let oid_refs: Vec<&Oid> = oids.iter().collect();

                Ok(session
                    .getbulk(&oid_refs, non_repeaters, max_repetitions)
                    .await
                    .map_err(SnmpClientError::Snmp2Error)?)
            }
            _ => Err(SnmpClientError::UnsupportedSnmpOperation),
        }?;

        let raw_router_data: Vec<RawRouterData> = response
            .varbinds
            .into_iter()
            .map(|(oid, value)| RawRouterData::Snmp {
                oid: oid.to_owned(),
                value: LinkStateValue::from(&value),
            })
            .collect();

        Ok(raw_router_data)
    }
}

#[derive(Debug)]
pub enum SnmpClientError {
    OidParseError,
    NoV3Security,
    IoError(std::io::Error),
    Snmp2Error(snmp2::Error),
    NoSession,
    InvalidQuery,
    MultipleOidsOnGet,
    UnsupportedSnmpOperation,
    InvalidData,
}

#[allow(unused_variables)]
impl Display for SnmpClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl Error for SnmpClientError {}

impl NetworkClient for SnmpClient {
}

/// A utility struct representing a single row of an SNMP table.
#[derive(Debug, Clone)]
pub struct SnmpTableRow<'a> {
    pub table_oid_prefix: Oid<'a>,
    pub row_index_suffix: Oid<'a>,
    pub columns: HashMap<Oid<'a>, LinkStateValue>,
}

impl SnmpTableRow<'_> {
    pub fn group_into_rows<'a>(
        raw_data: Vec<RawRouterData<'a>>,
        table_oid_prefix: &Oid<'a>,
        column_id_component_length: usize,
    ) -> Result<Vec<SnmpTableRow<'a>>, SnmpClientError> {
        // Convert to OID, value pairs
        let pairs: Vec<(Oid<'a>, LinkStateValue)> = raw_data
            .into_iter()
            .map(|data| match data {
                RawRouterData::Snmp { oid, value } => Ok((oid, value)),
                _ => Err(SnmpClientError::InvalidData),
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Row index: (column oid, value)
        let mut rows_map: HashMap<Oid<'a>, HashMap<Oid<'a>, LinkStateValue>> = HashMap::new();
        let table_prefix_length = table_oid_prefix.iter().expect("Table OID prefix component(s) don't fit into u64").count();
        
        for (oid, value) in pairs.into_iter().filter(|(oid, _)| oid.starts_with(table_oid_prefix)) {
            let row_index_suffix: Vec<u64> = oid.iter().expect("OID component(s) don't fit into u64")
                .skip(table_prefix_length + column_id_component_length).collect();
            let row_index_suffix = Oid::from(&row_index_suffix).map_err(|_| SnmpClientError::OidParseError)?;
            // Skip the row suffix
            let column_oid = {
                let oid: Vec<u64> = oid.iter().ok_or(SnmpClientError::OidParseError)?.collect();
                let column_oid = &oid[..table_prefix_length + column_id_component_length];
                Oid::from(column_oid).map_err(|_| SnmpClientError::OidParseError)
            }?;
            rows_map.entry(row_index_suffix).or_default().insert(column_oid, value);
        }
        
        let rows = rows_map.into_iter().map(|(row_index_suffix, columns)| {
            SnmpTableRow {
                table_oid_prefix: table_oid_prefix.clone(),
                row_index_suffix,
                columns,
            }
        }).collect();
        
        Ok(rows)
    }
}

mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::*;
    
    async fn setup() -> Result<Arc<Mutex<AsyncSession>>, SnmpClientError> {
        let mut client = SnmpClient::new(
            SocketAddr::new("127.0.0.1".parse().unwrap(), 1161),
            "public",
            Version::V2C,
            None
        );
        client.get_session().await
    }
    
    #[tokio::test]
    async fn test_snmp_connect() {
        let session = setup().await;
        assert!(session.is_ok());
    }
    
    #[tokio::test]
    async fn test_snmp_get_data() {
        let session = setup().await.expect("Failed to setup session");
        let mut lock = session.lock().await;
        let result = lock.get(&Oid::from_str("1.3.6.1.2.1.1").unwrap()).await;
        assert!(result.is_ok());
        dbg!(result.unwrap());
    }
    
    #[tokio::test]
    async fn test_snmp_get_ospf_data() {
        let session = setup().await.expect("Failed to setup session");
        let mut lock = session.lock().await;
        let result = lock.get(&Oid::from_str("1.3.6.1.2.1.14.1.1.0").unwrap()).await;
        assert!(result.is_ok());
        let varbinds : Vec<(Oid<'_>, snmp2::Value<'_>)> = result.unwrap().varbinds.collect();
        assert_ne!(varbinds.len(), 0);
        assert!(varbinds.first().is_some());
        println!("Oid: {}\nValue: {:?}", varbinds.first().unwrap().0, varbinds.first().unwrap().1);
    }
    
    #[tokio::test]
    async fn test_snmp_get_table() {
        let mut client = SnmpClient::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1161),
            "public",
            Version::V2C,
            None
        );
        let column_oids: Vec<Oid> = [
            "1.3.6.1.2.1.14.4.1.1",
            "1.3.6.1.2.1.14.4.1.2",
            "1.3.6.1.2.1.14.4.1.3",
            "1.3.6.1.2.1.14.4.1.4",
            "1.3.6.1.2.1.14.4.1.5",
            "1.3.6.1.2.1.14.4.1.6",
            "1.3.6.1.2.1.14.4.1.7",
            "1.3.6.1.2.1.14.4.1.8"
            ].iter().map(|oid| Oid::from_str(oid).unwrap()).collect();
        let query = client.query().await.unwrap()
            .oids(column_oids)
            .get_bulk(0, 20);
        
        let results = query.execute().await.unwrap();
        
        let rows = SnmpTableRow::group_into_rows(results, &Oid::from_str("1.3.6.1.2.1.14.4.1").unwrap(), 1).unwrap();
        
        for row in rows {
            dbg!(row);
        }
    }
}