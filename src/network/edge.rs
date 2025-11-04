use uuid::Uuid;


#[derive(Debug, Clone)]
pub struct Edge {
    pub source_id: Uuid,
    pub destination_id: Uuid,
    pub metric: EdgeMetric,
}

#[derive(Debug, Clone)]
pub enum EdgeMetric {
    // TODO
    Ospf,
    IsIs,
    Other
}