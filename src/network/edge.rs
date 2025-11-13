use uuid::Uuid;


#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Edge {
    pub source_id: Uuid,
    pub destination_id: Uuid,
    pub metric: EdgeMetric,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum EdgeMetric {
    // TODO
    Ospf,
    IsIs,
    Other
}