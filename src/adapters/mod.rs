pub mod packets;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingProtocol {
    Ospf,
    IsIs,
    Bgp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportProtocol {
    Snmp,
    Ssh,
    Grpc
}

#[async_trait::async_trait]
pub trait VendorAdapter {
    fn vendor_name(&self) -> &'static str;
    fn routing_protocol(&self) -> RoutingProtocol;
    fn transport_protocol(&self) -> TransportProtocol;

    async fn 
}

pub trait CanonicalRoutingPacket {
    fn routing_protocol(&self) -> RoutingProtocol;
}