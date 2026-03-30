use std::fmt::Display;

use nom::number::complete::{be_u8, be_u16};
use nom::{IResult, number::complete::be_u32};

use super::super::span::{Field, Span};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LinkStateId(pub u32);

impl Display for LinkStateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ipv4 = std::net::Ipv4Addr::from(self.0.to_be_bytes());
        write!(f, "{}", ipv4)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RouterId(pub u32);

impl Display for RouterId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ipv4 = std::net::Ipv4Addr::from(self.0.to_be_bytes());
        write!(f, "{}", ipv4)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options(pub u8);

impl Options {
    /// Returns the value of the E-bit.
    /// This bit describes the way AS-external-LSAs are flooded, as described in Sections 3.6, 9.5, 10.8 and 12.1.2 of this memo.
    pub fn e_bit(&self) -> bool {
        self.0 & 0b0000_0010 != 0
    }
    
    /// Returns the value of the MC-bit.
    /// This bit describes whether IP multicast datagrams are forwarded according to the specifications in Ref18.
    pub fn mc_bit(&self) -> bool {
        self.0 & 0b0000_0100 != 0
    }
    
    /// Returns the value of the N/P-bit.
    /// This bit describes the handling of Type-7 LSAs, as specified in Ref19.
    pub fn np_bit(&self) -> bool {
        self.0 & 0b0000_1000 != 0
    }
    
    /// Returns the value of the EA-bit
    /// This bit describes the router's willingness to receive and forward External-Attributes-LSAs, as specified in [Ref20].
    pub fn ea_bit(&self) -> bool {
        self.0 & 0b0001_0000 != 0
    }
    
    /// Returns the value of the DC-bit.
    /// This bit describes the router's willingness to receive and forward Demand Circuit LSAs, as specified in [Ref21].
    pub fn dc_bit(&self) -> bool {
        self.0 & 0b0010_0000 != 0
    }
}

impl std::fmt::Debug for LinkStateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ipv4 = std::net::Ipv4Addr::from(self.0.to_be_bytes());
        write!(f, "LinkStateId({:#010x}, {})", self.0, ipv4)
    }
}

impl std::fmt::Debug for RouterId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ipv4 = std::net::Ipv4Addr::from(self.0.to_be_bytes());
        write!(f, "RouterId({:#010x}, {})", self.0, ipv4)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum LsaType {
    Router,
    Network,
    Summary,
    AsExternal,
}

impl Display for LsaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Router => write!(f, "Router (1)"),
            Self::Network => write!(f, "Network (2)"),
            Self::Summary => write!(f, "Summary (3)"),
            Self::AsExternal => write!(f, "AS External (5)"),
            _ => write!(f, "Unknown"),
        }
    }
}

impl LsaType {
    pub fn from_u8(value: u8) -> Result<Self, ()> {
        match value {
            1 => Ok(Self::Router),
            2 => Ok(Self::Network),
            3 => Ok(Self::Summary),
            5 => Ok(Self::AsExternal),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LsaHeader {
    pub ls_age: Field<u16>,
    pub options: Field<Options>,
    pub ls_type: Field<LsaType>,
    pub ls_id: Field<LinkStateId>,
    pub adv_router: Field<RouterId>,
    pub seq_num: Field<u32>,
    pub checksum: Field<u16>,
    pub length: Field<u16>,
    pub span: Span,
}

impl LsaHeader {
    pub fn from_bytes(input: &[u8]) -> IResult<&[u8], LsaHeader> {
        let full_len = input.len();

        let (i, ls_age) = be_u16(input)?;
        let ls_age = Field::new(
            Span::new(full_len - i.len() - 2, full_len - i.len()),
            ls_age,
        );

        let (i, options) = be_u8(i)?;
        let options = Field::new(
            Span::new(full_len - i.len() - 1, full_len - i.len()),
            Options(options),
        );

        let (i, ls_type) = be_u8(i)?;
        let ls_type = Field::new(
            Span::new(full_len - i.len() - 2, full_len - i.len()),
            LsaType::from_u8(ls_type).map_err(|_| {
                nom::Err::Error(nom::error::Error::new(i, nom::error::ErrorKind::Tag))
            })?,
        );

        let (i, ls_id) = be_u32(i)?;
        let ls_id = Field::new(
            Span::new(full_len - i.len() - 4, full_len - i.len()),
            LinkStateId(ls_id),
        );

        let (i, adv_router) = be_u32(i)?;
        let adv_router = Field::new(
            Span::new(full_len - i.len() - 4, full_len - i.len()),
            RouterId(adv_router),
        );

        let (i, seq_num) = be_u32(i)?;
        let seq_num = Field::new(
            Span::new(full_len - i.len() - 4, full_len - i.len()),
            seq_num,
        );

        let (i, checksum) = be_u16(i)?;
        let checksum = Field::new(
            Span::new(full_len - i.len() - 2, full_len - i.len()),
            checksum,
        );

        let (i, length) = be_u16(i)?;
        let length = Field::new(
            Span::new(full_len - i.len() - 2, full_len - i.len()),
            length,
        );
        
        let consumed_bytes = full_len - i.len();
        let span = Span::new(0, consumed_bytes);

        Ok((
            i,
            LsaHeader {
                ls_age,
                options,
                ls_type,
                ls_id,
                adv_router,
                seq_num,
                checksum,
                length,
                span
            },
        ))
    }
}
