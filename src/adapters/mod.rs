pub mod packets;
pub mod vendors;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingProtocol {
    Ospf,
    IsIs,
    Bgp,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RoutingMessage {
    Ospf(packets::ospf::lsa::Lsa),
    IsIs(packets::isis::Lsp),
    Bgp(),
}

pub trait VendorAdapter {
    fn vendor_name(&self) -> &'static str;
    fn routing_protocol(&self) -> RoutingProtocol;
    fn transport_protocol(&self) -> crate::data_acquisition::TransportProtocol;
    fn subscribtion_paths(&self) -> Vec<String>;

    fn parse_transport_payload(&self, payload: crate::data_acquisition::TransportPayload) -> Result<Vec<RoutingMessage>, anyhow::Error>;
}