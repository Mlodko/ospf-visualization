use std::net::IpAddr;


pub struct BgpAttributeSet {
    pub index: u64,
    pub aggregator: Option<BgpAttributeAggregator>,
    pub aigp: Option<u64>,
    pub as_path: Option<Vec<BgpAsPathSegment>>,
    pub atomic_aggregate: bool,
    pub cluster_list: Option<IpAddr>,
    pub communities: Option<BgpCommunities>,
    pub local_pref: Option<u32>,
    pub med: Option<u32>,
    pub next_hop: Option<IpAddr>,
    pub origin: Option<BgpAttrOrigin>,
    pub originator_id: Option<IpAddr>
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum BgpAttrOrigin {
    Igp = 0,
    Egp = 1,
    Incomplete = 2,
}

pub struct BgpCommunities {
    pub community: Option<Vec<String>>,
    pub ext_community: Option<Vec<String>>,
    pub ext_community_raw_format: Option<Vec<String>>,
    pub ipv6_ext_community: Option<Vec<String>>,
    pub large_community: Option<Vec<String>>,
}

pub struct BgpAttributeAggregator {
    pub address: IpAddr,
    pub as_number: u32,
}

pub struct BgpAsPathSegment {
    pub index: u32,
    pub members: Vec<u32>,
    pub kind: BgpAsPathSegmentType
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum BgpAsPathSegmentType {
    AsSet = 0,
    AsSequence = 1,
    AsConfedSequence = 2,
    AsConfedSet = 3,
}