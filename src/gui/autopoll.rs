use std::net::SocketAddr;

use crate::{data_aquisition::{snmp::SnmpClient, ssh::SshClient}, parsers::{isis_parser::topology::IsIsTopology, ospf_parser::snmp_source::OspfSnmpSource}, topology::{OspfSnmpTopology, source::SnapshotSource}};



#[derive(Clone)]
pub enum ProtocolKind {
    Ospf,
    Isis
}

#[derive(Clone)]
pub enum AcquisitionConfig {
    Snmp(SnmpAcquisitionConfig),
    Ssh(SshAcquisitionConfig),
}

#[derive(Clone)]
pub struct SnmpAcquisitionConfig {
    address: SocketAddr,
    community: String,
    snmp_version: snmp2::Version,
    security: Option<snmp2::v3::Security>,
}

#[derive(Clone)]
pub struct SshAcquisitionConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String
}

#[derive(Clone)]
pub struct SourceSpec {
    pub protocol: ProtocolKind,
    pub acquisition: AcquisitionConfig
}

impl SourceSpec {
    
    pub fn new_ssh(host: String, port: u16, username: String, password: String, protocol: ProtocolKind) -> Self {
        Self {
            protocol,
            acquisition: AcquisitionConfig::Ssh(SshAcquisitionConfig {
                host,
                port,
                username,
                password
            })
        }
    }
    
    pub fn new_snmp(address: SocketAddr, community: String, version: snmp2::Version, security: Option<snmp2::v3::Security>, protocol: ProtocolKind) -> Self {
        Self {
            protocol,
            acquisition: AcquisitionConfig::Snmp(SnmpAcquisitionConfig {
                address,
                community,
                snmp_version: version,
                security
            })
        }
    }
    
    pub async fn build_topology(&self) -> Result<Box<dyn SnapshotSource>, String> {
        match (&self.protocol, &self.acquisition) {
            (ProtocolKind::Ospf, AcquisitionConfig::Snmp(config)) => {
                let client = SnmpClient::new(
                    config.address,
                    &config.community,
                    config.snmp_version,
                    config.security.clone()
                );
                let topo = OspfSnmpTopology::from_snmp_client(client);
                Ok(Box::new(topo))
            }
            (ProtocolKind::Isis, AcquisitionConfig::Ssh(config)) => {
                let client = SshClient::new_with_password(config.username.clone(), config.host.clone(), config.password.clone(), config.port);
                let topo = IsIsTopology::new_from_ssh_client(client).await
                    .map_err(|e| format!("Failed to build ISIS topology: {}", e))?;
                Ok(Box::new(topo))
            }
            _ => Err("Unsupported protocol or acquisition method".to_string())
        }
    }
}