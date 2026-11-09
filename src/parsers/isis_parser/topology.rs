use crate::{
    data_acquisition::transport::grpc::client::GnmiClient,
    parsers::isis_parser::protocol::JsonIsisProtocol,
    topology::{protocol::RoutingMessageTopology, routing_source::GnmiRoutingMessageSource},
};

pub type NokiaGnmiIsisTopology = RoutingMessageTopology<
    JsonIsisProtocol,
    GnmiRoutingMessageSource<crate::adapters::vendors::nokia::gnmi_isis::NokiaGnmiIsisAdapter>,
>;

impl NokiaGnmiIsisTopology {
    pub fn from_gnmi_client(
        client: GnmiClient,
        source_id: crate::network::router::RouterId,
        interface_stats: Vec<crate::network::router::InterfaceStats>,
    ) -> Self {
        let source = GnmiRoutingMessageSource::new(
            client,
            crate::adapters::vendors::nokia::gnmi_isis::NokiaGnmiIsisAdapter,
            source_id,
            interface_stats,
        );
        Self::new(
            JsonIsisProtocol::new(crate::parsers::isis_parser::hostname::HostnameMap::empty()),
            source,
        )
    }
}
