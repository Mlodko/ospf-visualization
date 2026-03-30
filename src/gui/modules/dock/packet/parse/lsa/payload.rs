use std::{fmt::Display, net::Ipv4Addr};

use nom::{
    IResult,
    number::complete::{be_u8, be_u16, be_u32},
};

use crate::gui::modules::dock::packet::parse::{
    lsa::header::{LsaType, RouterId},
    span::{Field, Span},
};

#[derive(Debug, Clone)]
pub enum LsaPayload {
    Router(RouterLsa),
    Network(NetworkLsa),
    Summary(SummaryLsa),
    AsExternal(AsExternalLsa),
}

impl LsaPayload {
    /// Parse payload based on provided `lsa_type`, starting at `start_offset` (absolute).
    /// `input` must begin at the first payload byte (after the 20-byte header).
    pub fn parse(input: &[u8], start_offset: usize, lsa_type: LsaType) -> IResult<&[u8], Self> {
        match lsa_type {
            LsaType::Router => {
                let (rest, router) = RouterLsa::parse(input, start_offset)?;
                Ok((rest, LsaPayload::Router(router)))
            }
            LsaType::Network => {
                let (rest, net) = NetworkLsa::parse(input, start_offset)?;
                Ok((rest, LsaPayload::Network(net)))
            }
            LsaType::Summary => {
                let (rest, sum) = SummaryLsa::parse(input, start_offset)?;
                Ok((rest, LsaPayload::Summary(sum)))
            }
            LsaType::AsExternal => {
                let (rest, ext) = AsExternalLsa::parse(input, start_offset)?;
                Ok((rest, LsaPayload::AsExternal(ext)))
            }
        }
    }
}

/*
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|            LS age             |     Options   |       1       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                        Link State ID                          |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     Advertising Router                        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     LS sequence number                        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|         LS checksum           |             length            |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|    0    |V|E|B|        0      |            # links            |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                          Link ID                              |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                         Link Data                             |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|     Type      |     # TOS     |            metric             |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                              ...                              |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|      TOS      |        0      |          TOS  metric          |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                          Link ID                              |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                         Link Data                             |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                              ...                              |
*/

#[derive(Debug, Clone)]
pub struct RouterLsa {
    pub span: Span,
    pub flags: Field<RouterFlags>,
    pub link_count: Field<u16>,
    pub links: Vec<RouterLink>,
}

