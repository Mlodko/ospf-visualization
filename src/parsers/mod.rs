use crate::data_aquisition::core::RawRouterData;

pub mod ospf_parser;

/// This trait defines a parser from data_aquisition::RawRouterData to NetworkGraph or its elements (Node or Edge) (TBD)
pub trait ProtocolParser {
    type Error;
    async fn parse(data: Vec<RawRouterData>) -> Result<(), Self::Error>;
}