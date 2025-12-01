/*!
This file defines the authoritative LSP structs that will exclusively be used to construct Nodes and Edges.
*/

use std::{collections::HashMap, fmt::Display, net::Ipv4Addr};

use ipnetwork::IpNetwork;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LspError {
    #[error("Invalid system ID: {0}")]
    InvalidSystemId(String),
    #[error("Invalid IP prefix: {0}")]
    InvalidIpPrefixOrAddress(String),
    #[error("Invalid LSP ID: {0}")]
    InvalidLspId(String),
    #[error("Missing data: {0}")]
    MissingData(String),
    #[error("Invalid IS level: {0}")]
    InvalidIsLevel(u32),
    #[error("Bad data format in {0}: {1}")]
    BadDataFormat(String, String),
    #[error("Invalid NET address: {0}")]
    InvalidNetAddress(String),
}

/// Represents a single IS-IS Link State PDU (LSP) as advertised by a router.
/// This struct contains only the protocol fields common to all LSPs,
/// and a vector of TLVs (Type-Length-Value) describing the LSP's contents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lsp {
    /// The unique identifier for this LSP (System ID + Pseudonode ID + fragment).
    pub lsp_id: LspId,
    /// The system ID of the originating router.
    pub system_id: SystemId,
    /// The IS-IS level (L1, L2, or both) this LSP belongs to.
    pub is_level: IsLevel,
    /// The sequence number of the LSP (used for versioning).
    pub sequence_number: Option<String>,
    /// Remaining lifetime of the LSP in seconds.
    pub holdtime: Option<String>,
    /// Whatever that means...
    pub area_addr: Option<AreaAddress>,
    /// The list of TLVs (Type-Length-Value) contained in this LSP.
    pub tlvs: Vec<Tlv>,
}

impl Lsp {
    pub fn new(
        lsp_id: LspId,
        system_id: SystemId,
        is_level: IsLevel,
        sequence_number: Option<String>,
        holdtime: Option<String>,
        area_addr: Option<AreaAddress>,
        tlvs: Vec<Tlv>
    ) -> Self {
        Self {
            lsp_id,
            system_id,
            is_level,
            sequence_number,
            holdtime,
            area_addr,
            tlvs
        }
    }
    
    pub fn get_net_address(&self) -> Option<NetAddress> {
        println!("get_net_address called");
        if let Some(Tlv::AreaAddresses(t)) = self.get_tlvs_by(|t| matches!(t, Tlv::AreaAddresses(_))).first() {
            println!("AreaAddresses TLV found");
            t.addresses.first().map(|area| {
                NetAddress { area_address: area.clone(), system_id: self.system_id.clone() }
            })
        } else if let Some(area_addr) = &self.area_addr {
            println!("AreaName found");
            let net_addr_string = format!("{}.{}", area_addr, self.system_id);
            let net = NetAddress::from_str(&net_addr_string).ok();
            
            if net.is_none() {
                println!("Invalid NetAddress: {}", &net_addr_string);
            }
            
            net
            
        } else {
            println!("No area address found");
            None
        }
    }
    