impl RouterLsa {
    /// Parse a Router-LSA payload starting at `start_offset` (absolute), producing absolute spans.
    /// The input slice should start at the first payload byte (immediately after the 20-byte header).
    pub fn parse(input: &[u8], start_offset: usize) -> IResult<&[u8], Self> {
        let mut cursor = 0usize;

        // Flags (2 bytes)
        let (i1, flags_u16) = be_u16(input)?;
        let flags = Field::new(
            Span::new(start_offset + cursor, start_offset + cursor + 2),
            RouterFlags(flags_u16),
        );
        cursor += 2;

        // Link count (2 bytes)
        let (i2, link_count_val) = be_u16(i1)?;
        let link_count = Field::new(
            Span::new(start_offset + cursor, start_offset + cursor + 2),
            link_count_val,
        );
        cursor += 2;

        // Links
        let mut links = Vec::with_capacity(link_count.value as usize);
        let mut rest = i2;
        for _ in 0..link_count.value {
            let (i_next, link) = RouterLink::parse(rest, start_offset + cursor)?;
            cursor = link.span.end - start_offset; // advance by parsed link length
            links.push(link);
            rest = i_next;
        }

        let span = Span::new(start_offset, start_offset + cursor);

        Ok((
            rest,
            RouterLsa {
                span,
                flags,
                link_count,
                links,
            },
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouterFlags(pub u16);

impl RouterFlags {
    pub fn v_bit(&self) -> bool {
        self.0 & 0x0800 != 0
    }
    
    pub fn e_bit(&self) -> bool {
        self.0 & 0x0400 != 0
    }
    
    pub fn b_bit(&self) -> bool {
        self.0 & 0x0200 != 0
    }
}

/*
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                          Link ID                              |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                         Link Data                             |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|     Type      |     # TOS     |            metric             |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                              ...                              |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|      TOS      |        0      |          TOS  metric          |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
*/

#[derive(Debug, Clone)]
pub struct RouterLink {
    pub span: Span,
    pub id: Field<RouterLinkId>,
    pub data: Field<RouterLinkData>,
    pub link_type: Field<RouterLinkType>,
    pub tos_count: Field<u8>,
    pub metric: Field<u16>,
    pub tos_metrics: Vec<RouterLinkTosMetric>,
}

impl RouterLink {
    pub fn parse(input: &[u8], start_offset: usize) -> IResult<&[u8], Self> {
        let mut cursor = 0usize;

        // Link ID (4 bytes)
        let (i1, id_u32) = be_u32(input)?;
        let id = Field::new(
            Span::new(start_offset + cursor, start_offset + cursor + 4),
            RouterLinkId(id_u32),
        );
        cursor += 4;

        // Link Data (4 bytes)
        let (i2, data_u32) = be_u32(i1)?;
        let data = Field::new(
            Span::new(start_offset + cursor, start_offset + cursor + 4),
            RouterLinkData(data_u32),
        );
        cursor += 4;

        // Type (1 byte)
        let (i3, link_type_raw) = be_u8(i2)?;
        let link_type_val = RouterLinkType::try_from(link_type_raw)
            .map_err(|_| nom::Err::Error(nom::error::Error::new(i3, nom::error::ErrorKind::Tag)))?;
        let link_type = Field::new(
            Span::new(start_offset + cursor, start_offset + cursor + 1),
            link_type_val,
        );
        cursor += 1;

        // # TOS (1 byte)
        let (i4, tos_count_val) = be_u8(i3)?;
        let tos_count = Field::new(
            Span::new(start_offset + cursor, start_offset + cursor + 1),
            tos_count_val,
        );
        cursor += 1;

        // Base metric (2 bytes)
        let (i5, metric_u16) = be_u16(i4)?;
        let metric = Field::new(
            Span::new(start_offset + cursor, start_offset + cursor + 2),
            metric_u16,
        );
        cursor += 2;

        // Optional per‑TOS metric blocks (each 4 bytes)
        let mut rest = i5;
        let mut tos_metrics = Vec::with_capacity(tos_count.value as usize);
        for _ in 0..tos_count.value {
            let child_slice = &rest[..4];
            let child_offset = start_offset + cursor;
            let (_after_child, tm) = RouterLinkTosMetric::parse(child_slice, child_offset)?;
            tos_metrics.push(tm);

            cursor += 4;
            rest = &rest[4..];
        }

        // Composite span for the entire link
        let span = Span::new(start_offset, start_offset + cursor);

        Ok((
            rest,
            RouterLink {
                span,
                id,
                data,
                link_type,
                tos_count,
                metric,
                tos_metrics,
            },
        ))
    }
    
    pub fn link_id(&self) -> RouterLinkIdKind {
        match self.link_type.value {
            RouterLinkType::PointToPoint | RouterLinkType::Virtual => RouterLinkIdKind::NeighborRouter(RouterId(self.id.value.0)),
            RouterLinkType::Transit => RouterLinkIdKind::DesignatedRouterId(Ipv4Addr::from_bits(self.id.value.0)),
            RouterLinkType::Stub => RouterLinkIdKind::StubNetworkNumber(Ipv4Addr::from_bits(self.id.value.0))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouterLinkType {
    PointToPoint = 1,
    Transit = 2,
    Stub = 3,
    Virtual = 4,
}

impl Display for RouterLinkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PointToPoint => write!(f, "Point-to-Point (1)"),
            Self::Transit => write!(f, "Transit (2)"),
            Self::Stub => write!(f, "Stub (3)"),
            Self::Virtual => write!(f, "Virtual (4)"),
        }
    }
}

impl TryFrom<u8> for RouterLinkType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::PointToPoint),
            2 => Ok(Self::Transit),
            3 => Ok(Self::Stub),
            4 => Ok(Self::Virtual),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouterLinkIdKind {
    // Types 1 and 4
    NeighborRouter(RouterId),
    // Type 2
    DesignatedRouterId(Ipv4Addr),
    // Type 3
    StubNetworkNumber(Ipv4Addr),
}

impl Display for RouterLinkIdKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeighborRouter(id) => write!(f, "Neighbor Router: {}", id),
            Self::DesignatedRouterId(id) => write!(f, "Designated Router ID: {}", id),
            Self::StubNetworkNumber(id) => write!(f, "Stub Network Number: {}", id),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouterLinkId(pub u32);

impl Display for RouterLinkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Ipv4Addr::from_bits(self.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouterLinkData(pub u32);

/*
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|      TOS      |        0      |          TOS  metric          |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
*/

#[derive(Debug, Clone)]
pub struct RouterLinkTosMetric {
    pub span: Span,
    pub tos: Field<u8>,
    pub metric: Field<u16>,
}

impl RouterLinkTosMetric {
    pub fn parse(input: &[u8], start_offset: usize) -> IResult<&[u8], Self> {
        if input.len() != 4 {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::LengthValue,
            )));
        }

        // TOS (1 byte) at [start_offset .. start_offset+1)
        let (i1, tos_val) = be_u8(input)?;
        let tos = Field::new(Span::new(start_offset + 0, start_offset + 1), tos_val);

        // Reserved (1 byte) — advance but don't store
        let (i2, _reserved) = be_u8(i1)?;

        // Metric (2 bytes) at [start_offset+2 .. start_offset+4)
        let (i3, metric_val) = be_u16(i2)?;
        let metric = Field::new(Span::new(start_offset + 2, start_offset + 4), metric_val);

        // Block span is the full 4 bytes at the given absolute offset
        let span = Span::new(start_offset, start_offset + 4);

        Ok((i3, RouterLinkTosMetric { span, tos, metric }))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NetworkMask(pub u32);

impl Display for NetworkMask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ipv4 = std::net::Ipv4Addr::from(self.0.to_be_bytes());
        write!(f, "{}", ipv4)
    }
}

impl std::fmt::Debug for NetworkMask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ipv4 = std::net::Ipv4Addr::from(self.0.to_be_bytes());
        write!(f, "NetworkMask({:#010x}, {})", self.0, ipv4)
    }
}

