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
    Manual(u32),
    Other,
    None
}

impl Into<u32> for &EdgeMetric {
    fn into(self) -> u32 {
        match self {
            EdgeMetric::Ospf(v) => *v,
            EdgeMetric::IsIs(v) => *v,
            EdgeMetric::Manual(v) => *v,
            EdgeMetric::Other => 0,
            EdgeMetric::None => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UndirectedEdgeKey {
    pub a: Uuid,
    pub b: Uuid,
    pub kind: EdgeKind
}

impl UndirectedEdgeKey {
    pub fn new(a: Uuid, b: Uuid, kind: EdgeKind) -> Self {
        let (a, b) = if a < b { (a, b) } else { (b, a) };
        UndirectedEdgeKey { a, b, kind }
    }
    
    pub fn endpoints(&self) -> (Uuid, Uuid) {
        (self.a, self.b)
    }
}

#[derive(Clone, Debug)]
pub struct ManualEdgeSpec {
    pub key: UndirectedEdgeKey,
    pub metric: EdgeMetric,
    pub protocol_tag: String
}

impl ManualEdgeSpec {
    pub fn new(key: UndirectedEdgeKey, metric: u32) -> Self {
        ManualEdgeSpec { key, metric: EdgeMetric::Manual(metric), protocol_tag: "MANUAL".to_string() }
    }
    
    pub fn set_metric(&mut self, metric: u32) {
        self.metric = EdgeMetric::Manual(metric);
    }
}