#![allow(dead_code)]

use std::net::Ipv4Addr;

use snmp2::{Oid, Value};

/// Represents raw data retrieved from a network device along with the protocol used to retrieve it.
pub enum RawRouterData<'a> {
    Snmp {
        oid: Oid<'a>,
        value: LinkStateValue,
    },
    Netconf(String),
    Restconf(String)
}

/// Represents a network client that can retrieve data from a network device.
pub trait NetworkClient {
    type Error: std::error::Error + Send + Sync + 'static;
    
    fn get_data_from_device(&self) -> Result<RawRouterData<'_>, Self::Error>; 
}

/// Replacement for the snmp2::Value type due to lifetime shenanigans
#[derive(Debug, Clone)]
pub enum LinkStateValue {
    // Core OSPF types
    Integer(i64),           // RouterID, AreaID, LSA types, sequence numbers, ages, metrics
    IpAddress(Ipv4Addr),     // IPv4 addresses (router IDs, interface addresses)
    OctetString(String),   // LSA advertisement data, authentication keys, opaque data
    
    // Counters and metrics  
    Counter32(u32),         // SPF runs, event counters, LSA counts
    
    // Time-related
    Timeticks(u32),         // LSA ages, hello intervals, dead intervals (in centiseconds)
    
    // Status and flags
    Boolean(bool),          // Admin status, capability flags, hello suppressed
    
    // Network and routing info
    Unsigned32(u32),        // Checksums, bandwidth values, route tags
    
    // Fallback
    Unknown,                // For anything else
}

impl From<&Value<'_>> for LinkStateValue {
    fn from(value: &Value) -> Self {
        match value {
            Value::Integer(i) => LinkStateValue::Integer(*i),
            Value::IpAddress(ip) => LinkStateValue::IpAddress(Ipv4Addr::from_bits(u32::from_ne_bytes(*ip))),
            Value::OctetString(s) => LinkStateValue::OctetString(String::from_utf8_lossy(s).to_string()),
            Value::Counter32(c) => LinkStateValue::Counter32(*c),
            Value::Timeticks(t) => LinkStateValue::Timeticks(*t),
            Value::Boolean(b) => LinkStateValue::Boolean(*b),
            Value::Unsigned32(u) => LinkStateValue::Unsigned32(*u),
            _ => LinkStateValue::Unknown,
        }
    }
}
