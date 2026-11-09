/*!
Topology module

This module defines the GUI-facing topology provider interface and concrete implementations.

Structure:
- `source`: A small async trait (`TopologySource`) that returns protocol-agnostic nodes,
            plus a minimal error type used by the GUI layer.
- `ospf_protocol`: Active gNMI topology plus deprecated SNMP compatibility aliases.

Re-exports:
- `TopologySource`, `TopologyError`, and `TopologyResult` for easy consumption by callers.
- `NokiaGnmiOspfTopology` as the primary OSPF topology provider.
*/

pub mod ospf_bfs_protocol;
pub mod ospf_protocol;
pub mod protocol;
pub mod routing_source;
pub mod source;
pub mod store;

pub use ospf_protocol::NokiaGnmiOspfTopology;
pub use source::TopologySource;
