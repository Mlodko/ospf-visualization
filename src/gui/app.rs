use std::collections::HashMap;
use std::sync::Arc;

use crate::data_aquisition::ssh::SshClient;
use crate::gui::node_panel::{
    FloatingNodePanel, bullet_list, collapsible_section, protocol_data_section,
};
use crate::network::node::NodeInfo;

use crate::parsers::isis_parser::topology::IsIsTopology;
use crate::topology::ospf_protocol::OspfFederator;
use crate::topology::protocol::FederationError;
use crate::topology::source::SnapshotSource;
use crate::topology::store::{
    AvailableFederators, MergeConfig, SourceId, SourceState, TopologyStore,
};
use crate::{
    gui::node_shape::{
        LabelOverlay, NetworkGraphNodeShape, clear_area_highlight, clear_label_overlays,
        partition_highlight_enabled, set_partition_highlight_enabled, take_label_overlays,
    },
    network::{network_graph::NetworkGraph, node::Node},
    topology::OspfSnmpTopology,
};
use eframe::egui;
use egui::{
    CentralPanel, Checkbox, CollapsingHeader, ComboBox, Context, Id, Separator, SidePanel, Ui,
};
use egui_extras::{Column, TableBuilder};
use egui_graphs::{
    DefaultEdgeShape, FruchtermanReingoldWithCenterGravity,
    FruchtermanReingoldWithCenterGravityState, LayoutForceDirected, SettingsInteraction,
    SettingsNavigation, SettingsStyle,
};
use petgraph::{Directed, csr::DefaultIx, graph::NodeIndex};
use tokio::runtime::Runtime;

