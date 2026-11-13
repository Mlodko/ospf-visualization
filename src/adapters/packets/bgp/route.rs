use std::net::IpAddr;

use chrono::{DateTime, Utc};
use ipnetwork::IpNetwork;

pub struct BgpRoute {
    pub neighbor: IpAddr,
    pub path_id: u32,
    pub prefix: IpNetwork,
    pub attr_id: String,
    pub metadata: Option<BgpRouteMetadata>,
}

pub struct BgpRouteMetadata {
    pub route_state: BgpRouteState,
    pub last_modified: DateTime<Utc>,
    pub neighbor_as: u32,
    pub route_flap_damping: Option<BgpRouteFlapDamping>,
    pub internal_tags: Option<Vec<String>>
}

pub struct BgpRouteFlapDamping {
    pub decayed: bool,
    pub figure_of_merit: u16,
    pub flap_count: u32,
    pub history: bool,
    pub reuse_time: u16,
    pub suppressed: bool
}

pub struct BgpRouteState {
    pub best_route: bool,
    pub group_best: bool,
    pub backup_route: bool,
    pub used_route: bool,
    pub fib_disabled: bool,
    pub stale_route: bool,
    pub pending_delete: bool,
    pub unused_weight_only: bool,
    pub validity: BgpRouteValidity,
    pub tie_break_reason: Option<BgpTieBreakReason>,
}

pub enum BgpRouteValidity {
    Valid,
    Invalid { reasons: InvalidRouteReasons },
    Unknown,
}

pub struct InvalidRouteReasons {
    pub as_loop: bool,
    pub cluster_loop: bool,
    pub domain_path_loop: bool,
    pub fib_programming_failed: bool,
    pub label_allocation_failed: bool,
    pub next_hop_unresolved: bool,
    pub rejected_route: bool
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum BgpTieBreakReason {
    Unknown = -1,
    None = 0,
    Origin = 1,
    AsPathLength = 2,
    NextHopCost = 3,
    Med = 4,
    LocalPref = 5,
    Aggregate = 6,
    OriginatorId = 7,
    ClusterList = 9,
    ExtendedCommunity = 10,
    Aigp = 11,
    MissingAttribute = 12,
    RtmPref = 13,
    Owner = 14,
    EigpLabeled = 15,
    VpnRoute = 16,
    EbgpRoute = 17,
    PeerIp = 18,
    LocalPeer = 19,
    MultiPath = 20,
    VpnRd = 21,
    NextHopType = 22,
    InvalidRoute = 23,
    OriginValidation = 24,
    LongLiveGrStale = 25,
    DefaultOriginate = 26,
    FibInstallDisabled = 27,
    PeerRouterId = 28,
    PathIdentifier = 29,
    DomainPathLength = 30,
}
