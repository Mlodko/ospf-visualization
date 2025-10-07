use uuid::Uuid;

pub struct Edge {
    pub source_id: Uuid,
    pub destination_id: Uuid,
    pub metric: EdgeMetric,
}

pub enum EdgeMetric {
    // TODO
    Ospf,
    IsIs,
    Other
}