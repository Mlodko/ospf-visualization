use crate::data_acquisition::transport::grpc::prelude::*;

pub mod transport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportProtocol {
    Snmp,
    Ssh,
    Grpc,
}

#[derive(Debug, Clone)]
pub enum TransportPayload {
    Snmp(transport::snmp::PortableSnmpTableRow),
    Ssh(String),
    GnmiGet(Vec<JsonIetfNotification>),
    GnmiSub(JsonIetfSubscriptionEvent),
}