#[derive(Debug, Clone)]
pub struct NetworkLsa {
    pub span: Span,
    pub mask: Field<NetworkMask>,
    pub attached_routers: Vec<Field<RouterId>>,
}

impl NetworkLsa {
    /// Parse a Network-LSA payload starting at `start_offset` (absolute), producing absolute spans.
    /// The input slice should start at the first payload byte (immediately after the 20-byte header).
    pub fn parse(input: &[u8], start_offset: usize) -> IResult<&[u8], Self> {
        let mut cursor = 0usize;

        // Network Mask (4 bytes)
        let (i1, mask_u32) = be_u32(input)?;
        let mask = Field::new(
            Span::new(start_offset + cursor, start_offset + cursor + 4),
            NetworkMask(mask_u32),
        );
        cursor += 4;

        // Attached Routers: repeat 4-byte RouterId until end of payload
        let mut attached_routers = Vec::new();
        let mut rest = i1;
        while !rest.is_empty() {
            let (i_next, rid_u32) = be_u32(rest)?;
            let rid = Field::new(
                Span::new(start_offset + cursor, start_offset + cursor + 4),
                RouterId(rid_u32),
            );
            attached_routers.push(rid);
            cursor += 4;
            rest = i_next;
        }

        let span = Span::new(start_offset, start_offset + cursor);

        Ok((
            rest,
            NetworkLsa {
                span,
                mask,
                attached_routers,
            },
        ))
    }
}

/*
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|     TOS       |                TOS  metric                    |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
*/

#[derive(Debug, Clone)]
pub struct SummaryTosMetric {
    pub span: Span, // Should be 32 bits
    pub tos: Field<u8>,
    pub metric: Field<u32>, // Same as base metric, really 24-bit
}

impl SummaryTosMetric {
    pub fn metric_24bit(&self) -> u32 {
        self.metric.value & 0x00FF_FFFF
    }
}

/*
0                   1                   2                   3
0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|            LS age             |     Options   |    3 or 4     |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                        Link State ID                          |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     Advertising Router                        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     LS sequence number                        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|         LS checksum           |             length            |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                         Network Mask                          |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|      0        |                  metric                       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|     TOS       |                TOS  metric                    |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                              ...                              |
*/

#[derive(Debug, Clone)]
pub struct SummaryLsa {
    pub span: Span,
    pub mask: Field<NetworkMask>,
    pub metric: Field<u32>, // It's 24-bit really, with leading zeros (1 byte)
    pub tos_metrics: Vec<SummaryTosMetric>,
}