    pub fn get_tlvs_by<F>(&self, pred: F) -> Vec<&Tlv> 
    where 
        F: Fn(&Tlv) -> bool,
    {
        self.tlvs.iter()
            .filter(|t| pred(t))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetAddress {
    pub area_address: AreaAddress,
    pub system_id: SystemId
}

impl NetAddress {
    pub fn to_vec(&self) -> Vec<u8> {
        let mut vec = Vec::new();
        vec.extend_from_slice(&self.area_address.raw_address);
        vec.extend_from_slice(&self.system_id.raw_id);
        vec
    }
    
    pub fn from_str(net_address: &str) -> Result<Self, LspError> {
        let mut parts: Vec<&str> = net_address.split(".").collect();
        
        if parts.is_empty() {
            return Err(LspError::InvalidNetAddress(net_address.to_string()));
        }
        
        // Strip AFI and NSEG, check values
        
        let afi = parts.remove(0);
        if afi != "49" {
            return Err(LspError::InvalidNetAddress(net_address.to_string()));
        }
        
        let nseg = parts.pop();
        if nseg != Some("00") {
            return Err(LspError::InvalidNetAddress(net_address.to_string()));
        }
        
        // Pop the last 3 parts, that's the system ID
        // the rest is the area address
        
        if parts.len() < 3 {
            return Err(LspError::InvalidNetAddress(net_address.to_string()));
        }
        
        let sys_parts = parts.split_off(parts.len() - 3);
        let system_id = SystemId::from_string(&sys_parts.join("."))?;
        
        let mut area_bytes: Vec<u8> = Vec::new();
        for raw_part in parts {
            let part = raw_part.trim();
            if part.is_empty() {
                continue;
            }
            let chunk = if part.len() % 2 != 0 {
                format!("0{}", part)
            } else {
                part.to_string()
            };
            let mut decoded = hex::decode(&chunk)
                .map_err(|_| LspError::InvalidNetAddress(net_address.to_string()))?;
            area_bytes.append(&mut decoded);
        }

        if area_bytes.is_empty() {
            return Err(LspError::InvalidNetAddress(net_address.to_string()));
        }

        let area_address = AreaAddress { raw_address: area_bytes };
        
        Ok(NetAddress { area_address, system_id })  
    }
}

impl Display for NetAddress{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.00", self.area_address, self.system_id)
    }
}

/// Enum representing all supported TLVs that may appear in an IS-IS LSP.
/// Each variant holds a struct or value specific to that TLV type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Tlv {
    /// TLV #1: Area Addresses — identifies the IS-IS areas this router belongs to.
    AreaAddresses(AreaAddressesTlv),
    /// TLV #2: IS Reachability — describes neighbors and link metrics.
    IsReachability(IsReachabilityTlv),
    /// TLV #137: Hostname — the router's human-readable name.
    Hostname(String),
    /// TLV #128: IP Reachability — IPv4 prefixes directly connected or redistributed by this router.
    IpReachability(IpReachabilityTlv),
    /// TLV #22: Extended IS Reachability — describes neighbors and link metrics.
    ExtendedReachability(IsExtendedReachabilityTlv),
    /// TLV #242: Router Capability — router's capabilities (e.g., TE Router ID, flags).
    RouterCapability(RouterCapabilityTlv),
    /// TLV #135: 
    ExtendedIpReachability(ExtendedIpReachabilityTlv)
}

impl Tlv {
    pub fn get_name(&self) -> &str {
        match self {
            Tlv::AreaAddresses(_) => "#1 Area Addresses",
            Tlv::IsReachability(_) => "#2 IS Reachability",
            Tlv::Hostname(_) => "#137 Hostname",
            Tlv::IpReachability(_) => "#128 IP Reachability",
            Tlv::ExtendedReachability(_) => "#22 Extended IS Reachability",
            Tlv::RouterCapability(_) => "#242 Router Capability",
            Tlv::ExtendedIpReachability(_) => "#135 Extended IP Reachability",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedIpReachabilityTlv {
    pub neighbors: Vec<ExtendedIpReachabilityNeighbor>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedIpReachabilityNeighbor {
    pub metric: u32,
    /// Up is true, down is false
    pub up_down: bool,
    pub prefix: IpNetwork
}

impl ExtendedIpReachabilityNeighbor {
    pub fn new(prefix: IpNetwork, metric: u32, is_up: bool) -> Self {
        Self {
            prefix,
            metric,
            up_down: is_up
        }
    }
}

/// TLV #242: Router Capability — describes additional capabilities of the router,
/// such as Traffic Engineering Router ID and protocol-specific flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterCapabilityTlv {
    /// Traffic Engineering Router ID (if present).
    pub te_router_id: Option<Ipv4Addr>,
    /// Arbitrary capability flags (e.g., SR, FAD, etc.), keyed by flag name.
    pub flags: HashMap<String, bool>,
}

/// TLV #2: IS Reachability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsReachabilityTlv {
    neighbors: Vec<IsNeighbor>
}

impl IsReachabilityTlv {
    pub fn neighbors_iter(&self) -> impl Iterator<Item = &IsNeighbor> {
        self.neighbors.iter()
    }
}

/// TLV #128: IP Reachability — lists IPv4 prefixes reachable via this router.
/// Typically represents directly connected networks or redistributed routes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpReachabilityTlv {
    /// List of IPv4 prefixes and their associated metrics and up/down status.
    prefixes: Vec<Prefix>,
}

/// TLV #1: Area Addresses — lists all IS-IS area addresses this router belongs to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AreaAddressesTlv {
    /// List of area addresses (variable-length, protocol-specific encoding).
    pub addresses: Vec<AreaAddress>,
}

impl AreaAddressesTlv {
    pub fn new(addresses: Vec<AreaAddress>) -> Self {
        Self { addresses }
    }
}

