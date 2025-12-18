use std::cell::RefCell;
use std::collections::HashMap;

use std::hash::{DefaultHasher, Hash};
use std::sync::Arc;
use std::time::Duration;

use std::hash::Hasher;

use crate::data_aquisition::ssh::SshClient;
use crate::gui::autopoll::SourceSpec;
use crate::gui::edge_anim;
use crate::gui::edge_shape::{self, NetworkGraphEdgeShape};
use crate::gui::node_panel::{
    FloatingNodePanel, bullet_list, collapsible_section, protocol_data_section
};
use crate::gui::node_shape::{self, clear_path_highlight};
use crate::network::edge::EdgeKind;
use crate::network::node::NodeInfo;

use crate::network::router::InterfaceStats;
use crate::parsers::isis_parser::topology::IsIsTopology;
use crate::topology::protocol::FederationError;
use crate::topology::source::SnapshotSource;
use crate::topology::store::{MergeConfig, SourceId, SourceState, TopologyStore};
use crate::{
    gui::node_shape::{
        LabelOverlay, NetworkGraphNodeShape, clear_area_highlight, clear_label_overlays,
        partition_highlight_enabled, set_partition_highlight_enabled, take_label_overlays,
    },
    network::{network_graph::NetworkGraph, node::Node},
    topology::OspfSnmpTopology,
};
use catppuccin_egui::Theme;
use eframe::egui;
use egui::{Button, CentralPanel, Checkbox, CollapsingHeader, Context, Id, SidePanel, Ui};
use egui_extras::{Column, TableBuilder};
use egui_graphs::{
    FruchtermanReingoldWithCenterGravity, FruchtermanReingoldWithCenterGravityState,
    LayoutForceDirected, SettingsInteraction, SettingsNavigation,
};
use ipnetwork::IpNetwork;
use petgraph::{Directed, csr::DefaultIx, graph::NodeIndex};
use ssh2::DisconnectCode::ProtocolError;
use tokio::runtime::Runtime;
use tokio::sync::watch;
use uuid::Uuid;

thread_local! {
    static THEME: RefCell<Theme> = RefCell::new(catppuccin_egui::MACCHIATO);
}

pub fn get_theme() -> Theme {
    THEME.with(|theme| theme.borrow().clone())
}

pub fn main(rt: Arc<Runtime>) {
    let native_options = eframe::NativeOptions::default();
    let result = eframe::run_native(
        "My egui App",
        native_options,
        Box::new(|cc| {
            let app = rt.block_on(App::new(cc, rt.clone()));

            match app {
                Ok(app) => {
                    egui_extras::install_image_loaders(&cc.egui_ctx);
                    Ok(Box::new(app) as Box<dyn eframe::App>)
                }
                Err(e) => Err(e.into()),
            }

            // if let Ok(app) = app {
            //     Ok(Box::new(app) as Box<dyn eframe::App>)
            // } else {
            //     Err("Failed to create app".into())
            // }
        }),
    );

    if let Err(e) = result {
        println!("{}", e);
    }
}

type Layout = FruchtermanReingoldWithCenterGravity;
type LayoutState = FruchtermanReingoldWithCenterGravityState;

#[derive(Debug)]
#[allow(dead_code)]
enum RuntimeError {
    TopologyFetchError(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::TopologyFetchError(msg) => write!(f, "Topology fetch error: {}", msg),
        }
    }
}

impl std::error::Error for RuntimeError {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditTool {
    None,
    Snip,
    Draw,
}

pub type PollResult = Result<(SourceId, Vec<Node>, Vec<InterfaceStats>), String>;

struct App {
    #[allow(unused)]
    topo: Box<dyn SnapshotSource>,
    store: TopologyStore,

    graph: NetworkGraph,

    selected_node: Option<NodeIndex>,
    #[allow(unused)]
    runtime: Arc<Runtime>,
    layout_state: LayoutState,
    theme: Theme,

    pending_destroy: Vec<(Uuid, Uuid, EdgeKind, bool)>,

    path_mode: bool,
    path_start: Option<NodeIndex>,
    path_end: Option<NodeIndex>,

    edit_tool: EditTool,
    draw_first: Option<NodeIndex>,
    selected_edge: Option<(Uuid, Uuid, EdgeKind)>,
    previous_manual_metric: Option<u32>,
    
    source_specs: HashMap<SourceId, SourceSpec>,
    autopoll_enabled: bool,
    autopoll_interval: Duration,
    autopoll_interval_tx: Option<tokio::sync::watch::Sender<Duration>>,
    poll_tx: Option<std::sync::mpsc::Sender<PollResult>>,
    poll_rx: Option<std::sync::mpsc::Receiver<PollResult>>,
    autopoll_handles: Vec<tokio::task::JoinHandle<()>>,
    
    // SNMP source switching state
    snmp_host: String,
    snmp_port: u16,
    snmp_community: String,
    clear_sources_on_switch: bool,
    // Quick & dirty: shared result storage for background SNMP connect -> snapshot result
    snmp_connect_res: std::sync::Arc<
        std::sync::Mutex<Option<Result<(SourceId, Vec<Node>, Vec<InterfaceStats>, SourceSpec), String>>>,
    >,
    // Quick & dirty: flag indicating SNMP connect in progress
    snmp_connect_pending: bool,

    // SSH source switching state
    ssh_host: String,
    ssh_port: u16,
    ssh_username: String,
    ssh_password: String,
    ssh_clear_sources_on_switch: bool,
    // Quick & dirty: shared result storage for background SSH connect -> snapshot result
    ssh_connect_res: std::sync::Arc<
        std::sync::Mutex<Option<Result<(SourceId, Vec<Node>, Vec<InterfaceStats>, SourceSpec), String>>>,
    >,
    // Quick & dirty: flag indicating SSH connect in progress
    ssh_connect_pending: bool,

