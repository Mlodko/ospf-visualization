use std::{
    cell::RefCell,
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use catppuccin_egui::Theme;
use eframe::CreationContext;
use egui::{CentralPanel, CollapsingHeader, Context, Pos2, SidePanel, TopBottomPanel, Ui};
use egui_dock::DockState;
use egui_graphs::{
    FruchtermanReingoldWithCenterGravity, FruchtermanReingoldWithCenterGravityState,
};
use ipnetwork::IpNetwork;
use tokio::{runtime::Runtime, sync::Mutex, time::Instant};
use usvg::layout;
use uuid::Uuid;

use crate::{
    data_aquisition::{snmp::SnmpClient, ssh::SshClient},
    gui::{
        actions::{
            AppActions, ConnectActions, ConnectStatus, GraphActions, SourceActions, SourceSummary,
        },
        autopoll::{AutoPoller, PollMessage, ProtocolKind, SourceSpec},
        edge_shape,
        modules::{
            connection::ConnectionsPanel,
            dock::{
                Dock,
                packet::{
                    bytes::BytesPacketInspector, semantic::SemanticPacketInspector,
                    state::PacketInspectorState,
                },
            },
            graph::GraphPanel, theme::ThemeSelect,
        }, tools::{path_tool::PathTool, select_tool::SelectTool, tray::ToolsTray},
    },
    network::{
        edge::Edge,
        network_graph::NetworkGraph,
        node::{Node, NodeInfo},
        router::{InterfaceStats, RouterId},
    },
    parsers::isis_parser::topology::IsIsBfsTopology,
    topology::{
        ospf_protocol::OspfBfsSnmpTopology,
        source::SnapshotSource,
        store::{MergeConfig, SourceId, TopologyStore},
    },
};


thread_local! {
    static THEME: RefCell<ThemeId> = RefCell::new(ThemeId::Macchiato);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeId {
    Macchiato,
    Latte,
    Frappe,
    Mocha,
}

impl ThemeId {
    pub fn name(&self) -> &str {
        match self {
            ThemeId::Macchiato => "Macchiato",
            ThemeId::Latte => "Latte",
            ThemeId::Frappe => "Frappe",
            ThemeId::Mocha => "Mocha",
        }
    }
    
    pub fn all() -> impl Iterator<Item = ThemeId> {
        [
            ThemeId::Macchiato,
            ThemeId::Latte,
            ThemeId::Frappe,
            ThemeId::Mocha,
        ].into_iter()
    }
    
    pub fn theme(&self) -> Theme {
        match self {
            ThemeId::Macchiato => catppuccin_egui::MACCHIATO,
            ThemeId::Latte => catppuccin_egui::LATTE,
            ThemeId::Frappe => catppuccin_egui::FRAPPE,
            ThemeId::Mocha => catppuccin_egui::MOCHA,
        }
    }
}

pub fn get_theme() -> ThemeId {
    THEME.with(|theme| theme.borrow().clone())
}

pub fn set_theme(new_theme: ThemeId) {
    THEME.with(|theme| *theme.borrow_mut() = new_theme);
}

pub type PollResult = anyhow::Result<PollMessage>;
pub type Layout = FruchtermanReingoldWithCenterGravity;
pub type LayoutState = FruchtermanReingoldWithCenterGravityState;

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

struct App {
    // Topology stuffs
    topo: Box<dyn SnapshotSource>,
    store: TopologyStore,
    graph: NetworkGraph,

    // Runtime
    runtime: Arc<Runtime>,

    // Autopoller
    autopoller: AutoPoller,
    autopoller_enabled_gui: bool,

    // GUI modules
    connections_panel: ConnectionsPanel,
    ssh_connect_status: ConnectStatus,
    ssh_connect_result: Arc<Mutex<Option<PollResult>>>,
    snmp_connect_status: ConnectStatus,
    snmp_connect_result: Arc<Mutex<Option<PollResult>>>,
    graph_panel: GraphPanel,
    bottom_dock: Dock,
    tool_tray: ToolsTray,

    merge_config: MergeConfig,
}

impl App {
    async fn new(cc: &eframe::CreationContext<'_>, runtime: Arc<Runtime>) -> anyhow::Result<Self> {
        let _ = cc;
        let snmp_client = crate::data_aquisition::snmp::SnmpClient::default();
        let ssh_client = SshClient::new_with_password(
            "client".to_string(),
            "localhost".to_string(),
            "password".to_string(),
            2221,
        );
        let topo = IsIsBfsTopology::new_from_ssh_client(ssh_client)
            .await
            .unwrap();
        let topo: Box<dyn SnapshotSource> =
            //Box::new(OspfSnmpTopology::from_snmp_client(snmp_client));
            Box::new(topo);
        let store = TopologyStore::default();

        let merge_config = MergeConfig::default();

        let mut layout_state = LayoutState::default();
        layout_state.base.k_scale = 0.2;

        let autopoller = AutoPoller::new(None, runtime.clone());

        let inspector_state = PacketInspectorState::new_arc();
        let sem_inspect = Box::new(SemanticPacketInspector::new(inspector_state.clone()));
        let bytes_inspect = Box::new(BytesPacketInspector::new(inspector_state.clone()));
        let bottom_dock = Dock::new(vec![sem_inspect, bytes_inspect], Some(inspector_state));

        Ok(Self {
            topo,
            store,
            graph: NetworkGraph::default(),

            runtime,

            graph_panel: GraphPanel::new(layout_state),

            autopoller,
            autopoller_enabled_gui: false,

            connections_panel: ConnectionsPanel::default(),
            ssh_connect_status: ConnectStatus::Idle,
            ssh_connect_result: Arc::new(Mutex::new(None)),
            snmp_connect_status: ConnectStatus::Idle,
            snmp_connect_result: Arc::new(Mutex::new(None)),
            
            tool_tray: ToolsTray::new(Pos2::ZERO, vec![Box::new(SelectTool), Box::new(PathTool)]),

            merge_config,

            bottom_dock,
        })
    }
}

impl ConnectActions for App {
    fn ssh_connect_status(&self) -> &ConnectStatus {
        &self.ssh_connect_status
    }

    fn clear_ssh_connect_status(&mut self) {
        self.ssh_connect_status = ConnectStatus::Idle;
    }

    fn snmp_connect_status(&self) -> &ConnectStatus {
        &self.snmp_connect_status
    }

    fn clear_snmp_connect_status(&mut self) {
        self.snmp_connect_status = ConnectStatus::Idle;
    }

    fn connect_ssh(
        &mut self,
        host: String,
        port: u16,
        username: String,
        password: String,
        clear_other_sources: bool,
    ) {
        println!(
            "Connecting to {}:{}@{}:{}",
            &username, &password, &host, port
        );
        self.ssh_connect_status = ConnectStatus::Pending;

        if clear_other_sources {
            self.store = TopologyStore::default();
            self.autopoller.clear_source_specs();
        }

        let result_slot = self.ssh_connect_result.clone();
        let runtime = self.runtime.clone();

        let host_for_spec = host.clone();
        let username_for_spec = username.clone();
        let password_for_spec = password.clone();

        runtime.spawn(async move {
            let result: PollResult = async {
                let client =
                    SshClient::new_with_password(username, host_for_spec.clone(), password, port);

                let mut topo = IsIsBfsTopology::new_from_ssh_client(client)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to create ISIS topology: {}", e))?;

                let (source_id, nodes, if_stats) = topo
                    .fetch_snapshot()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to fetch snapshot: {}", e))?;

                let source_spec = SourceSpec::new_ssh(
                    host_for_spec,
                    port,
                    username_for_spec,
                    password_for_spec,
                    ProtocolKind::Isis,
                );

                Ok(PollMessage {
                    source_id,
                    nodes,
                    if_stats,
                    source_spec,
                })
            }
            .await;

            let mut guard = result_slot.lock().await;
            *guard = Some(result);
        });
    }

    fn connect_snmp(
        &mut self,
        host: String,
        port: u16,
        community: String,
        clear_other_sources: bool,
    ) {
        self.snmp_connect_status = ConnectStatus::Pending;

        if clear_other_sources {
            self.store = TopologyStore::default();
            self.autopoller.clear_source_specs();
        }

        let result_slot = self.snmp_connect_result.clone();
        let runtime = self.runtime.clone();

        let host_for_lookup = host.clone();
        let community_for_spec = community.clone();

        runtime.spawn(async move {
            let result: PollResult = async {
                let addr = if let Ok(ip) = host_for_lookup.parse::<IpAddr>() {
                    SocketAddr::new(ip, port)
                } else {
                    let mut addrs = tokio::net::lookup_host((host_for_lookup.as_str(), port))
                        .await
                        .map_err(|e| anyhow::anyhow!("DNS lookup failed: {}", e))?;
                    addrs
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("DNS lookup returned no addresses"))?
                };

                let client = SnmpClient::new(addr, &community, snmp2::Version::V2C, None);
                let mut topo = OspfBfsSnmpTopology::from_snmp_client(client)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to build OSPF topology: {}", e))?;

                let (source_id, nodes, if_stats) = topo
                    .fetch_snapshot()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to fetch snapshot: {}", e))?;

                let source_spec = SourceSpec::new_snmp(
                    addr,
                    community_for_spec,
                    snmp2::Version::V2C,
                    None,
                    ProtocolKind::Ospf,
                );

                Ok(PollMessage {
                    source_id,
                    nodes,
                    if_stats,
                    source_spec,
                })
            }
            .await;

            let mut guard = result_slot.lock().await;
            *guard = Some(result);
        });
    }
}

impl GraphActions for App {
    fn reload_graph(&mut self) -> anyhow::Result<()> {
        let merged = self.store.build_merged_view_with(&self.merge_config)?;

        self.graph.reconcile(merged);
        // Authoritatively recompute edge traffic weights after reconciling the graph
        self.apply_edge_traffic_weights();
        Ok(())
    }

    fn selected_node(&self) -> Option<&Node> {
        let node = self.graph
            .graph
            .selected_nodes()
            .first()
            .map(|node_idx| self.graph.graph.node(*node_idx).map(|node| node.payload()))
            .flatten();
        //println!("Selected node: {:?}", node);
        node
    }

    fn selected_edge(&self) -> Option<&Edge> {
        self.graph
            .graph
            .selected_edges()
            .first()
            .map(|edge_idx| self.graph.graph.edge(*edge_idx).map(|edge| edge.payload()))
            .flatten()
    }

    fn node_index_to_uuid(&self, index: petgraph::prelude::NodeIndex) -> Option<Uuid> {
        self.graph.graph.node(index)
            .map(|node| node.payload().id)
    }

    fn compute_path(&mut self, start: Uuid, end: Uuid) -> Option<(u32, Vec<Uuid>)> {
        if let (Some(start_idx), Some(end_idx)) = (self.graph.node_id_to_index_map.get(&start), self.graph.node_id_to_index_map.get(&end)) {
            petgraph::algo::astar(
                &self.graph.graph.g(),
                *start_idx,
                |node| node == *end_idx,
                |e|  {
                    (&e.weight().payload().metric).into()
                },
                |_| 0
            )
            .map(|(cost, path)| {
                let path = path
                    .into_iter()
                    .map(|node_idx| {
                        self.graph.graph.node(node_idx)
                            .expect("Node not found, this should never happen")
                            .payload()
                            .id
                    })
                    .collect();
                (cost, path)
            })
        } else { 
            None
        }
    }

    fn graph_mut(&mut self) -> &mut NetworkGraph {
        &mut self.graph
    }

    fn clear_node_selection(&mut self) {
        self.graph.graph.set_selected_nodes(vec![]);
    }
}

impl SourceActions for App {
    fn list_sources(&self) -> Vec<super::actions::SourceSummary> {
        self.store
            .sources_iter()
            .map(|(id, source)| SourceSummary {
                id: id.clone(),
                health: source.health.clone(),
                nodes_count: source.partition.nodes.len(),
                last_snapshot: source.last_snapshot,
                interface_stats: source.interface_stats.clone(),
            })
            .collect()
    }

    fn is_source_enabled(&self, id: &RouterId) -> bool {
        self.merge_config.is_source_enabled(id)
    }

    fn toggle_source(&mut self, src_id: &RouterId) {
        self.merge_config.toggle_source(src_id);
    }

    fn remove_source(&mut self, src_id: &RouterId) -> anyhow::Result<()> {
        self.store.remove_partition(src_id).map_err(|e| e.into())
    }

    fn store_to_string(&self) -> anyhow::Result<String> {
        serde_json::to_string_pretty(&self.store).map_err(|e| e.into())
    }

    fn source_to_string(&self, src_id: &RouterId) -> anyhow::Result<String> {
        let source = self
            .store
            .get_source_state(&src_id)
            .ok_or(anyhow::anyhow!("Source not found"))?;
        serde_json::to_string_pretty(source).map_err(|e| e.into())
    }

    fn source_summary(&self, id: &SourceId) -> Option<SourceSummary> {
        self.store.get_source_state(id).map(|source| SourceSummary {
            id: id.clone(),
            health: source.health.clone(),
            nodes_count: source.partition.nodes.len(),
            last_snapshot: source.last_snapshot,
            interface_stats: source.interface_stats.clone(),
        })
    }
}

impl AppActions for App {
    fn theme(&self) -> ThemeId {
        get_theme()
    }

    fn set_theme(&mut self, theme: ThemeId) {
        set_theme(theme);
    }

    fn get_active_tool(&self) -> Option<super::tools::tray::ToolId> {
        self.tool_tray.selected_tool
    }
}
pub trait AppPanel {
    /// Renders the panel UI
    fn ui(&mut self, ctx: &Context, ui: &mut Ui, actions: &mut dyn AppActions);
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        catppuccin_egui::set_theme(ctx, get_theme().theme());
        self.process_poll_results();

        SidePanel::right("right_panel")
            .resizable(true)
            .show(ctx, |ui| {
                self.render_side_panel(ctx, frame, ui);
                ui.collapsing("egui debug", |ui| {
                    // Clone, edit via built-in UI, then apply:
                    let mut style = (*ctx.style()).clone();
                    style.debug.ui(ui); // renders controls for all DebugOptions
                    ctx.set_style(style);
                });
            });

        let bottom_dock_response = TopBottomPanel::bottom("bottom_dock")
            .resizable(true)
            .default_height(240.0)
            .show(ctx, |ui| {
                self.render_bottom_dock(ctx, frame, ui);
            });
        let dock_height = bottom_dock_response.response.rect.height();
        self.bottom_dock.overlay_add_tab_menu(ctx, dock_height);

        CentralPanel::default().show(ctx, |ui| {
            let mut graph_panel = std::mem::take(&mut self.graph_panel);
            graph_panel.ui(ctx, ui, self);
            self.graph_panel = graph_panel;
        });
        
        {
            let mut tool_tray = std::mem::take(&mut self.tool_tray);
            tool_tray.ui(ctx, self);
            self.tool_tray = tool_tray;
        }
    }
}