/// TLV #22: Extended IS Reachability — describes a neighbor router and the metric to reach it.
/// Each instance typically represents a single adjacency.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsExtendedReachabilityTlv {
    pub neighbors: Vec<ExtendedIsNeighbor>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedIsNeighbor {
    /// The System ID of the neighboring router.
    pub neighbor_id: SystemId,
    /// The IS-IS metric for the link to this neighbor.
    pub metric: u32,
    /// Pseudonode ID, if this is equal to 0 it's not a pseudonode.
    pub pseudonode_id: u8,
}

/// Represents a neighbor relationship (adjacency) between routers.
/// Not directly a TLV, but useful for topology construction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsNeighbor {
    /// The System ID of the neighboring router.
    pub system_id: SystemId,
    /// The IS-IS metric for the link to this neighbor.
    pub metric: u32,
}

/// Represents an IPv4 prefix advertised in an LSP, along with its metric and up/down status.
/// Used in IP Reachability TLVs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prefix {
    /// The IPv4 prefix (address and mask).
    pub prefix: IpNetwork,
    /// The IS-IS metric for this prefix.
    pub metric: u32,
    /// True if the prefix is "up" (internal), false if "down" (external/leaked).
    pub up: bool,
}

/// Unique identifier for an LSP, consisting of the originating System ID,
/// pseudonode ID, and fragment number (8 bytes total).
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct LspId {
    raw_id: [u8; 8],
}

impl LspId {
    /// Constructs a new LSP ID from an 8-byte array.
    pub fn new(raw_id: [u8; 8]) -> Self {
        LspId { raw_id }
    }
    
    pub fn new_from(system_id: &SystemId, pseudonode_id: u8, fragment_number: u8) -> Self {
        let mut raw_id = [0u8; 8];
        raw_id[0..6].copy_from_slice(&system_id.raw_id);
        raw_id[6] = pseudonode_id;
        raw_id[7] = fragment_number;
        Self::new(raw_id)
    }
    
    pub fn is_pseudonode_of(&self, dis_id: &LspId) -> bool {
        self.is_pseudonode() &&
        !dis_id.is_pseudonode() &&
        self.raw_id[0..6] == dis_id.raw_id[0..6]
    }

    /// Returns the raw 8-byte LSP ID.
    #[allow(dead_code)]
    pub fn get_raw_id(&self) -> &[u8] {
        &self.raw_id
    }

    /// Extracts the System ID (first 6 bytes) from the LSP ID.
    pub fn get_system_id(&self) -> Result<SystemId, LspError> {
        println!("LspId::get_system_id returns {:?}", &self.raw_id[0..6]);
        SystemId::new(
            &self.raw_id[0..6]
        )
    }

    /// Returns the pseudonode ID (7th byte) from the LSP ID.    
    #[allow(dead_code)]
    pub fn get_pseudonode_id(&self) -> u8 {
        self.raw_id[6]
    }
    
    pub fn is_pseudonode(&self) -> bool {
        self.raw_id[6] != 0
    }
    #[allow(dead_code)]
    pub fn from_string_no_dots_in_sys_id(id_str: &str) -> Result<Self, LspError> {
        // Format
        // XXXXXXXXXXXX.XX-XX
        let sanitized = id_str.replace(".", "").replace("-", "");
        let bytes = hex::decode(sanitized).map_err(|_| LspError::InvalidLspId(id_str.to_string()))?;
        
        if bytes.len() < 8 {
            return Err(LspError::InvalidLspId(id_str.to_string()))
        }
        
        let raw_id: [u8; 8] = bytes[0..8].try_into().map_err(|_| LspError::InvalidLspId(id_str.to_string()))?;
        
        Ok(Self {
            raw_id
        })     
    }
    
