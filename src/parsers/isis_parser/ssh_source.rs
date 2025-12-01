use async_trait::async_trait;

use crate::{
    data_aquisition::ssh::SshClient,
    parsers::isis_parser::{
        core_lsp::NetAddress, frr_json_lsp::JsonLspdb, hostname::HostnameMap,
        protocol::JsonIsisProtocol,
    },
    topology::{
        protocol::{AcquisitionError, AcquisitionSource},
        store::SourceId,
    },
};
use std::env;

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
}