impl App {
    fn process_poll_results(&mut self) {
        fn apply_poll(app: &mut App, msg: PollMessage) {
            let source_id = msg.source_id.clone();
            let nodes = msg.nodes;
            let if_stats = msg.if_stats;
            let source_spec = msg.source_spec;

            app.autopoller.add_source(source_id.clone(), source_spec);
            let now = std::time::SystemTime::now();
            app.store
                .replace_partition(&source_id, nodes, if_stats, now);

            if let Err(e) = app.reload_graph() {
                eprintln!("[new_app] Failed to reload graph after snapshot: {}", e);
            }
        }

        let mut pending = Vec::new();

        if let Ok(mut guard) = self.ssh_connect_result.try_lock() {
            if let Some(res) = guard.take() {
                match res {
                    Ok(msg) => {
                        pending.push(msg);
                        self.ssh_connect_status = ConnectStatus::Success;
                    }
                    Err(err) => {
                        self.ssh_connect_status = ConnectStatus::Failure(err.to_string());
                    }
                }
            }
        }

        if let Ok(mut guard) = self.snmp_connect_result.try_lock() {
            if let Some(res) = guard.take() {
                match res {
                    Ok(msg) => {
                        pending.push(msg);
                        self.snmp_connect_status = ConnectStatus::Success;
                    }
                    Err(err) => {
                        self.snmp_connect_status = ConnectStatus::Failure(err.to_string());
                    }
                }
            }
        }

        if let Some(rx) = self.autopoller.poll_rx_mut() {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Ok(poll) => pending.push(poll),
                    Err(err) => eprintln!("[new_app] autopoll failed: {}", err),
                }
            }
        }

        for msg in pending {
            apply_poll(self, msg);
        }
    }

    fn render_side_panel(&mut self, ctx: &Context, frame: &mut eframe::Frame, ui: &mut Ui) {
        CollapsingHeader::new("Connections")
            .default_open(true)
            .show(ui, |ui| {
                let mut connection_panel = std::mem::take(&mut self.connections_panel);
                connection_panel.ui(ctx, ui, self);
                self.connections_panel = connection_panel;
            });
        { 
            let mut autopoller = std::mem::take(&mut self.autopoller);
            autopoller.ui(ctx, ui, self);
            self.autopoller = autopoller;
        }
        ThemeSelect::ui(ctx, ui, self);
    }

    fn render_bottom_dock(&mut self, ctx: &Context, frame: &mut eframe::Frame, ui: &mut Ui) {
        let mut bottom_dock = std::mem::take(&mut self.bottom_dock);
        bottom_dock.ui(ui, self);
        self.bottom_dock = bottom_dock;
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

            let mut prefix_to_dst_uuid: HashMap<IpNetwork, Uuid> = edges
                .iter()
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
                edge_shape::insert_edge_weight(src_uuid, dst_uuid, weight);
            }
        }
    }
}
