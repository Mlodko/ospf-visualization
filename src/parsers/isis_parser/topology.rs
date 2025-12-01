use crate::{data_aquisition::ssh::SshClient, parsers::isis_parser::{protocol::JsonIsisProtocol, ssh_source::IsisSshSource}, topology::protocol::{AcquisitionError, Topology}};


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