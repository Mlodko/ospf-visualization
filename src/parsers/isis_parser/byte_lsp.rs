#![allow(dead_code)]
/*!
This module defines Rust structures for representing Link State Packets (LSPs) in the IS-IS protocol.
*/

use std::fmt::Display;

use thiserror::Error;


pub struct IsisLspFlags {
    partition_repair: bool,
    attached: bool,
    overload: bool
}

impl From<u8> for IsisLspFlags {
    fn from(value: u8) -> Self {
        let partition_repair = value & 0b1000_0000 != 0;
        let attached = value & 0b0100_0000 != 0;
        let overload = value & 0b0010_0000 != 0;
        IsisLspFlags {
            partition_repair,
            attached,
            overload
        }
    }
}

impl IsisLspFlags {
    pub fn partition_repair(&self) -> bool {
        self.partition_repair
    }
    
    pub fn attached(&self) -> bool {
        self.attached
    }
    
    pub fn overload(&self) -> bool {
        self.overload
    }
}

pub struct IsisLspId {
    raw_id: [u8; 8],
}

impl IsisLspId {
    pub fn new(raw_id: [u8; 8]) -> Self {
        IsisLspId { raw_id }
    }
    
    pub fn get_raw_id(&self) -> &[u8] {
        &self.raw_id
    }
    
    pub fn get_system_id(&self) -> IsisSystemId {
        IsisSystemId::new(self.raw_id[0..6].try_into().expect("Slice out of bounds, this should never happen"))
    }
    
    pub fn get_pseudonode_id(&self) -> u8 {
        self.raw_id[6]
    }
}

pub struct IsisSystemId {
    raw_id: [u8; 6],
}

impl IsisSystemId {
    pub fn new(raw_id: [u8; 6]) -> Self {
        IsisSystemId { raw_id }
    }
}

pub enum IsisIsType {
    Level1,
    Level2,
    Level1Level2
}

impl From<u8> for IsisIsType {
    fn from(value: u8) -> Self {
        let level_1 = value & 0b0000_0001 != 0;
        let level_2 = value & 0b0000_0010 != 0;
        match (level_1, level_2) {
            (true, false) => IsisIsType::Level1,
            (false, true) => IsisIsType::Level2,
            (true, true) => IsisIsType::Level1Level2,
            _ => panic!("Invalid IS type"),
        }
    }
}

pub enum IsisLspTlv {
    // To be populated with actual TLVs
    // Each TLV type should be a variant of this enum that contains a specific TLV struct
    AreaAddresses(AreaAddressesTlv)
}

pub struct AreaAddressesTlv {
    addresses: Vec<IsisAreaAddress>
}

pub struct IsisAreaAddress {
    raw_address: Vec<u8>
}

impl Display for IsisAreaAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format as groups of 2 bytes (4 hex digits) separated by dots
        let chunks = self.raw_address.chunks(2)
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

#[derive(Debug, Error)]
pub enum IsisError {
    #[error("Invalid area address: {0:?}")]
    InvalidAreaAddress(Vec<u8>)
}

impl TryFrom<&[u8]> for IsisAreaAddress {
    type Error = IsisError;
    fn try_from(raw_address: &[u8]) -> Result<Self, Self::Error> {
        IsisAreaAddress::new(raw_address)
    }
}

impl IsisAreaAddress {
    pub fn new(raw_address: &[u8]) -> Result<Self, IsisError> {
        if raw_address.len() > 13 {
            Err(IsisError::InvalidAreaAddress(raw_address.to_vec()))
        } else {
            Ok(IsisAreaAddress { raw_address: raw_address.to_vec() })
        }
    }
}

pub struct IsisLspHeader {
    pdu_length: u16,
    remaining_lifetime: u16,
    lsp_id: IsisLspId,
    sequence_number: u32,
    checksum: u16,
    flags: IsisLspFlags,
    is_type: IsisIsType,
    tlvs: Vec<IsisLspTlv>
}