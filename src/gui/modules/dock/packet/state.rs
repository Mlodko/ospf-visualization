use std::sync::{Arc, Mutex};

use anyhow::anyhow;

use crate::{
    gui::modules::dock::packet::parse::{lsa::lsa::Lsa, span::Span},
    network::node::{Node, NodeInfo, ProtocolData}, parsers::isis_parser::core_lsp::Lsp,
};

#[derive(Default, Clone, Debug)]
pub struct PacketInspectorState {
    pub current_packet: Option<Packet>,
    pub selected_span: Option<Span>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    Ospf(Lsa),
    IsIs(Lsp)
}

impl PacketInspectorState {
    pub fn new() -> Self {
        Self {
            current_packet: None,
            selected_span: None,
        }
    }

    pub fn set_packet_from_node(&mut self, node: &Node) -> anyhow::Result<()> {
        let protocol_data = match &node.info {
            NodeInfo::Router(r) => {
                &r.protocol_data.clone().ok_or(anyhow!("No protocol data"))?
            }
            NodeInfo::Network(n) => {
                &n.protocol_data.clone().ok_or(anyhow!("No protocol data"))?
            }
        };
        
        match protocol_data {
            ProtocolData::Ospf(ospf_data) => {
                let raw = ospf_data.base_advertisement.raw();
        
                let (_, lsa) = Lsa::parse(raw).map_err(|e| anyhow!("Failed to parse LSA: {e}"))?;
        
                self.set_lsa(lsa);
            }
            ProtocolData::IsIs(isis_data) => {
                self.set_isis(isis_data.lsp.clone());
            }
            ProtocolData::Other(_) => {}
        }


        Ok(())
    }

    pub fn new_arc() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::new()))
    }

    pub fn set_lsa(&mut self, lsa: Lsa) {
        self.current_packet = Some(Packet::Ospf(lsa));
    }
    
    pub fn set_isis(&mut self, lsp: Lsp) {
        self.current_packet = Some(Packet::IsIs(lsp));
    }

    pub fn set_span(&mut self, span: Span) {
        self.selected_span = Some(span);
    }

    pub fn clear(&mut self) {
        self.current_packet = None;
        self.selected_span = None;
    }

    pub fn span(&self) -> Option<&Span> {
        self.selected_span.as_ref()
    }

    pub fn lsa(&self) -> Option<&Lsa> {
        if let Some(Packet::Ospf(lsa)) = &self.current_packet {
            Some(lsa)
        } else {
            None
        }
    }

    pub fn lsp(&self) -> Option<&Lsp> {
        if let Some(Packet::IsIs(lsp)) = &self.current_packet {
            Some(lsp)
        } else {
            None
        }
    }
    
    pub fn packet(&self) -> Option<&Packet> {
        self.current_packet.as_ref()
    }

}