pub fn main(rt: Arc<Runtime>) {
    let native_options = eframe::NativeOptions::default();
    let result = eframe::run_native(
        "My egui App",
        native_options,
        Box::new(|cc| {
            let app = rt.block_on(App::new(cc, rt.clone()));

            match app {
                Ok(app) => Ok(Box::new(app) as Box<dyn eframe::App>),
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

struct App {
    #[allow(unused)]
    topo: Box<dyn SnapshotSource>,
    store: TopologyStore,

    graph: NetworkGraph,

    selected_node: Option<NodeIndex>,
    #[allow(unused)]
    runtime: Arc<Runtime>,
    layout_state: LayoutState,

    // SNMP source switching state
    snmp_host: String,
    snmp_port: u16,
    snmp_community: String,
    clear_sources_on_switch: bool,
    // Quick & dirty: shared result storage for background SNMP connect -> snapshot result
    snmp_connect_res:
        std::sync::Arc<std::sync::Mutex<Option<Result<(SourceId, Vec<Node>), String>>>>,
    // Quick & dirty: flag indicating SNMP connect in progress
    snmp_connect_pending: bool,

    // SSH source switching state
    ssh_host: String,
    ssh_port: u16,
    ssh_username: String,
    ssh_password: String,
    ssh_clear_sources_on_switch: bool,
    // Quick & dirty: shared result storage for background SSH connect -> snapshot result
    ssh_connect_res:
        std::sync::Arc<std::sync::Mutex<Option<Result<(SourceId, Vec<Node>), String>>>>,
    // Quick & dirty: flag indicating SSH connect in progress
    ssh_connect_pending: bool,

    merge_config: MergeConfig,
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

    fn read_data(&mut self) {
        if let Some(node_index) = self.graph.graph.selected_nodes().first() {
            self.selected_node = Some(*node_index);
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
                                    state.last_snapshot.clone()
                                )
                            })
                            .collect();
                        rows.sort_by(|this, other| this.3.cmp(&other.3));

                        let mut sources_to_remove: Vec<SourceId> = Vec::new();
                        let mut source_enable_states: HashMap<SourceId, bool> = rows.iter().map(|(src_id, _, _, _)| {
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
                            .column(Column::auto().at_least(55.0))
                            .column(Column::auto().at_least(20.0));

                        table
                            .header(20.0, |mut header| {
                                header.col(|ui| { ui.strong("Source"); });
                                header.col(|ui| { ui.strong("Health"); });
                                header.col(|ui| { ui.strong("#Nodes"); });
                                header.col(|ui| { ui.strong("Last snapshot (s)"); });
                                header.col(|ui| { ui.strong("Actions"); });
                                header.col(|ui| { ui.strong("Enabled"); });
                            })
                            .body(|mut body| {
                                for (src_id, health, nodes_count, last_snapshot) in rows {
                                    body.row(22.0, |mut row| {
                                        row.col(|ui| { ui.label(src_id.to_string()); });
                                        row.col(|ui| { ui.label(health.to_string()); });
                                        row.col(|ui| { ui.label(nodes_count.to_string()); });
                                        row.col(|ui| { ui.label(humantime::format_rfc3339_seconds(last_snapshot).to_string()); });
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

    fn reload_graph(&mut self) -> Result<(), FederationError> {
        let merged = self.store.build_merged_view_with(&self.merge_config)?;

        self.graph.reconcile(merged);
        Ok(())
    }

    // update_data removed: label edits now apply directly via floating panel

    fn render(&mut self, ctx: &Context) {
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
            let mut guard = self.ssh_connect_res.lock().unwrap();
            if let Some(res) = guard.take() {
                match res {
                    Ok((src_id, nodes)) => {
                        println!("[app] SSH snapshot received in UI thread (via Arc<Mutex>)");
                        if self.ssh_clear_sources_on_switch {
                            self.store = TopologyStore::default();
                        }
                        let now = std::time::SystemTime::now();
                        self.store.replace_partition(&src_id, nodes, now);

                        // Print store count for diagnostics
                        let store_count = self.store.sources_iter().count();
                        match self.store.build_merged_view_with(&self.merge_config) {
                            Ok(merged) => {
                                let merged_count = merged.len();
                                println!(
                                    "[app] Applying merged view: sources={}, merged_nodes={}",
                                    store_count, merged_count
                                );
                                // Reconcile with the merged view
                                self.graph.reconcile(merged);
                            }
                            Err(e) => {
                                eprintln!(
                                    "[app] Error building merged view after SSH snapshot: {:?}",
                                    e
                                );
                                println!(
                                    "[app] Store sources count after SSH snapshot: {}",
                                    store_count
                                );
                            }
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
            let mut guard = self.snmp_connect_res.lock().unwrap();
            if let Some(res) = guard.take() {
                match res {
                    Ok((src_id, nodes)) => {
                        println!("[app] SNMP snapshot received in UI thread (via Arc<Mutex>)");
                        if self.clear_sources_on_switch {
                            self.store = TopologyStore::default();
                        }
                        let now = std::time::SystemTime::now();
                        self.store.replace_partition(&src_id, nodes, now);

                        // Print store count for diagnostics
                        let store_count = self.store.sources_iter().count();
                        match self.store.build_merged_view_with(&self.merge_config) {
                            Ok(merged) => {
                                let merged_count = merged.len();
                                println!(
                                    "[app] Applying merged view: sources={}, merged_nodes={}",
                                    store_count, merged_count
                                );
                                // Reconcile with the merged view
                                self.graph.reconcile(merged);
                            }
                            Err(e) => {
                                eprintln!(
                                    "[app] Error building merged view after SNMP snapshot: {:?}",
                                    e
                                );
                                println!(
                                    "[app] Store sources count after SNMP snapshot: {}",
                                    store_count
                                );
                            }
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
                                    SshClient::new_with_password(username, host, password, port);
                                println!("[bg-ssh async] created SSH client, creating topology");
                                match IsIsTopology::new_from_ssh_client(client).await {
                                    Ok(mut topo) => {
                                        println!("[bg-ssh async] topology created, fetching snapshot");
                                        match topo.fetch_snapshot().await {
                                            Ok((src_id, nodes)) => {
                                                println!("[bg-ssh async] snapshot fetch succeeded, src_id={:?}, nodes_count={}", src_id, nodes.len());
                                                Ok((src_id, nodes))
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
                                    addr,
                                    &community,
                                    snmp2::Version::V2C,
                                    None,
                                );
                                println!("[bg-snmp async] created SNMP client, building topology");
                                let mut topo = OspfSnmpTopology::from_snmp_client(client);
                                println!("[bg-snmp async] fetching snapshot from SNMP topology");
                                match topo.fetch_snapshot().await {
                                    Ok((src_id, nodes)) => {
                                        println!("[bg-snmp async] snapshot fetch succeeded src_id={:?}, nodes_count={}", src_id, nodes.len());
                                        Ok((src_id, nodes))
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

            ui.add(Separator::default());

            self.render_sources_section(ui);

            ui.add(Separator::default());

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

            ui.collapsing("egui debug", |ui| {
                // Clone, edit via built-in UI, then apply:
                let mut style = (*ctx.style()).clone();
                style.debug.ui(ui); // renders controls for all DebugOptions
                ctx.set_style(style);
            });
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
                DefaultEdgeShape,
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
                    .with_edge_clicking_enabled(true),
            );

            // Add widget and obtain response so we can overlay labels afterwards.
            let _response = ui.add(widget);

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
            Ok((src_id, nodes)) => {
                let rollback_state = self.store.get_source_state(&src_id).cloned();
                self.store.replace_partition(&src_id, nodes, now);
                let merged = self.store.build_merged_view_with(&self.merge_config);
                match merged {
                    Ok(merged) => {
                        self.graph.reconcile(merged);
                        println!("Merged view reconciled");
                    }
                    Err(e) => {
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
                                self.store.replace_partition(&src_id, nodes, now);
                            }
                            None => {
                                eprintln!(
                                    "No state found before merge, removing partition for {}",
                                    src_id
                                );
                                _ = self.store.remove_partition(&src_id);
                            }
                        }
                    }
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
