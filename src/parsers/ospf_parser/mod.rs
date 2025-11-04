use crate::{data_aquisition::snmp::SnmpClientError, parsers::ospf_parser::lsa::LsaError};

/*
This module handles converting raw router data into semantic OSPF data structures, 
and then turning them into a protocol-agnostic graph format (network module)

So basically

--- data_aquisition module ---
SNMP/RESTCONF
|
| Raw router data, data_aquisition doesn't care about the routing protocol 
v
--- ospf_parser module ---
Semantic OSPF data structures from ospf-parser crate
|
| (this transition should still happen in ospf_parser, network only touches the protocol-agnostic graph)
v
--- network module ---
Protocol-agnostic graph format 
|
|
v
--- gui module ---
User interface for visualizing the graph
*/
pub mod lsa;
pub mod snmp;

#[derive(Debug)]
pub enum OspfError {
    Lsa(LsaError),
    Snmp(SnmpClientError)
}