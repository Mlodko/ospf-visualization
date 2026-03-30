use std::{collections::HashMap, time::SystemTime};

use catppuccin_egui::Theme;
use petgraph::graph::NodeIndex;
use uuid::Uuid;

use crate::{gui::{modules::graph::GraphWidget, new_app::ThemeId, tools::tray::ToolId}, network::{edge::Edge, network_graph::NetworkGraph, node::Node, router::InterfaceStats}, topology::store::{SourceHealth, SourceId}};

/// Set of actions that can be called from modules/panels and executed by the app
pub trait AppActions: ConnectActions + GraphActions + SourceActions {
    fn theme(&self) -> ThemeId;
    fn set_theme(&mut self, theme: ThemeId);
    fn get_active_tool(&self) -> Option<ToolId>;
}

pub trait GraphActions {
    fn graph_mut(&mut self) -> &mut NetworkGraph;
    fn reload_graph(&mut self) -> anyhow::Result<()>;
    fn selected_node(&self) -> Option<&Node>;
    fn selected_edge(&self) -> Option<&Edge>;
    fn node_index_to_uuid(&self, index: NodeIndex) -> Option<Uuid>;
    fn compute_path(&mut self, start: Uuid, end: Uuid) -> Option<(u32, Vec<Uuid>)>;
    fn clear_node_selection(&mut self);
}

pub trait SourceActions {
    fn list_sources(&self) -> Vec<SourceSummary>;
    fn source_summary(&self, id: &SourceId) -> Option<SourceSummary>;
    fn is_source_enabled(&self, id: &SourceId) -> bool;
    fn toggle_source(&mut self, src_id: &SourceId);
    fn remove_source(&mut self, src_id: &SourceId) -> anyhow::Result<()>;
    fn store_to_string(&self) -> anyhow::Result<String>;
    fn source_to_string(&self, src_id: &SourceId) -> anyhow::Result<String>;
}

#[derive(Debug, Clone)]
pub struct SourceSummary {
    pub id: SourceId,
    pub health: SourceHealth,
    pub nodes_count: usize,
    pub last_snapshot: SystemTime,
    pub interface_stats: Vec<InterfaceStats>,
}

pub trait ConnectActions {
    fn ssh_connect_status(&self) -> &ConnectStatus;
    fn clear_ssh_connect_status(&mut self);
    fn snmp_connect_status(&self) -> &ConnectStatus;
    fn clear_snmp_connect_status(&mut self);
    fn connect_ssh(&mut self, host: String, port: u16, username: String, password: String, clear_other_sources: bool);
    fn connect_snmp(&mut self, host: String, port: u16, community: String, clear_other_sources: bool);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectStatus {
    Idle,
    Pending,
    Success,
    Failure(String)
}

impl ConnectStatus {
    pub fn is_idle(&self) -> bool {
        matches!(self, ConnectStatus::Idle)
    }
    
    pub fn is_pending(&self) -> bool {
        matches!(self, ConnectStatus::Pending)
    }
    
    pub fn is_success(&self) -> bool {
        matches!(self, ConnectStatus::Success)
    }
    
    pub fn is_failure(&self) -> bool {
        matches!(self, ConnectStatus::Failure(_))
    }
    
    pub fn failure_reason(&self) -> Option<&str> {
        if let ConnectStatus::Failure(reason) = self {
            Some(reason)
        } else {
            None
        }
    }
}

impl Default for ConnectStatus {
    fn default() -> Self {
        Self::Idle
    }
}