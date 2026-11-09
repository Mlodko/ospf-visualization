use async_trait::async_trait;

use crate::{
    adapters::{RoutingMessage, VendorAdapter},
    data_acquisition::{
        TransportPayload,
        transport::grpc::{client::GnmiClient, types::JsonIetfNotification},
    },
    network::router::{InterfaceStats, RouterId},
    topology::{protocol::AcquisitionError, source::RoutingMessageSource},
};

/// gNMI-backed source of canonical routing messages.
pub struct GnmiRoutingMessageSource<A> {
    client: GnmiClient,
    adapter: A,
    source_id: RouterId,
    interface_stats: Vec<InterfaceStats>,
}

impl<A> GnmiRoutingMessageSource<A> {
    pub fn new(
        client: GnmiClient,
        adapter: A,
        source_id: RouterId,
        interface_stats: Vec<InterfaceStats>,
    ) -> Self {
        Self {
            client,
            adapter,
            source_id,
            interface_stats,
        }
    }

    pub fn adapter(&self) -> &A {
        &self.adapter
    }
}

#[async_trait]
impl<A> RoutingMessageSource for GnmiRoutingMessageSource<A>
where
    A: VendorAdapter + Send + Sync,
{
    async fn fetch_messages(&mut self) -> Result<Vec<RoutingMessage>, AcquisitionError> {
        let paths = self.adapter.subscribtion_paths();
        let path_refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        let response = self
            .client
            .get(&path_refs)
            .await
            .map_err(|error| AcquisitionError::Transport(error.to_string()))?;
        let notifications: Vec<JsonIetfNotification> = (&response)
            .try_into()
            .map_err(|error: anyhow::Error| AcquisitionError::Invalid(error.to_string()))?;

        self.adapter
            .parse_transport_payload(TransportPayload::GnmiGet(notifications))
            .map_err(|error| AcquisitionError::Invalid(error.to_string()))
    }

    async fn fetch_source_id(&mut self) -> Result<RouterId, AcquisitionError> {
        Ok(self.source_id.clone())
    }

    async fn fetch_stats(&mut self) -> Result<Vec<InterfaceStats>, AcquisitionError> {
        Ok(self.interface_stats.clone())
    }
}