    merge_config: MergeConfig,
}

impl Drop for App {
    fn drop(&mut self) {
        self.stop_autopoll();
    }
}

impl App {
    async fn new(
        cc: &eframe::CreationContext<'_>,
        runtime: Arc<Runtime>,
    ) -> Result<Self, RuntimeError> {
        let _ = cc; // silence unused variable warning for now

        //let snmp_client = crate::data_aquisition::snmp::SnmpClient::default();
        let ssh_client = SshClient::new_with_password(
            "client".to_string(),
            "localhost".to_string(),
            "password".to_string(),
            2221,
        );
        let topo = IsIsTopology::new_from_ssh_client(ssh_client).await.unwrap();
        let topo: Box<dyn SnapshotSource> =
            //Box::new(OspfSnmpTopology::from_snmp_client(snmp_client));
            Box::new(topo);
        let store = TopologyStore::default();

        let merge_config = MergeConfig::default();

        let mut layout_state = LayoutState::default();
        layout_state.base.k_scale = 0.2;

        let app = Self {
            topo,
            store,
            graph: NetworkGraph::default(),

            selected_node: Option::default(),
            runtime,
            layout_state,
            selected_edge: None,
            pending_destroy: Vec::new(),
            theme: THEME.with(|theme| theme.borrow().clone()),

            path_mode: false,
            path_start: None,
            path_end: None,
            previous_manual_metric: None,

            edit_tool: EditTool::None,
            draw_first: None,
            
            source_specs: HashMap::new(),
            autopoll_enabled: false,
            autopoll_interval: Duration::from_secs(30),
            autopoll_interval_tx: None,
            poll_rx: None,
            poll_tx: None,
            autopoll_handles: Vec::new(),

            snmp_host: "127.0.0.1".to_string(),
            snmp_port: 1161,
            snmp_community: "public".to_string(),
            clear_sources_on_switch: true,

            ssh_host: "127.0.0.1".to_string(),
            ssh_port: 2221,
            ssh_username: "client".to_string(),
            ssh_password: "password".to_string(),
            ssh_clear_sources_on_switch: true,
            snmp_connect_res: std::sync::Arc::new(std::sync::Mutex::new(None)),
            snmp_connect_pending: false,
            ssh_connect_res: std::sync::Arc::new(std::sync::Mutex::new(None)),
            ssh_connect_pending: false,

            merge_config,
        };

        Ok(app)
    }
    
    fn start_autopoll(&mut self) {
        
        if !self.autopoll_handles.is_empty() {
            self.stop_autopoll();
        }
        
        // Create channels
        let (poll_tx, poll_rx) = std::sync::mpsc::channel();
        self.poll_tx = Some(poll_tx.clone());
        self.poll_rx = Some(poll_rx);
        
        let (interval_tx, interval_rx) = watch::channel(
            if self.autopoll_interval.is_zero() {
                Duration::from_secs(1)
            } else {
                self.autopoll_interval
            }
        );
        
        self.autopoll_interval_tx = Some(interval_tx.clone());
        
        for (src_id, spec) in self.source_specs.iter() {
            let poll_tx = poll_tx.clone();
            let src_id = src_id.clone();
            let spec = spec.clone();
            let mut interval_rx = interval_rx.clone();
            let handle = self.runtime.spawn(async move {
                let mut hasher = DefaultHasher::new();
                src_id.hash(&mut hasher);
                let jitter = Duration::from_millis(hasher.finish() % 250);
                tokio::time::sleep(jitter).await;
                
                let mut source = match spec.build_topology().await {
                    Ok(topology) => {
                        Some(topology)
                    }
                    Err(e) => {
                        let _ = poll_tx.send(Err(format!("Init failed: {}", e)));
                        None
                    }
                };
                let mut current_interval = *interval_rx.borrow();
                let mut ticker = tokio::time::interval(current_interval);
                loop {
                    tokio::select! {
                        _ = ticker.tick() => {
                            if source.is_none() {
                                match spec.build_topology().await {
                                    Ok(s) => source = Some(s),
                                    Err(e) => {
                                        let _ = poll_tx.send(Err(format!("reinit failed: {}", e)));
                                        continue;
                                    }
                                }
                            }
                            match source.as_mut().unwrap().fetch_snapshot().await {
                                Ok((id, nodes, stats)) => {
                                    let _ = poll_tx.send(Ok((id, nodes, stats)));
                                }
                                Err(e) => {
                                    source = None; // force rebuild next tick
                                    let _ = poll_tx.send(Err(format!("poll failed: {}", e)));
                                }
                            }
                        }
                        // Interval change notification
                        changed = interval_rx.changed() => {
                            if changed.is_err() {
                                // sender dropped; exit task
                                break;
                            }
                            let new_interval = *interval_rx.borrow();
                            if new_interval != current_interval {
                                current_interval = new_interval;
                                ticker = tokio::time::interval(current_interval);
                            }
                        }
                    }
                }
            });
            self.autopoll_handles.push(handle);
        }
    }
    
    fn stop_autopoll(&mut self) {
        for h in self.autopoll_handles.drain(..) {
            h.abort();
        }
        self.poll_tx = None;
        self.poll_rx = None;
        self.autopoll_interval_tx = None;
    }

    fn read_data(&mut self) {
        if let Some(node_index) = self.graph.graph.selected_nodes().first() {
            self.selected_node = Some(*node_index);
        }
    }

    fn apply_edge_traffic_weights(&mut self) {
        for (src_id, state) in self.store.sources_iter() {
            let src_uuid = src_id.to_uuidv5();
            let src_node_idx = self.graph.node_id_to_index_map.get(&src_uuid);
            let src_node_idx = if let Some(idx) = src_node_idx {
                idx.clone()
            } else {
                continue;
            };

            let edges: Vec<_> = self
                .graph
                .graph
                .edges_directed(src_node_idx, petgraph::Direction::Outgoing)
                .collect();
            
            if edges.len() < 2 {
                continue;
            }
            
            let mut prefix_to_dst_uuid: HashMap<IpNetwork, Uuid> = edges.iter()
                .filter_map(|edge| {
                    let dst_uuid = edge.weight().payload().destination_id;
                    let dst_node_idx = self
                        .graph
                        .node_id_to_index_map
                        .get(&dst_uuid)
                        .unwrap()
                        .clone();
                    let dst_node = self.graph.graph.node(dst_node_idx).unwrap();
                    if let NodeInfo::Network(net) = &dst_node.payload().info {
                        Some((net.ip_address, edge.weight().payload().destination_id))
                    } else {
                        None
                    }
                })
                .collect();

            let total_weight: f32 = state
                .interface_stats
                .iter()
                .map(|stats| stats.get_weight() as f32)
                .sum();

            for stats in state.interface_stats.iter() {
                if stats.ip_address.is_loopback() {
                    continue;
                }

                let prefix = prefix_to_dst_uuid.iter().find_map(|(prefix, _)| {
                    if prefix.contains(stats.ip_address) {
                        Some(prefix)
                    } else {
                        None
                    }
                });
                let prefix = if let Some(prefix) = prefix {
                    prefix.clone()
                } else {
                    dbg!("No prefix found for IP address: {}", stats.ip_address);
                    dbg!(&prefix_to_dst_uuid);
                    return;
                };

                let weight = stats.get_weight() as f32 / total_weight;
                let dst_uuid = prefix_to_dst_uuid.remove(&prefix).unwrap();
                println!(
                    "Setting weight for {} -> {} to {}",
                    src_uuid, &prefix, weight
                );
                edge_shape::insert_edge_weight(src_uuid, dst_uuid, weight);
            }
        }
    }

