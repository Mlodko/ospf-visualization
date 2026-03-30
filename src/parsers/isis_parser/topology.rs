use crate::{data_aquisition::ssh::SshClient, parsers::isis_parser::{bfs_protocol::JsonIsisBfsProtocol, protocol::JsonIsisProtocol, ssh_source::IsisSshSource}, topology::protocol::{AcquisitionError, Topology}};


pub type IsIsTopology = Topology<JsonIsisProtocol, IsisSshSource>;

impl IsIsTopology {
    pub async fn new_from_ssh_client(mut client: SshClient) -> Result<Self, AcquisitionError> {
        if !client.is_connected() {
            client.connect().await.map_err(|e| AcquisitionError::Transport(format!("Couldn't connect to SSH client: {}", e)))?;
        }
        
        let source = IsisSshSource::new(client);
        
        let hostname_map = source.fetch_hostname_map().await?;
        
        let protocol = JsonIsisProtocol::new(hostname_map);
        
        let topology = Topology::new(protocol, source);
        
        Ok(topology)
    }
}

pub type IsIsBfsTopology = Topology<JsonIsisBfsProtocol, IsisSshSource>;

impl IsIsBfsTopology {
    pub async fn new_from_ssh_client(mut client: SshClient) -> Result<Self, AcquisitionError> {
        if !client.is_connected() {
            client.connect().await.map_err(|e| AcquisitionError::Transport(format!("Couldn't connect to SSH client: {}", e)))?;
        }
        
        let source = IsisSshSource::new(client);
        
        let hostname_map = source.fetch_hostname_map().await?;
        
        let protocol = JsonIsisBfsProtocol::new(hostname_map);
        
        let topology = Topology::new(protocol, source);
        
        Ok(topology)
    }
}

mod tests {
    use crate::topology::source::SnapshotSource;

    use super::*;
    
    fn new_r1_client() -> SshClient {
        SshClient::new_with_password(
            "client".to_string(),
            "localhost".to_string(),
            "password".to_string(),
            2221)
    }
    
    #[tokio::test]
    async fn test_isis_bfs_topology() {
        let client = new_r1_client();
        let mut topology = IsIsBfsTopology::new_from_ssh_client(client).await.unwrap();
        
        let snapshot = topology.fetch_snapshot().await;
        
        assert!(snapshot.is_ok());
    }
}
