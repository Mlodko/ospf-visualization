use async_trait::async_trait;

use crate::{
    data_aquisition::ssh::SshClient, network::router::InterfaceStats, parsers::isis_parser::{
        core_lsp::NetAddress, frr_json_lsp::JsonLspdb, hostname::HostnameMap,
        protocol::JsonIsisProtocol,
    }, topology::{
        protocol::{AcquisitionError, AcquisitionSource},
        store::SourceId,
    }
};
use std::{collections::HashMap, env, net::IpAddr};

pub struct IsisSshSource {
    client: SshClient,
}

impl IsisSshSource {
    pub fn new(client: SshClient) -> Self {
        Self { client }
    }

    pub async fn fetch_hostname_map(&self) -> Result<HostnameMap, AcquisitionError> {
        println!("[IsisSshSource] fetch_hostname_map: start");
        if !self.client.is_connected() {
            println!("[IsisSshSource] fetch_hostname_map: client not connected");
            return Err(AcquisitionError::Transport(
                "SSH client is not connected".to_string(),
            ));
        }

        let output = self
            .client
            .execute_command("vtysh -c 'show isis hostname'")
            .await
            .map_err(|e| {
                AcquisitionError::Transport(format!("Failed to execute command: {}", e))
            })?;

        println!(
            "[IsisSshSource] fetch_hostname_map: got output length {}",
            output.len()
        );
        let map = HostnameMap::build_map_from_lines(output.lines());
        println!(
            "[IsisSshSource] fetch_hostname_map: built hostname map ({} entries)",
            map.len()
        );
        Ok(map)
    }

    async fn fetch_json_lspdb(&self) -> Result<JsonLspdb, AcquisitionError> {
        println!("[IsisSshSource] fetch_json_lspdb: start");
        if !self.client.is_connected() {
            println!("[IsisSshSource] fetch_json_lspdb: client not connected");
            return Err(AcquisitionError::Transport(
                "SSH client is not connected".to_string(),
            ));
        }

        let output = self
            .client
            .execute_command("vtysh -c 'show isis database detail json'")
            .await
            .map_err(|e| AcquisitionError::Transport(format!("Failed to retrieve LSPDB: {}", e)))?;
        println!(
            "[IsisSshSource] fetch_json_lspdb: received output size {}",
            output.len()
        );

        let mut lspdb = JsonLspdb::from_string(&output)
            .map_err(|e| AcquisitionError::Invalid(format!("Failed to parse JSON LSPDB: {}", e)))?;
        // Count total LSPs
        let total_lsps: usize = lspdb
            .areas
            .iter()
            .flat_map(|a| a.levels.iter())
            .map(|l| l.lsps.len())
            .sum();
        println!(
            "[IsisSshSource] fetch_json_lspdb: parsed JsonLspdb with {} areas and {} total lsps",
            lspdb.areas.len(),
            total_lsps
        );

        // Optional truncation for diagnostics/testing to avoid running the full parser
        if let Ok(max_str) = env::var("ISIS_MAX_LSPS") {
            if let Ok(max) = max_str.parse::<usize>() {
                if max > 0 {
                    println!(
                        "[IsisSshSource] fetch_json_lspdb: ISIS_MAX_LSPS={} set, truncating to max",
                        max
                    );
                    let mut removed = 0usize;
                    'outer: for area in &mut lspdb.areas {
                        for level in &mut area.levels {
                            while !level.lsps.is_empty() && total_lsps.saturating_sub(removed) > max
                            {
                                level.lsps.pop();
                                removed += 1;
                                if total_lsps.saturating_sub(removed) <= max {
                                    break 'outer;
                                }
                            }
                        }
                    }
                    println!(
                        "[IsisSshSource] fetch_json_lspdb: truncated, removed {} lsps",
                        removed
                    );
                }
            } else {
                println!(
                    "[IsisSshSource] fetch_json_lspdb: invalid ISIS_MAX_LSPS value '{}'",
                    max_str
                );
            }
        }