    fn render_sources_section(&mut self, ui: &mut Ui) {
        CollapsingHeader::new("Sources")
            .default_open(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        if ui.button("Print store data").clicked() {
                            println!("[app] Pressed print store data button");
                            let json = serde_json::to_string_pretty(&self.store);
                            match json {
                                Ok(json) => println!("{}", json),
                                Err(err) => println!("Error serializing store data: {}", err)
                            }
                        }

                        let mut rows: Vec<_> = self.store.sources_iter()
                            .map(|(src_id, state): (&SourceId, &SourceState)| {
                                (
                                    src_id.clone(),
                                    state.health.clone(),
                                    state.partition.nodes.len(),
                                    state.last_snapshot.clone(),
                                    state.interface_stats.clone()
                                )
                            })
                            .collect();
                        rows.sort_by(|this, other| this.3.cmp(&other.3));

                        let mut sources_to_remove: Vec<SourceId> = Vec::new();
                        let mut source_enable_states: HashMap<SourceId, bool> = rows.iter().map(|(src_id, _, _, _, _)| {
                            let enabled = self.merge_config.is_source_enabled(src_id);
                            (src_id.clone(), enabled)
                        }).collect();

                        let table = TableBuilder::new(ui)
                            .striped(true)
                            .resizable(true)
                            .column(Column::auto().at_least(70.0))
                            .column(Column::auto().at_least(70.0))
                            .column(Column::auto().at_least(55.0))
                            .column(Column::auto().at_least(145.0))
                            .column(Column::auto().at_least(40.0))
                            .column(Column::auto().at_least(55.0))
                            .column(Column::auto().at_least(20.0));
                        table
                            .header(20.0, |mut header| {
                                header.col(|ui| { ui.strong("Source"); });
                                header.col(|ui| { ui.strong("Health"); });
                                header.col(|ui| { ui.strong("#Nodes"); });
                                header.col(|ui| { ui.strong("Last snapshot (s)"); });
                                header.col(|ui| { ui.strong("IfStats"); });
                                header.col(|ui| { ui.strong("Actions"); });
                                header.col(|ui| { ui.strong("Enabled"); });
                            })
                            .body(|mut body| {
                                rows.sort_by(|(src_id_a, _, _, _, _), (src_id_b, _, _, _, _)| {
                                    src_id_a.as_string().cmp(&src_id_b.to_string())
                                });
                                for (src_id, health, nodes_count, last_snapshot, if_stats) in rows {
                                    body.row(22.0, |mut row| {
                                        row.col(|ui| { ui.label(src_id.to_string()); });
                                        row.col(|ui| { ui.label(health.to_string()); });
                                        row.col(|ui| { ui.label(nodes_count.to_string()); });
                                        row.col(|ui| { ui.label(humantime::format_rfc3339_seconds(last_snapshot).to_string()); });

                                        // IfStats column
                                        row.col(|ui| {
                                            let response = ui.link("ℹ");
                                            let tooltip_closure = |ui: &mut Ui| {
                                                ui.set_width(420.0);
                                                ui.label("Interface Stats");
                                                ui.separator();

                                                let stats_table = TableBuilder::new(ui)
                                                    .striped(true)
                                                    .resizable(false)
                                                    .column(Column::auto().at_least(120.0)) // IP address
                                                    .column(Column::auto().at_least(70.0))  // RX bytes
                                                    .column(Column::auto().at_least(70.0))  // TX bytes
                                                    .column(Column::auto().at_least(70.0))  // RX packets
                                                    .column(Column::auto().at_least(70.0)); // TX packets

                                                stats_table
                                                    .header(18.0, |mut h| {
                                                        h.col(|ui| { ui.strong("IP"); });
                                                        h.col(|ui| { ui.strong("RX B"); });
                                                        h.col(|ui| { ui.strong("TX B"); });
                                                        h.col(|ui| { ui.strong("RX Pkts"); });
                                                        h.col(|ui| { ui.strong("TX Pkts"); });
                                                    })
                                                    .body(|mut b| {
                                                        for interface in if_stats {
                                                            b.row(18.0, |mut r| {
                                                                r.col(|ui| { ui.label(interface.ip_address.to_string()); });
                                                                r.col(|ui| { ui.label(interface.rx_bytes.map(|v| humanize_bytes(v)).unwrap_or_else(|| "-".to_string())); });
                                                                r.col(|ui| { ui.label(interface.tx_bytes.map(|v| humanize_bytes(v)).unwrap_or_else(|| "-".to_string())); });
                                                                r.col(|ui| { ui.label(interface.rx_packets.map(|v| humanize_packet_count(v)).unwrap_or_else(|| "-".to_string())); });
                                                                r.col(|ui| { ui.label(interface.tx_packets.map(|v| humanize_packet_count(v)).unwrap_or_else(|| "-".to_string())); });
                                                            });
                                                        }
                                                    });
                                            };
                                            if response.hovered() {
                                                egui::Tooltip::for_widget(&response).show(tooltip_closure);
                                            }
                                        });

                                        row.col(|ui| {
                                            ui.horizontal(|ui| {
                                                if ui.small_button("🗑").on_hover_text("Remove a source and its partition from the store").clicked() {
                                                    sources_to_remove.push(src_id.clone());
                                                }
                                                if ui.small_button("🗋").on_hover_text("Serialize the source state and print to stdout").clicked() {
                                                    let state = self.store.get_source_state(&src_id).expect("Failed to get source state, this should never happen");
                                                    println!("{}", serde_json::to_string_pretty(state).unwrap_or("Couldn't serialize".to_string()))
                                                }
                                            });
                                        });
                                        row.col(|ui| {
                                            ui.add(Checkbox::without_text(&mut source_enable_states.get_mut(&src_id).unwrap())).on_hover_text("Temporarily enable/disable source from view");
                                        });
                                    })
                                }
                            });

                        if !sources_to_remove.is_empty() {
                            for src_id in sources_to_remove.iter() {
                                if let Err(e) = self.store.remove_partition(src_id) {
                                    eprintln!("Failed to remove partition: {}", e);
                                }
                            }
                        }

                        let sources_enable_state_changed: Vec<_> = source_enable_states.into_iter().filter_map(|(src_id, enabled)| {
                            if enabled != self.merge_config.is_source_enabled(&src_id) {
                                Some((src_id, enabled))
                            } else {
                                None
                            }
                        }).collect();

                        for (src_id, enabled) in sources_enable_state_changed.iter() {
                            println!("Toggling source {} to {}", src_id, enabled);
                            self.merge_config.toggle_source(src_id);
                            println!("New state: {}", self.merge_config.is_source_enabled(src_id))
                        }

                        // There's been some change, reload
                        if !sources_enable_state_changed.is_empty() || !sources_to_remove.is_empty() {
                            if let Err(e) = self.reload_graph() {
                                eprintln!("Failed to reload graph: {}", e);
                            }
                        }

                    })
            });
    }

    fn render_path_controls(&mut self, ui: &mut Ui) {
        ui.checkbox(&mut self.path_mode, "Enable Path Mode");

        if !self.path_mode || ui.button("Clear path").clicked() {
            self.path_start = None;
            self.path_end = None;
            clear_path_highlight();
        }

        if ui.button("Use Selected as start").clicked() {
            if let Some(selected) = self.selected_node {
                self.path_start = Some(selected);
            }
        }

        if ui.button("Use Selected as end").clicked() {
            if let Some(selected) = self.selected_node {
                self.path_end = Some(selected);
            }
        }

        if ui.button("Compute Path").clicked() {
            use petgraph::algo::astar;
            if let (Some(start_id), Some(end_id)) = (self.path_start, self.path_end) {
                let graph = self.graph.graph.g();
                let paths = astar(
                    &graph,
                    start_id,
                    |idx| idx == end_id,
                    |e| -> u32 { (&e.weight().payload().metric).into() },
                    |_| 0,
                );

                let path_uuids = if let Some((_, path)) = paths {
                    path.iter()
                        .filter_map(|idx| self.graph.graph.node(*idx))
                        .map(|n| n.payload().id)
                        .collect()
                } else {
                    Vec::new()
                };

                node_shape::set_path_highlight(path_uuids.into_iter());
            }
        }

        let start_id_name = self
            .path_start
            .and_then(|idx| self.graph.graph.node(idx))
            .map(|node| node.payload().id)
            .map(|id| id.to_string())
            .unwrap_or("None".to_string());

        let end_id_name = self
            .path_end
            .and_then(|idx| self.graph.graph.node(idx))
            .map(|node| node.payload().id)
            .map(|id| id.to_string())
            .unwrap_or("None".to_string());

        ui.label(format!("Start: {}", start_id_name));
        ui.label(format!("End: {}", end_id_name));
    }

    fn reload_graph(&mut self) -> Result<(), FederationError> {
        let merged = self.store.build_merged_view_with(&self.merge_config)?;

        self.graph.reconcile(merged);
        // Authoritatively recompute edge traffic weights after reconciling the graph
        self.apply_edge_traffic_weights();
        Ok(())
    }

    fn render_edit_tools(&mut self, ui: &mut Ui) {
        ui.label("Edit mode");
        ui.horizontal(|ui| {
            let mut t = self.edit_tool;
            if ui
                .selectable_label(matches!(t, EditTool::None), "None")
                .clicked()
            {
                t = EditTool::None;
            }
            if ui
                .selectable_label(matches!(t, EditTool::Snip), "Snip")
                .clicked()
            {
                t = EditTool::Snip;
            }
            if ui
                .selectable_label(matches!(t, EditTool::Draw), "Draw")
                .clicked()
            {
                t = EditTool::Draw;
            }
            self.edit_tool = t;
        });
        ui.add_enabled_ui(self.graph.any_manual_changes(), |ui| {
            if ui.button("Clear all manual changes").clicked() {
                self.graph.clear_manual_changes();
            }
        });
        ui.label("Hint: In Draw, click node A then node B to create an edge. Esc or click empty space cancels.");
        if let Some((a, b, kind)) = self.selected_edge {
            let is_manual = self.graph.is_manual_edge(a, b, kind);
            ui.separator();
            ui.label("Manual edge properties");
            let mut metric_val: i32 = if let Some(metric) = self.previous_manual_metric {
                metric as i32
            } else {
                1
            };
            if ui
                .add_enabled(
                    is_manual,
                    egui::DragValue::new(&mut metric_val).range(1..=u32::MAX as i32),
                )
                .changed()
            {
                println!("Updating edge metric to {}", metric_val);
                self.graph.update_manual_edge(a, b, kind, metric_val as u32);
            }
            if ui
                .add_enabled(is_manual, Button::new("Delete manual edge"))
                .clicked()
            {
                self.graph.remove_manual_edge(a, b, kind);
                self.selected_edge = None;
            }
            self.previous_manual_metric = Some(metric_val as u32);
        } else {
            self.previous_manual_metric = None;
        }
    }

    fn render_autopoll_controls(&mut self, ui: &mut Ui) {
        CollapsingHeader::new("Autopoll Controls")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Interval (s)");
                    let mut seconds = self.autopoll_interval.as_secs();
                    if ui.add_enabled(self.autopoll_enabled, egui::DragValue::new(&mut seconds).range(1..=3600)).changed() {
                        let new_duration = Duration::from_secs(seconds.max(1));
                        self.autopoll_interval = new_duration;
                        
                        if let Some(tx) = &self.autopoll_interval_tx {
                            let _ = tx.send(new_duration);
                        }
                    }
                });
                
                let was_enabled = self.autopoll_enabled;
                ui.checkbox(&mut self.autopoll_enabled, "Enable periodic polling for known sources");
                if self.autopoll_enabled && !was_enabled {
                    self.start_autopoll();
                } else if !self.autopoll_enabled && was_enabled {
                    self.stop_autopoll();
                }
            });
    }

    fn render(&mut self, ctx: &Context) {
        catppuccin_egui::set_theme(ctx, self.theme);
        // Debug: print pending/connect slot state at start of render
        {
            // Snapshot the mutex states briefly for logging (non-blocking relative to UI)
            let _ = match self.ssh_connect_res.lock() {
                Ok(g) => g.is_some(),
                Err(_) => {
                    eprintln!("[app] failed to lock ssh_connect_res for debug");
                    false
                }
            };
            let _ = match self.snmp_connect_res.lock() {
                Ok(g) => g.is_some(),
                Err(_) => {
                    eprintln!("[app] failed to lock snmp_connect_res for debug");
                    false
                }
            };
        }

        // Poll shared result slots for SSH/SNMP at start of render (non-blocking).
        // Apply any completed snapshots to the store and reconcile the graph on the UI thread.
        {
            let res_opt = { self.ssh_connect_res.lock().unwrap().take() };
            if let Some(res) = res_opt {
                match res {
                    Ok((src_id, nodes, stats, source_spec)) => {
                        println!("[app] SSH snapshot received in UI thread (via Arc<Mutex>)");
                        
                        if self.ssh_clear_sources_on_switch {
                            self.store = TopologyStore::default();
                            self.source_specs.clear();
                        }
                        
                        self.source_specs.insert(src_id.clone(), source_spec);
                        
                        let now = std::time::SystemTime::now();
                        self.store.replace_partition(&src_id, nodes, stats, now);

                        // Rebuild graph via authoritative reload_graph()
                        if let Err(e) = self.reload_graph() {
                            eprintln!("[app] Error reloading graph after SSH snapshot: {:?}", e);
                        }
                    }
                    Err(err) => {
                        eprintln!("[app] SSH connect/fetch failed (via Arc<Mutex>): {}", err);
                    }
                }
                // Ensure pending flag is cleared so UI buttons re-enable
                self.ssh_connect_pending = false;
                // Request a repaint so the updated graph is shown
                ctx.request_repaint();
            }
        }

        {
            let res_opt = { self.snmp_connect_res.lock().unwrap().take() };
            if let Some(res) = res_opt {
                match res {
                    Ok((src_id, nodes, stats, spec)) => {
                        println!("[app] SNMP snapshot received in UI thread (via Arc<Mutex>)");
                        if self.clear_sources_on_switch {
                            self.store = TopologyStore::default();
                            self.source_specs.clear();
                        }
                        
                        self.source_specs.insert(src_id.clone(), spec);
                        
                        let now = std::time::SystemTime::now();
                        self.store.replace_partition(&src_id, nodes, stats, now);

                        // Rebuild graph via authoritative reload_graph()
                        if let Err(e) = self.reload_graph() {
                            eprintln!("[app] Error reloading graph after SNMP snapshot: {:?}", e);
                        }
                    }
                    Err(err) => {
                        eprintln!("[app] SNMP connect/fetch failed (via Arc<Mutex>): {}", err);
                    }
                }
                // Ensure pending flag is cleared so UI buttons re-enable
                self.snmp_connect_pending = false;
                // Request a repaint so the updated graph is shown
                ctx.request_repaint();
            }
        }
        
        {
            let mut reload_needed = false;
            if let Some(rx) = &self.poll_rx {
                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        Ok((src_id, nodes, stats)) => {
                            let now = std::time::SystemTime::now();
                            self.store.replace_partition(&src_id, nodes, stats, now);
                            reload_needed = true;
                        }
                        Err(e) => {
                            eprintln!("[app] autopoll failed: {:?}", e);
                        }
                    }
                }
            }
            if reload_needed {
                let _ = self.reload_graph();
                ctx.request_repaint();
            }
        }

        let render_side_panel = |ui: &mut Ui| {
            let mut highlight_enabled = partition_highlight_enabled();
            if ui
                .checkbox(&mut highlight_enabled, "Partition highlight")
                .on_hover_text("Toggle partition-wide highlight on hover")
                .changed()
            {
                println!(
                    "[app] Partition highlight changed to: {}",
                    highlight_enabled
                );
                set_partition_highlight_enabled(highlight_enabled);
            }
            let mut edge_labels_enabled = edge_shape::edge_labels_enabled();
            if ui
                .checkbox(&mut edge_labels_enabled, "Edge metric labels")
                .changed()
            {
                println!(
                    "[app] Edge metric labels changed to: {}",
                    edge_labels_enabled
                );
                edge_shape::set_edge_labels_enabled(edge_labels_enabled);
            }

            ui.separator();

            // SSH connection management
            CollapsingHeader::new("SSH Connection (IS-IS)")
                .default_open(false)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Host");
                        ui.text_edit_singleline(&mut self.ssh_host);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Port");
                        let mut port_val = self.ssh_port as i32;
                        if ui
                            .add(egui::DragValue::new(&mut port_val).range(1..=65535))
                            .changed()
                        {
                            self.ssh_port = port_val as u16;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Username");
                        ui.text_edit_singleline(&mut self.ssh_username);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Password");
                        ui.text_edit_singleline(&mut self.ssh_password);
                    });
                    ui.checkbox(
                        &mut self.ssh_clear_sources_on_switch,
                        "Clear previous sources on connect",
                    );
                    if self.ssh_connect_pending {
                        ui.add_enabled_ui(false, |ui| {
                            _ = ui.button("Connect");
                        });
                    } else if ui.button("Connect").clicked() {
                        // Quick & dirty: spawn a thread and create a per-thread runtime to perform SSH connect + snapshot fetch,
                        // then send snapshot back via channel for the UI thread to apply.
                        let res_arc = std::sync::Arc::new(std::sync::Mutex::new(None));
                        self.ssh_connect_res = res_arc.clone();
                        self.ssh_connect_pending = true;

                        let host = self.ssh_host.clone();
                        let port = self.ssh_port;
                        let username = self.ssh_username.clone();
                        let password = self.ssh_password.clone();
                        let res_arc = res_arc.clone();

                        std::thread::spawn(move || {
                            println!("[bg-ssh] thread start - attempting to create runtime");
                            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                                Ok(rt) => {
                                    println!("[bg-ssh] runtime (current_thread) created");
                                    rt
                                }
                                Err(e) => {
                                    eprintln!("[bg-ssh] failed to create runtime: {:?}", e);
                                    // Store the error into the shared result slot so the UI thread can observe it.
                                    {
                                        let mut guard = res_arc.lock().unwrap();
                                        *guard = Some(Err(format!("Failed to create runtime: {:?}", e)));
                                    }
                                    return;
                                }
                            };

                            println!("[bg-ssh] entering block_on to run async connect/fetch");
                            let res = rt.block_on(async move {
                                println!("[bg-ssh async] creating SSH client");
                                let client =
                                    SshClient::new_with_password(username.clone(), host.clone(), password.clone(), port);
                                println!("[bg-ssh async] created SSH client, creating topology");
                                match IsIsTopology::new_from_ssh_client(client).await {
                                    Ok(mut topo) => {
                                        println!("[bg-ssh async] topology created, fetching snapshot");
                                        match topo.fetch_snapshot().await {
                                            Ok((src_id, nodes, stats)) => {
                                                println!("[bg-ssh async] snapshot fetch succeeded, src_id={:?}, nodes_count={}", src_id, nodes.len());
                                                // Register source spec
                                                
                                                let source_spec = SourceSpec::new_ssh(
                                                    host.clone(),
                                                    port,
                                                    username.clone(),
                                                    password.clone(),
                                                    crate::gui::autopoll::ProtocolKind::Isis
                                                );
                                                
                                                Ok((src_id, nodes, stats, source_spec))
                                            }
                                            Err(e) => {
                                                eprintln!("[bg-ssh async] snapshot fetch failed: {:?}", e);
                                                Err(format!("Failed to fetch snapshot: {:?}", e))
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("[bg-ssh async] failed to create topology: {:?}", e);
                                        Err(format!("Failed to create IsIsTopology: {:?}", e))
                                    }
                                }
                            });

                            println!("[bg-ssh] async work complete, sending result back to UI thread (ok/error)");
                            {
                                // store result into shared Arc<Mutex<Option<...>>> so UI thread can pick it up
                                let mut guard = res_arc.lock().unwrap();
                                *guard = Some(res);
                            }
                        }); ui.ctx().request_repaint();
                    }
                });

            // SNMP connection management
            CollapsingHeader::new("SNMP Connection (OSPF)")
                .default_open(false)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Host");
                        ui.text_edit_singleline(&mut self.snmp_host);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Port");
                        let mut port_val = self.snmp_port as i32;
                        if ui
                            .add(egui::DragValue::new(&mut port_val).range(1..=65535))
                            .changed()
                        {
                            self.snmp_port = port_val as u16;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Community");
                        ui.text_edit_singleline(&mut self.snmp_community);
                    });
                    ui.checkbox(
                        &mut self.clear_sources_on_switch,
                        "Clear previous sources on connect",
                    );
                    if self.snmp_connect_pending {
                        ui.add_enabled_ui(false, |ui| {
                            _ = ui.button("Connect");
                        });
                    } else if ui.button("Connect").clicked() {
                        // Quick & dirty: spawn a thread and create a per-thread runtime to perform SNMP connect + snapshot fetch,
                        // then send snapshot back via channel for the UI thread to apply.
                        let res_arc = std::sync::Arc::new(std::sync::Mutex::new(None));
                        self.snmp_connect_res = res_arc.clone();
                        self.snmp_connect_pending = true;

                        let host = self.snmp_host.clone();
                        let port = self.snmp_port;
                        let community = self.snmp_community.clone();
                        let res_arc = res_arc.clone();

                        std::thread::spawn(move || {
                            println!("[bg-snmp] thread start - attempting to create runtime");
                            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                                Ok(rt) => {
                                    println!("[bg-snmp] runtime (current_thread) created");
                                    rt
                                }
                                Err(e) => {
                                    eprintln!("[bg-snmp] failed to create runtime: {:?}", e);
                                    // Store the error into the shared result slot so the UI thread can observe it.
                                    {
                                        let mut guard = res_arc.lock().unwrap();
                                        *guard = Some(Err(format!("Failed to create runtime: {:?}", e)));
                                    }
                                    return;
                                }
                            };

                            println!("[bg-snmp] entering block_on to run async SNMP lookup/fetch");
                            let res = rt.block_on(async move {
                                println!("[bg-snmp async] resolving host");
                                // Resolve host (IP or DNS)
                                let addr = if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                                    std::net::SocketAddr::new(ip, port)
                                } else {
                                    match tokio::net::lookup_host((host.as_str(), port)).await {
                                        Ok(mut addrs) => addrs.next().unwrap_or_else(|| {
                                            std::net::SocketAddr::new(
                                                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                                                port,
                                            )
                                        }),
                                        Err(e) => {
                                            eprintln!("[bg-snmp async] DNS lookup failed: {:?}", e);
                                            return Err(format!("DNS lookup failed: {:?}", e));
                                        }
                                    }
                                };

                                println!("[bg-snmp async] creating SNMP client for addr={}", addr);
                                let client = crate::data_aquisition::snmp::SnmpClient::new(
                                    addr.clone(),
                                    &community,
                                    snmp2::Version::V2C,
                                    None,
                                );
                                println!("[bg-snmp async] created SNMP client, building topology");
                                let mut topo = OspfSnmpTopology::from_snmp_client(client);
                                println!("[bg-snmp async] fetching snapshot from SNMP topology");
                                match topo.fetch_snapshot().await {
                                    Ok((src_id, nodes, stats)) => {
                                        println!("[bg-snmp async] snapshot fetch succeeded src_id={:?}, nodes_count={}", src_id, nodes.len());
                                        
                                        let spec = SourceSpec::new_snmp(addr, community, snmp2::Version::V2C, None, crate::gui::autopoll::ProtocolKind::Ospf);
                                        
                                        Ok((src_id, nodes, stats, spec))
                                    }
                                    Err(e) => {
                                        eprintln!("[bg-snmp async] failed to fetch snapshot: {:?}", e);
                                        Err(format!("Failed to fetch snapshot: {:?}", e))
                                    }
                                }
                            });

                            println!("[bg-snmp] async work complete, sending result back to UI thread");
                            {
                                // store result into shared Arc<Mutex<Option<...>>> so UI thread can pick it up
                                let mut guard = res_arc.lock().unwrap();
                                *guard = Some(res);
                            }
                            println!("[bg-snmp] send complete, thread exiting");
                        }); ui.ctx().request_repaint();
                    }
                });

            ui.separator();

            self.render_sources_section(ui);

            ui.separator();
            
            self.render_autopoll_controls(ui);
            
            ui.separator();

            // Theme selector
            {
                let theme_before = self.theme;
                egui::ComboBox::from_label("Select theme")
                    .selected_text(format!(
                        "{}",
                        match self.theme {
                            catppuccin_egui::LATTE => "Latte",
                            catppuccin_egui::FRAPPE => "Frappe",
                            catppuccin_egui::MACCHIATO => "Macchiato",
                            catppuccin_egui::MOCHA => "Mocha",
                            _ => "Unknown",
                        }
                    ))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.theme, catppuccin_egui::LATTE, "Latte");
                        ui.selectable_value(&mut self.theme, catppuccin_egui::FRAPPE, "Frappe");
                        ui.selectable_value(
                            &mut self.theme,
                            catppuccin_egui::MACCHIATO,
                            "Macchiato",
                        );
                        ui.selectable_value(&mut self.theme, catppuccin_egui::MOCHA, "Mocha");
                    });
                if theme_before != self.theme {
                    THEME.with(|theme| theme.replace(self.theme));
                    catppuccin_egui::set_theme(ctx, self.theme);
                }
            }

            ui.separator();

            // Forces section
            CollapsingHeader::new("Forces").default_open(true).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut self.layout_state.base.k_scale, 0.2..=3.0).text("k_scale"));
                    info_icon(ui, "Scale ideal edge length k; >1 spreads the layout, <1 compacts it.");
                });
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut self.layout_state.base.c_attract, 0.1..=3.0).text("c_attract"));
                    info_icon(ui, "Multiplier for attractive force along edges (higher pulls connected nodes together).");
                });
                ui.horizontal(|ui| {
                    ui.add(egui::Slider::new(&mut self.layout_state.base.c_repulse, 0.1..=3.0).text("c_repulse"));
                    info_icon(ui, "Multiplier for repulsive force between nodes (higher pushes nodes apart).");
                });

                ui.separator();
                ui.label("Extras");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.layout_state.extras.0.enabled, "center_gravity");
                    info_icon(ui, "Enable/disable center gravity force.");
                });
                ui.add_enabled_ui(self.layout_state.extras.0.enabled, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::Slider::new(&mut self.layout_state.extras.0.params.c, 0.0..=2.0).text("center_strength"));
                        info_icon(ui, "Coefficient for pull toward viewport/graph center.");
                    });
                });
            });
            ui.separator();
            self.render_path_controls(ui);
            ui.separator();
            self.render_edit_tools(ui);
            ui.separator();
            if ui.button("Print graph data").clicked() {
                println!("[app] Pressed print graph data button");
                println!("{}", self.graph.to_string())
            }
            if ui.button("Try build graph from store and print").clicked() {
                let merged = self.store.build_merged_view_with(&self.merge_config);
                match merged {
                    Ok(merged) => {
                        let graph = NetworkGraph::build_new(merged);
                        println!("[app] Pressed try build graph from store and print button");
                        println!("Fresh {}", graph.to_string())
                    }
                    Err(e) => {
                        println!("[app] Error building graph from store: {}", e);
                    }
                }
            }
            if ui.button("Print all node uuids").clicked() {
                println!("[app] Pressed print all node uuids button");
                for node in self.graph.graph.nodes_iter() {
                    let node = node.1.payload();
                    println!("{}", node.id);
                }
            }
            
            #[cfg(debug_assertions)]
            {
                ui.collapsing("egui debug", |ui| {
                    // Clone, edit via built-in UI, then apply:
                    let mut style = (*ctx.style()).clone();
                    style.debug.ui(ui); // renders controls for all DebugOptions
                    ctx.set_style(style);
                });
            }
        };

        SidePanel::right("right_panel").show(ctx, render_side_panel);

        CentralPanel::default().show(ctx, |ui| {
            egui_graphs::set_layout_state(ui, self.layout_state.clone(), None);

            // Reset area highlight and clear collector before drawing graph so shapes() will populate them during widget draw.
            clear_area_highlight();
            clear_label_overlays();

            let widget = &mut egui_graphs::GraphView::<
                Node,
                crate::network::edge::Edge,
                Directed,
                DefaultIx,
                NetworkGraphNodeShape,
                NetworkGraphEdgeShape,
                LayoutState,
                LayoutForceDirected<Layout>,
            >::new(&mut self.graph.graph)
            .with_navigations(
                &SettingsNavigation::default()
                    .with_zoom_and_pan_enabled(false)
                    .with_fit_to_screen_enabled(true),
            )
            .with_interactions(
                &SettingsInteraction::default()
                    .with_node_selection_enabled(true)
                    .with_edge_clicking_enabled(true)
                    .with_edge_selection_enabled(true),
            );

            edge_shape::clear_any_hit();
            edge_shape::clear_edge_events();

            // Add widget and obtain response so we can overlay labels afterwards.
            let _response = ui.add(widget);

            for ev in crate::gui::edge_shape::take_edge_events() {
                if matches!(self.edit_tool, EditTool::Snip) {
                    // Publish destruction animations for both directed edges
                    edge_anim::publish_destroy(ev.src_uuid, ev.dst_uuid, ev.kind);
                    edge_anim::publish_destroy(ev.dst_uuid, ev.src_uuid, ev.kind);
                    // Defer actual removal until the fade-out completes
                    self.pending_destroy
                        .push((ev.src_uuid, ev.dst_uuid, ev.kind, ev.is_manual));
                    ui.ctx().request_repaint();
                } else {
                    // Optional: set selected_edge to show properties panel
                    self.selected_edge = Some((ev.src_uuid, ev.dst_uuid, ev.kind));
                }
            }

            // Cleanup finished edge destroy animations and perform deferred removals
            {
                let fade_duration = std::time::Duration::from_millis(300);
                edge_anim::cleanup_finished(fade_duration);
                self.pending_destroy.retain(|(a, b, kind, is_manual)| {
                    let anim_ab = edge_anim::get_anim(*a, *b, *kind);
                    let anim_ba = edge_anim::get_anim(*b, *a, *kind);
                    let still_animating = anim_ab.is_some() || anim_ba.is_some();
                    if !still_animating {
                        if *is_manual {
                            self.graph.remove_manual_edge(*a, *b, *kind);
                        } else {
                            self.graph.supress_base_edge(*a, *b, *kind);
                        }
                        ui.ctx().request_repaint();
                    }
                    still_animating
                });
            }

            if matches!(self.edit_tool, EditTool::Draw) {
                // Cancel on escape or background click
                let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
                if esc {
                    self.draw_first = None;
                } else {
                    // If a click happened inside the graph and no node/edge was hit, cancel
                    // You can use the GraphView response rect to check pointer pos; here we rely on any_hit()
                    if !crate::gui::edge_shape::any_hit()
                        && ctx.input(|i| i.pointer.primary_released())
                    {
                        self.draw_first = None;
                    }
                }

                // Two-click node selection
                if let Some(&idx) = self.graph.graph.selected_nodes().first() {
                    match self.draw_first {
                        None => {
                            self.draw_first = Some(idx);
                        }
                        Some(a) if a != idx => {
                            // Validate Router <-> Network for Membership
                            let a_info = self.graph.graph.node(a).map(|n| n.payload().info.clone());
                            let b_info =
                                self.graph.graph.node(idx).map(|n| n.payload().info.clone());
                            let valid_membership = match (a_info, b_info) {
                                (
                                    Some(crate::network::node::NodeInfo::Router(_)),
                                    Some(crate::network::node::NodeInfo::Network(_)),
                                )
                                | (
                                    Some(crate::network::node::NodeInfo::Network(_)),
                                    Some(crate::network::node::NodeInfo::Router(_)),
                                ) => true,
                                _ => false,
                            };
                            if valid_membership {
                                let a_uuid = self.graph.graph.node(a).unwrap().payload().id;
                                let b_uuid = self.graph.graph.node(idx).unwrap().payload().id;
                                // Default metric 1; later make this user-configurable at creation
                                self.graph
                                    .add_manual_edge(a_uuid, b_uuid, EdgeKind::Membership, 1);
                                edge_anim::publish_create(a_uuid, b_uuid, EdgeKind::Membership);
                                edge_anim::publish_create(b_uuid, a_uuid, EdgeKind::Membership);
                            } else {
                                // Optional: feedback
                                eprintln!(
                                    "Invalid edge: only Router <-> Network membership supported."
                                );
                            }
                            self.draw_first = None;
                        }
                        _ => {}
                    }
                }
            }

            // Take the collected overlay labels and paint them on top of the graph widget.
            let labels: Vec<LabelOverlay> = take_label_overlays();
            if let Some(sel_idx) = self.selected_node {
                // Ensure the node is still selected in the underlying graph; if not, drop selection.
                let still_selected =
                    self.graph.graph.selected_nodes().first().map(|i| *i) == Some(sel_idx);
                if !still_selected {
                    self.selected_node = None;
                } else if let Some(first_overlay) = labels.into_iter().next() {
                    let id = Id::new(("node_panel", sel_idx.index()));
                    let panel = FloatingNodePanel::new(id, first_overlay.center);

                    let selected_node = self
                        .graph
                        .graph
                        .node(sel_idx)
                        .expect("Could not find selected node");
                    let render_node_label = |ui: &mut Ui, _ctx: &Context| {
                        let node_info = &selected_node.props().payload.info;
                        ui.label(format!("Node ID: {}", selected_node.payload().id));
                        if ui.button("Print serialized node data").clicked() {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(selected_node.payload()).unwrap()
                            );
                        }
                        match node_info {
                            NodeInfo::Router(router) => {
                                ui.label(format!("Router ID: {}", router.id));
                                protocol_data_section(ui, &router.protocol_data);
                            }
                            NodeInfo::Network(net) => {
                                ui.label(format!("Network prefix: {}", net.ip_address));
                                ui.label(format!("Network mask: {}", net.ip_address.mask()));
                                ui.separator();
                                collapsible_section(ui, "Attached router IDs", true, |ui| {
                                    bullet_list(ui, net.attached_routers.iter());
                                });
                                protocol_data_section(ui, &net.protocol_data);
                            }
                        }
                    };

                    let mut working_label = selected_node.label().to_string();
                    let resp = panel.show_with_label(ctx, &mut working_label, render_node_label);
                    if resp.label_changed {
                        if let Some(node) = self.graph.graph.node_mut(sel_idx) {
                            node.set_label(working_label);
                        }
                    }
                    if resp.close_clicked {
                        // Deselect node when panel is closed to prevent flicker on hover of other nodes.
                        self.selected_node = None;
                        // If the graph API provides a way to clear selection, call it here (left commented as placeholder):
                        // self.graph.graph.clear_selection();
                    }
                }
            }
        });

        // If a connect request is pending, request continuous repaints so render() keeps being called
        // and the background channels are polled until the result arrives. Without this, the UI may
        // stop repainting and never observe the channel message, leaving the buttons locked.
        if self.ssh_connect_pending || self.snmp_connect_pending {
            ctx.request_repaint();
        }
    }

    async fn switch_ssh_target(&mut self) {
        let client = SshClient::new_with_password(
            self.ssh_username.clone(),
            self.ssh_host.clone(),
            self.ssh_password.clone(),
            self.ssh_port,
        );
        let topo = match IsIsTopology::new_from_ssh_client(client).await {
            Ok(topo) => topo,
            Err(err) => {
                eprintln!("Failed to create IsIsTopology: {:?}", err);
                return;
            }
        };

        self.topo = Box::new(topo);

        self.refresh_from_source().await;
    }

    // Switch SNMP source target and refresh graph.
    #[deprecated]
    async fn switch_snmp_target(&mut self) {
        // Resolve host (IP or DNS)
        let addr = if let Ok(ip) = self.snmp_host.parse::<std::net::IpAddr>() {
            std::net::SocketAddr::new(ip, self.snmp_port)
        } else {
            match tokio::net::lookup_host((self.snmp_host.as_str(), self.snmp_port)).await {
                Ok(mut addrs) => addrs.next().unwrap_or_else(|| {
                    eprintln!("DNS lookup returned no addresses for {}", self.snmp_host);
                    std::net::SocketAddr::new(
                        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                        self.snmp_port,
                    )
                }),
                Err(e) => {
                    eprintln!(
                        "DNS lookup failed for {}:{} - {}",
                        self.snmp_host, self.snmp_port, e
                    );
                    return;
                }
            }
        };
        let client = crate::data_aquisition::snmp::SnmpClient::new(
            addr,
            &self.snmp_community,
            snmp2::Version::V2C,
            None,
        );
        self.topo = Box::new(OspfSnmpTopology::from_snmp_client(client));
        if self.clear_sources_on_switch {
            self.store = TopologyStore::default();
        }
        println!(
            "[app] Switched SNMP source to {}:{}",
            self.snmp_host, self.snmp_port
        );
        self.refresh_from_source().await;
    }

    // Replace the partition for the currently polled source and rebuild the graph from the union.
    async fn refresh_from_source(&mut self) {
        self.selected_node = None;

        let now = std::time::SystemTime::now();

        // Fetch SourceId first so we can mark it lost if node fetch fails.
        let snapshot = self.topo.fetch_snapshot().await;
        match snapshot {
            Ok((src_id, nodes, stats)) => {
                let rollback_state = self.store.get_source_state(&src_id).cloned();
                self.store
                    .replace_partition(&src_id, nodes, stats.clone(), now);
                // Route through authoritative reload_graph()
                if let Err(e) = self.reload_graph() {
                    eprintln!("Failed to build merged view: {:?}", e);
                    match rollback_state {
                        Some(state) => {
                            eprintln!("Found state from before merge, rollbacking");
                            let nodes = state
                                .partition
                                .nodes
                                .into_iter()
                                .map(|(_, node)| node)
                                .collect();
                            self.store.replace_partition(&src_id, nodes, stats, now);
                        }
                        None => {
                            eprintln!(
                                "No state found before merge, removing partition for {}",
                                src_id
                            );
                            _ = self.store.remove_partition(&src_id);
                        }
                    }
                } else {
                    println!("Merged view reconciled");
                }
            }
            Err(e) => {
                eprintln!("Failed to fetch snapshot: {:?}", e);
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.read_data();
        self.render(ctx);
        // update_data removed (direct edit applied in panel)
    }
}

fn info_icon(ui: &mut egui::Ui, tip: &str) {
    ui.add_space(4.0);
    ui.small_button("ℹ").on_hover_text(tip);
}

fn humanize_value(value: u64) -> (f64, String) {
    const UNITS: [&str; 11] = ["", "k", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q"];
    let mut current_value = value as f64;
    let mut unit_index = 0;

    while current_value >= 1000f64 && unit_index < UNITS.len() - 1 {
        current_value /= 1000f64;
        unit_index += 1;
    }

    (current_value, UNITS[unit_index].to_string())
}

fn humanize_bytes(bytes: u64) -> String {
    let (value, prefix) = humanize_value(bytes);

    format!("{:.2} {}B", value, prefix)
}

fn humanize_packet_count(count: u64) -> String {
    let (value, prefix) = humanize_value(count);

    format!("{:.2} {}pkts", value, prefix)
}
