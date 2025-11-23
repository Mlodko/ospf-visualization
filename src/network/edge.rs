use uuid::Uuid;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Edge {
    pub source_id: Uuid,
    pub destination_id: Uuid,
    pub metric: EdgeMetric,
    pub kind: EdgeKind,
    pub protocol_tag: Option<String>
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum EdgeMetric {
    // TODO
    Ospf(u32),
    IsIs(u32),
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    /// Physical or authoritative presence on network
    Membership,
    /// Inter-area / inter-level advertised reachability
    LogicalReachability,
    /// External route injection
    External,
    /// Virtual link / overlay adjacency
    VirtualAdjacency,
}