        Ok(lspdb)
    }

    async fn fetch_source_id(&self) -> Result<SourceId, AcquisitionError> {
        println!("[IsisSshSource] fetch_source_id: start");
        if !self.client.is_connected() {
            println!("[IsisSshSource] fetch_source_id: client not connected");
            return Err(AcquisitionError::Transport(
                "SSH client is not connected".to_string(),
            ));
        }

        let output = self
            .client
            .execute_command("vtysh -c 'show isis hostname'")
            .await
            .map_err(|e| {
                AcquisitionError::Transport(format!("Failed to retrieve source ID: {}", e))
            })?;

        println!(
            "[IsisSshSource] fetch_source_id: got output size {}",
            output.len()
        );

        let hostname_map = HostnameMap::build_map_from_lines(output.lines());
        let source_id = hostname_map.iter_entries()
            .filter(|entry| entry.is_local)
            .map(|entry| entry.system_id.clone())
            .next()
            .ok_or(AcquisitionError::Transport("No local system ID found".to_string()))?;

        Ok(SourceId::IsIs(source_id.clone()))
    }
    
    async fn fetch_if_id_to_stats(&self) -> Result<HashMap<u64, Stats>, AcquisitionError> {
        let cmd_output = self.client
            .execute_command("ip -j -s link show").await
            .map_err(|err| AcquisitionError::Transport(err.to_string()))?;
        println!("Stats cmd output: {}", cmd_output);
        let json: serde_json::Value = serde_json::from_str(&cmd_output)
            .map_err(|err| AcquisitionError::Invalid(err.to_string()))?;
        
        if let serde_json::Value::Array(interfaces) = json {
            let mut if_id_to_stats = HashMap::new();
            
            for interface in interfaces {
                if let serde_json::Value::Object(if_obj) = interface {
                    let id: u64 = if let Some(id) = if_obj.get("ifindex") {
                        id.as_u64().ok_or(AcquisitionError::Invalid("Invalid ifindex".to_string()))?
                    } else {
                        return Err(AcquisitionError::Invalid("Missing ifindex".to_string()));
                    };
                    
                    let stats = if let Some(serde_json::Value::Object(stats_map)) = if_obj.get("stats64") {
                        let mut stats = Stats {
                            rx_packets: 0,
                            tx_packets: 0,
                            rx_bytes: 0,
                            tx_bytes: 0,
                        };
                        
                        if let Some(serde_json::Value::Object(rx)) = stats_map.get("rx") {
                            let bytes = rx.get("bytes").and_then(|v| v.as_u64());
                            stats.rx_bytes = bytes.unwrap_or(0);
                            
                            let packets = rx.get("packets").and_then(|v| v.as_u64());
                            stats.rx_packets = packets.unwrap_or(0);
                        }
                        
                        if let Some(serde_json::Value::Object(tx)) = stats_map.get("tx") {
                            let bytes = tx.get("bytes").and_then(|v| v.as_u64());
                            stats.tx_bytes = bytes.unwrap_or(0);
                            
                            let packets = tx.get("packets").and_then(|v| v.as_u64());
                            stats.tx_packets = packets.unwrap_or(0);
                        }
                        
                        stats
                    } else {
                        return Err(AcquisitionError::Invalid("Missing stats64".to_string()));
                    };
                    
                    if_id_to_stats.insert(id, stats);
                }
            }
            return Ok(if_id_to_stats);
        } else {
            Err(AcquisitionError::Invalid("Missing stats64".to_string()))
        }
    }
    
    async fn fetch_if_id_to_ip(&self) -> Result<HashMap<u64, IpAddr>, AcquisitionError> {
        use serde_json::Value;
        let cmd_output = self.client
            .execute_command("vtysh -c 'show int json'")
            .await
            .map_err(|e| AcquisitionError::Invalid(format!("Failed to execute command: {}", e)))?;
        
        println!("IP command output: {}", cmd_output);
        
        let json: Value = serde_json::from_str(&cmd_output)
            .map_err(|e| AcquisitionError::Invalid(format!("Failed to parse JSON: {}", e)))?;
        
        if let Value::Object(interfaces) = json {
            let mut if_id_to_ip_map = HashMap::new();
            
            for if_obj in interfaces.values() {
                if let Value::Object(if_details) = if_obj {
                    let id = if_details.get("index").and_then(|v| v.as_u64());
                    dbg!(&id);
                    let mut ip = if let Some(Value::Array(ips)) = if_details.get("ipAddresses") {
                        let primary_ip_objs: Vec<_> = ips.iter().filter_map(|ip| {
                            if let Value::Object(ip_obj) = ip {
                                if let Some(Value::Bool(secondary)) = ip_obj.get("secondary") {
                                    if !secondary {
                                        Some(ip_obj)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }).collect();
                        
                        if let Some(primary_ip_obj) = primary_ip_objs.first() {
                            if let Some(Value::String(ip)) = primary_ip_obj.get("address") {
                                dbg!(ip);
                                let ip = ip.split('/').next().unwrap().to_string();
                                Some(ip.parse::<IpAddr>().map_err(|_| AcquisitionError::Invalid(format!("Invalid IP address: {}", ip)))?)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                        
                    } else {
                        None
                    };
                    
                    if let Some(Value::String(if_type)) = if_details.get("type") {
                        if if_type == "Loopback" {
                            ip = Some("127.0.0.1".parse::<IpAddr>().unwrap())
                        }
                    }
                    
                    if let (Some(id), Some(ip)) = (id, ip) {
                        if_id_to_ip_map.insert(id, ip);
                    } else {
                        return Err(AcquisitionError::Invalid("Invalid JSON format".to_string()))
                    }
                }
            }
            
            Ok(if_id_to_ip_map)
        } else {
            Err(AcquisitionError::Invalid("Invalid JSON format".to_string()))
        }
    }
}

#[derive(Debug)]
struct Stats {
    rx_packets: u64,
    tx_packets: u64,
    rx_bytes: u64,
    tx_bytes: u64,
}

#[async_trait]
impl AcquisitionSource<JsonIsisProtocol> for IsisSshSource {
    async fn fetch_raw(&mut self) -> Result<Vec<JsonLspdb>, AcquisitionError> {
        println!("[IsisSshSource] fetch_raw: start");
        let lspdb = self.fetch_json_lspdb().await?;
        println!("[IsisSshSource] fetch_raw: returning 1 JsonLspdb");
        Ok(vec![lspdb])
    }

    async fn fetch_source_id(&mut self) -> Result<SourceId, AcquisitionError> {
        // IMPORTANT: call the inherent method explicitly to avoid accidental recursion.
        // We have an inherent async method `fetch_source_id(&self)` above; call it with an explicit receiver.
        println!("[IsisSshSource] trait fetch_source_id: delegating to inherent method");
        IsisSshSource::fetch_source_id(&*self).await
    }
    
    async fn fetch_stats(&mut self) -> Result<Vec<InterfaceStats>, AcquisitionError> {
        let if_id_to_stats = self.fetch_if_id_to_stats().await?;
        dbg!(&if_id_to_stats);
        let if_id_to_ip = self.fetch_if_id_to_ip().await?;
        dbg!(&if_id_to_ip);
        let mut stats = Vec::new();
        
        for (if_id, ip_address) in if_id_to_ip {
            if let Some(if_stats) = if_id_to_stats.get(&if_id) {
                stats.push(InterfaceStats {
                    ip_address,
                    rx_bytes: Some(if_stats.rx_bytes),
                    tx_bytes: Some(if_stats.tx_bytes),
                    rx_packets: Some(if_stats.rx_packets),
                    tx_packets: Some(if_stats.tx_packets),
                });
            }
        }
        
        Ok(stats)
    }
}

mod tests {
    use crate::data_aquisition::ssh::SshError;

    use super::*;
    
    #[allow(unused)]
    async fn get_r1_ssh_client() -> Result<SshClient, SshError> {
        let mut client = SshClient::new_with_password(
            "client".to_string(),
            "localhost".to_string(),
            "password".to_string(),
            2221,
        );

        client.connect().await?;

        Ok(client)
    }

    #[tokio::test]
    async fn test_fetch_hostname_map() {
        let client = get_r1_ssh_client().await.unwrap();

        let source = IsisSshSource::new(client);

        let hostname_map = source.fetch_hostname_map().await;

        println!("{:#?}", hostname_map);
        assert!(hostname_map.is_ok());
    }

    #[tokio::test]
    async fn test_fetch_source_id() {
        let client = get_r1_ssh_client().await.unwrap();

        let source = IsisSshSource::new(client);

        let source_id = source.fetch_source_id().await;

        println!("{:#?}", source_id);
        assert!(source_id.is_ok());

        let source_id = source_id.unwrap();
        if let SourceId::IsIs(net_addr) = source_id {
            println!("NetAddress: {}", net_addr);
        }
    }

    #[tokio::test]
    async fn test_fetch_lspdb() {
        let client = get_r1_ssh_client().await.unwrap();

        let source = IsisSshSource::new(client);

        let lspdb = source.fetch_json_lspdb().await;

        println!("{:#?}", lspdb);
        assert!(lspdb.is_ok());
    }
    
    #[tokio::test]
    async fn test_fetch_interface_stats() {
        let client = get_r1_ssh_client().await.unwrap();

        let mut source = IsisSshSource::new(client);

        let interface_stats = source.fetch_stats().await;

        println!("{:#?}", interface_stats);
        assert!(interface_stats.is_ok());
    }
}
