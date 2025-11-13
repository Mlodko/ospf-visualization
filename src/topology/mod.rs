/*!
Topology module

This module defines the GUI-facing topology provider interface and concrete implementations.

Structure:
- `source`: A small async trait (`TopologySource`) that returns protocol-agnostic nodes,
            plus a minimal error type used by the GUI layer.
- `ospf`: Generic OSPF topology that consumes any `OspfDataSource` (with a
          convenience alias `OspfSnmpTopology` for SNMP).

Re-exports:
- `TopologySource`, `TopologyError`, and `TopologyResult` for easy consumption by callers.
- `OspfSnmpTopology` as the default OSPF-over-SNMP topology provider.
*/

pub mod ospf;
pub mod source;

pub use ospf::OspfSnmpTopology;
pub use source::{TopologySource};