impl SummaryLsa {
    pub fn metric_24bit(&self) -> u32 {
        self.metric.value & 0x00FF_FFFF
    }
}

impl SummaryLsa {
    /// Parse a Summary-LSA (Type 3/4) payload at `start_offset` (absolute).
    pub fn parse(input: &[u8], start_offset: usize) -> IResult<&[u8], Self> {
        let mut cursor = 0usize;

        // Network Mask (4 bytes)
        let (i1, mask_u32) = be_u32(input)?;
        let mask = Field::new(
            Span::new(start_offset + cursor, start_offset + cursor + 4),
            NetworkMask(mask_u32),
        );
        cursor += 4;

        // Base metric word (4 bytes; high byte often 0, low 24 bits carry metric)
        let (i2, metric_u32) = be_u32(i1)?;
        let metric = Field::new(
            Span::new(start_offset + cursor, start_offset + cursor + 4),
            metric_u32,
        );
        cursor += 4;

        // Optional TOS-specific metrics: each 4-byte block (TOS u8 + 24-bit metric)
        let mut tos_metrics = Vec::new();
        let mut rest = i2;
        while !rest.is_empty() {
            // TOS (1)
            let (i_tos, tos_val) = be_u8(rest)?;
            let tos_field = Field::new(
                Span::new(start_offset + cursor, start_offset + cursor + 1),
                tos_val,
            );
            cursor += 1;

            // TOS metric (3 bytes) — read as u32 via be_u16 + be_u8 or a `take(3)`, here we use be_u16 + be_u8
            let (i_m_hi, metric_hi) = be_u16(i_tos)?;
            let (i_m_lo, metric_lo) = be_u8(i_m_hi)?;
            let metric_24 = ((metric_hi as u32) << 8) | (metric_lo as u32);
            let metric_field = Field::new(
                Span::new(start_offset + cursor, start_offset + cursor + 3),
                metric_24,
            );
            cursor += 3;

            let span = Span::new(
                metric_field.span.start - 1, // start at TOS byte
                metric_field.span.end,
            );

            tos_metrics.push(SummaryTosMetric {
                span,
                tos: tos_field,
                metric: metric_field,
            });

            rest = i_m_lo;
        }

        let span = Span::new(start_offset, start_offset + cursor);

        Ok((
            rest,
            SummaryLsa {
                span,
                mask,
                metric,
                tos_metrics,
            },
        ))
    }
}

/*
0                   1                   2                   3
0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|E|    TOS      |                TOS  metric                    |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                      Forwarding address                       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                      Forwarding address                       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                      External Route Tag                       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
*/

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ForwardingAddress(pub u32);

impl Display for ForwardingAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ipv4 = std::net::Ipv4Addr::from(self.0.to_be_bytes());
        write!(f, "{}", ipv4)
    }
}

impl std::fmt::Debug for ForwardingAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ipv4 = std::net::Ipv4Addr::from(self.0.to_be_bytes());
        write!(f, "ForwardingAddress({:#010x}, {})", self.0, ipv4)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ExternalRouteTag(pub u32);

impl std::fmt::Debug for ExternalRouteTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExternalRouteTag({:#010x})", self.0)
    }
}

pub struct AsExternalTosMetric {
    pub span: Span,
    pub e_bit: Field<bool>,
    pub tos: Field<u8>,     // Really 7-bit
    pub metric: Field<u32>, // Really 24-bit
    pub forwarding_address: Field<ForwardingAddress>,
    pub external_route_tag: Field<ExternalRouteTag>,
}

impl AsExternalTosMetric {
    pub fn metric_24bit(&self) -> u32 {
        self.metric.value & 0xFFFFFF
    }
}

/*
0                   1                   2                   3
0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|            LS age             |     Options   |      5        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                        Link State ID                          |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     Advertising Router                        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     LS sequence number                        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|         LS checksum           |             length            |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                         Network Mask                          |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|E|     0       |                  metric                       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                      Forwarding address                       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                      External Route Tag                       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ ^
|E|    TOS      |                TOS  metric                    | |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ |
|                      Forwarding address                       | |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ | TOS METRICS
|                      Forwarding address                       | |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ |
|                      External Route Tag                       | |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ |
|                              ...                              | v
*/