    pub fn from_string(id_str: &str) -> Result<Self, LspError> {
        // LSP ID format
        // XXXX.XXXX.XXXX.XX-XX
        // ^^^^^^^^^^^^^^ ^^ ^^
        // ^^^^^^^^^^^^^^ ^^ fragment ID
        // System ID      0 => Router, else pseudonode
        
        let mut parts: Vec<String> = id_str.split(".").map(|s| s.to_string()).collect();
        let pseudonode_and_fragment = parts.pop().ok_or(LspError::InvalidLspId(id_str.to_string()))?;
        
        let mut id_bytes: Vec<u8> = Vec::with_capacity(8);
        for part in &parts {
            // Skip empty parts (defensive)
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            
            // Ensure even length hex string by padding a leading zero if needed
            let part = if part.len() % 2 != 0 {
                format!("0{}", part)
            } else {
                part.to_string()
            };
            
            let mut bytes = hex::decode(&part)
                .map_err(|_| LspError::InvalidLspId(id_str.to_string()))?;
            id_bytes.append(&mut bytes);
        }
        
        let mut sanitized_p_and_f: Vec<u8> = hex::decode(pseudonode_and_fragment.replace("-", ""))
            .map_err(|_| LspError::InvalidLspId(id_str.to_string()))?;
        id_bytes.append(&mut sanitized_p_and_f);
        
        if id_bytes.len() < 8 {
            return Err(LspError::InvalidLspId(id_str.to_string()))
        }
        
        let raw_id: [u8; 8] = id_bytes[0..8].try_into().map_err(|_| LspError::InvalidLspId(id_str.to_string()))?;
        
        Ok(Self {
            raw_id
        })      
    }
}

impl Display for LspId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format: XXXX.XXXX.XXXX.XX-XX
        let bytes_str: Vec<String> = self.raw_id.iter().map(|byte| format!("{:02X}", byte)).collect();
        let system_id_parts: Vec<String> = bytes_str[..6].chunks(2).map(|part| part.join("")).collect();
        let system_id = system_id_parts.join(".");
        let rest = bytes_str[6..].join("-");
        write!(f, "{}.{}", system_id, rest)
    }
}

/// Unique identifier for an IS-IS router (System ID, 6 bytes).
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct SystemId {
    raw_id: [u8; 6],
}

impl Display for SystemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let parts: Vec<String> = self.raw_id.chunks(2)
            .map(|part| {
                hex::encode(part)
            })
            .collect();
        write!(f, "{}", parts.join("."))
    }
}

impl SystemId {
    pub fn new(id_bytes: &[u8]) -> Result<Self, LspError> {
        println!("SystemId::new received {:?}", id_bytes);
        if id_bytes.len() != 6 {
            Err(LspError::InvalidSystemId(format!("{:?}", id_bytes)))
        } else {
            let mut raw_id = [0u8; 6];
            raw_id.copy_from_slice(id_bytes);
            Ok(SystemId { raw_id })
        }
    }
    
    pub fn to_vec(&self) -> Vec<u8> {
        self.raw_id.to_vec()
    }
    
    pub fn from_string(id_str: &str) -> Result<Self, LspError> {
        let mut id_bytes: Vec<u8> = Vec::new();

        for raw_part in id_str.split('.') {
            // Skip empty parts (defensive)
            let part = raw_part.trim();
            if part.is_empty() {
                continue;
            }

            // Ensure even length hex string by padding a leading zero if needed
            let part = if part.len() % 2 != 0 {
                format!("0{}", part)
            } else {
                part.to_string()
            };

            // Decode the hex chunk and append bytes to the result vector
            let mut bytes = hex::decode(&part)
                .map_err(|_| LspError::InvalidSystemId(id_str.to_string()))?;
            id_bytes.append(&mut bytes);
        }

        if id_bytes.len() < 6 {
            return Err(LspError::InvalidSystemId(id_str.to_string()));
        }

        Ok(SystemId {
            raw_id: id_bytes[..6].try_into()
                .map_err(|_| LspError::InvalidSystemId(id_str.to_string()))?
        })
    }
}



/// Represents an IS-IS area address (variable-length, protocol-specific encoding).
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct AreaAddress {
    pub raw_address: Vec<u8>,
}

impl Display for AreaAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format as groups of 2 bytes (4 hex digits) separated by dots
        let chunks = self
            .raw_address
            .chunks(2)
            .map(|chunk| {
                if chunk.len() == 2 {
                    format!("{:02X}{:02X}", chunk[0], chunk[1])
                } else {
                    // Odd-length: last byte alone
                    format!("{:02X}", chunk[0])
                }
            })
            .collect::<Vec<_>>();
        write!(f, "{}", chunks.join("."))
    }
}

/// Indicates the IS-IS level at which an LSP or adjacency operates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IsLevel {
    /// Level 1 (intra-area)
    Level1,
    /// Level 2 (inter-area/backbone)
    Level2,
    /// Both Level 1 and Level 2 (rare, but possible)
    Level1And2,
}

mod tests {
    #[allow(unused_imports)]
    use super::*;
    #[test]
    fn test_lsp_id_from_string() {
        let str = "0000.0000.0001.00-00";
        let id = LspId::from_string(str);
        assert!(id.is_ok());
        _ = dbg!(id);
    }
}
