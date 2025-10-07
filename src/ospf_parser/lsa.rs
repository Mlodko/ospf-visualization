use std::{net::Ipv4Addr, time::Duration};

pub enum LsaError {
    InvalidLength,
    MalformedData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawLsa {
    ls_age: u16,
    options: u8,
    ls_type: u8,
    ls_id: u32,
    advertising_router: u32,
    ls_sequence_number: u32,
    ls_checksum: u16,
    ls_length: u16,
    ls_body: Vec<u8>,
}

struct LsaOptions {}

impl From<u8> for LsaOptions {
    fn from(value: u8) -> Self {
        todo!()
    }
}

pub struct LsaHeader {
    age: Duration,
    options: LsaOptions,
    ls_id: Ipv4Addr,
    advertising_router: Ipv4Addr,
    sequence_number: u32,
    checksum: u16,
}

pub enum LsaBody {
    // TODO expand enum variants to hold the info from the LSA body
    Router(RouterLsa),    // Type 1
    Network,   // Type 2
    SummaryIp, // Type 3
    SummaryAs, // Type 4
    External,  // Type 5
    Unknown,   // Type 6+
}

pub struct RouterLsa {
    
}

pub struct Lsa {
    header: LsaHeader,
    body: LsaBody,
}

impl LsaBody {
    fn from_bytes(bytes: &[u8], ls_type: u8) -> Result<Self, LsaError> {
        // TODO implement parsing logic for each LSA type
        todo!()
    }
}

impl TryFrom<Vec<u8>> for LsaBody {
    type Error = LsaError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl TryFrom<Vec<u8>> for RawLsa {
    type Error = LsaError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        if bytes.len() < 20 {
            return Err(Self::Error::InvalidLength);
        }

        let ls_age = u16::from_be_bytes([bytes[0], bytes[1]]);
        let options = bytes[2];
        let ls_type = bytes[3];
        let ls_id = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let advertising_router = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let ls_sequence_number = u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
        let ls_checksum = u16::from_be_bytes([bytes[16], bytes[17]]);
        let ls_length = u16::from_be_bytes([bytes[18], bytes[19]]);
        let ls_body = bytes[20..].to_vec();

        Ok(RawLsa {
            ls_age,
            options,
            ls_type,
            ls_id,
            advertising_router,
            ls_sequence_number,
            ls_checksum,
            ls_length,
            ls_body,
        })
    }
}

impl Lsa {
    fn from_raw(raw: RawLsa) -> Result<Self, LsaError> {
        let header = LsaHeader {
            age: Duration::from_secs(raw.ls_age as u64),
            options: LsaOptions::from(raw.options),
            ls_id: Ipv4Addr::from_bits(raw.ls_id),
            advertising_router: Ipv4Addr::from_bits(raw.advertising_router),
            sequence_number: raw.ls_sequence_number,
            checksum: raw.ls_checksum,
        };

        let body = LsaBody::from_bytes(&raw.ls_body, raw.ls_type)?;

        Ok(Lsa { header, body })
    }
}