#[derive(Debug, Clone)]
pub struct AsExternalLsa {
    pub span: Span,
    pub mask: Field<NetworkMask>,
    pub e_bit: Field<bool>,
    pub metric: Field<u32>, // Really 24-bit
    pub forwarding_address: Field<ForwardingAddress>,
    pub external_route_tag: Field<ExternalRouteTag>,
    pub tos_metrics: Vec<SummaryTosMetric>,
}

impl AsExternalLsa {
    pub fn metric_24bit(&self) -> u32 {
        self.metric.value & 0xFFFFFF
    }
}

impl AsExternalLsa {
    /// Parse an AS-External-LSA (Type 5) payload at `start_offset` (absolute).
    pub fn parse(input: &[u8], start_offset: usize) -> IResult<&[u8], Self> {
        let mut cursor = 0usize;

        // Network Mask (4 bytes)
        let (i1, mask_u32) = be_u32(input)?;
        let mask = Field::new(
            Span::new(start_offset + cursor, start_offset + cursor + 4),
            NetworkMask(mask_u32),
        );
        cursor += 4;

        // Base E/metric word (4 bytes): treat as raw u32, derive e_bit in behavior layer
        let (i2, base_metric_u32) = be_u32(i1)?;
        let metric = Field::new(
            Span::new(start_offset + cursor, start_offset + cursor + 4),
            base_metric_u32,
        );
        // e_bit derived from high bit of metric.value when needed
        let e_bit = Field::new(
            Span::new(metric.span.start, metric.span.end), // same 4-byte word for highlighting
            (base_metric_u32 & 0x8000_0000) != 0,
        );
        cursor += 4;

        // Forwarding address (4 bytes)
        let (i3, fwd_u32) = be_u32(i2)?;
        let forwarding_address = Field::new(
            Span::new(start_offset + cursor, start_offset + cursor + 4),
            ForwardingAddress(fwd_u32),
        );
        cursor += 4;

        // External Route Tag (4 bytes)
        let (i4, tag_u32) = be_u32(i3)?;
        let external_route_tag = Field::new(
            Span::new(start_offset + cursor, start_offset + cursor + 4),
            ExternalRouteTag(tag_u32),
        );
        cursor += 4;

        // Optional TOS-specific blocks: each consists of 4-byte (E|TOS|metric) + fwd addr (4) + route tag (4)
        let mut tos_metrics = Vec::new();
        let mut rest = i4;
        while !rest.is_empty() {
            // E/TOS/metric word (4 bytes): parse as u32, then split into parts
            let (i_word, word_u32) = be_u32(rest)?;
            let e = (word_u32 & 0x8000_0000) != 0;
            let tos_val = ((word_u32 >> 24) & 0x7F) as u8;
            let metric_24 = word_u32 & 0x00FF_FFFF;

            let e_bit_field = Field::new(
                Span::new(start_offset + cursor, start_offset + cursor + 4),
                e,
            );
            // TOS is the high 7 bits of the second byte; highlight the whole first byte for simplicity
            let tos_field = Field::new(
                Span::new(start_offset + cursor, start_offset + cursor + 1),
                tos_val,
            );
            let metric_field = Field::new(
                Span::new(start_offset + cursor + 1, start_offset + cursor + 4),
                metric_24,
            );
            cursor += 4;

            // Forwarding address (4)
            let (i_fwd, fwd_u32_tos) = be_u32(i_word)?;
            let _forwarding_address_field = Field::new(
                Span::new(start_offset + cursor, start_offset + cursor + 4),
                ForwardingAddress(fwd_u32_tos),
            );
            cursor += 4;

            // External Route Tag (4)
            let (i_tag, tag_u32_tos) = be_u32(i_fwd)?;
            let external_route_tag_field = Field::new(
                Span::new(start_offset + cursor, start_offset + cursor + 4),
                ExternalRouteTag(tag_u32_tos),
            );
            cursor += 4;

            let span = Span::new(e_bit_field.span.start, external_route_tag_field.span.end);

            tos_metrics.push(SummaryTosMetric {
                span,
                tos: tos_field,
                metric: metric_field,
            });

            // We don't store per-TOS forwarding/tag separately in SummaryTosMetric; adjust if needed
            rest = i_tag;
        }

        let span = Span::new(start_offset, start_offset + cursor);

        Ok((
            rest,
            AsExternalLsa {
                span,
                mask,
                e_bit,
                metric,
                forwarding_address,
                external_route_tag,
                tos_metrics,
            },
        ))
    }
}
