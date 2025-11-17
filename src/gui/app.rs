use std::{sync::Arc, time::Instant};

use crate::gui::node_panel::{FloatingNodePanel, collapsible_section, label_editor_row};
use crate::topology::source::SnapshotSource;
use crate::{
    gui::node_shape::{LabelOverlay, MyNodeShape, clear_label_overlays, take_label_overlays},
    network::{network_graph::NetworkGraph, node::Node},
    topology::{OspfSnmpTopology},
};
use crate::topology::store::TopologyStore;
use eframe::egui;
use egui::{CentralPanel, CollapsingHeader, Context, Id, Separator, SidePanel, TextEdit};
use egui_graphs::{
    DefaultEdgeShape, FruchtermanReingoldWithCenterGravity,
    FruchtermanReingoldWithCenterGravityState, LayoutForceDirected, SettingsInteraction,
    SettingsNavigation,
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
                Err(e) => Err(e.into())
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
    topo: Box<dyn SnapshotSource>,
    store: TopologyStore,
    live_view_only: bool,

    graph: NetworkGraph,
    label_input: String,
    selected_node: Option<NodeIndex>,
    runtime: Arc<Runtime>,
    layout_state: LayoutState,
}

impl App {
    async fn new(
        cc: &eframe::CreationContext<'_>,
        runtime: Arc<Runtime>,
    ) -> Result<Self, RuntimeError> {
        let _ = cc; // silence unused variable warning for now

        let snmp_client = crate::data_aquisition::snmp::SnmpClient::default();
        let mut topo: Box<dyn SnapshotSource> = Box::new(OspfSnmpTopology::new(snmp_client));
        let mut store = TopologyStore::default();

        // First snapshot: replace partition and build union (live-only)
        let now = Instant::now();
        let (src, nodes) = topo
            .fetch_snapshot()
            .await
            .map_err(|e| RuntimeError::TopologyFetchError(e.to_string()))?;
        store.replace_partition(src, nodes, now);

        let merged: Vec<Node> = store.union_nodes(true);
        let graph = NetworkGraph::build_new(merged);
        let layout_state = LayoutState::default();

        Ok(Self {
            topo,
            store,
            live_view_only: true,
            graph,
            label_input: String::default(),
            selected_node: Option::default(),
            runtime,
            layout_state,
        })
    }

    fn read_data(&mut self) {
        if let Some(node_index) = self.graph.graph.selected_nodes().first() {
            self.selected_node = Some(*node_index);
            self.label_input = self.graph.graph.node(*node_index).unwrap().label();
        }
    }

    fn update_data(&mut self) {
        if let Some(index) = self.selected_node {
            if index.index().to_string() == self.label_input {
                return;
            }

            self.graph
                .graph
                .node_mut(index)
                .expect("Failed to get mutable node in update_data")
                .set_label(self.label_input.clone());
        }
    }

    fn render(&mut self, ctx: &Context) {
        SidePanel::right("right_panel").show(ctx, |ui| {
            // Live vs merged toggle
            if ui.checkbox(&mut self.live_view_only, "Live view (only connected sources)").changed()
            {
                let merged = self.store.union_nodes(self.live_view_only);
                self.graph = NetworkGraph::build_new(merged);
            }

            ui.separator();
            ui.label("Change node label");
            ui.add_enabled_ui(self.selected_node.is_some(), |ui| {
                TextEdit::multiline(&mut self.label_input)
                    .hint_text("Select node")
                    .show(ui)
            });
            if ui.button("Poll source and rebuild graph").clicked() {
                let rt = self.runtime.clone();
                rt.block_on(self.refresh_from_source(ui));
            }
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
        });

        CentralPanel::default().show(ctx, |ui| {
            egui_graphs::set_layout_state(ui, self.layout_state.clone(), None);

            // Clear collector before drawing graph so shapes() will populate it during widget draw.
            clear_label_overlays();

            let widget = &mut egui_graphs::GraphView::<
                Node,
                crate::network::edge::Edge,
                Directed,
                DefaultIx,
                MyNodeShape,
                DefaultEdgeShape,
                LayoutState,
                LayoutForceDirected<Layout>,
            >::new(&mut self.graph.graph)
            .with_navigations(
                &SettingsNavigation::default()
                    .with_zoom_and_pan_enabled(false)
                    .with_fit_to_screen_enabled(true),
            )
            .with_interactions(&SettingsInteraction::default().with_node_selection_enabled(true));

            // Add widget and obtain response so we can overlay labels afterwards.
            let _response = ui.add(widget);

            // Take the collected overlay labels and paint them on top of the graph widget.
            let labels: Vec<LabelOverlay> = take_label_overlays();
            if !labels.is_empty() {
                for (i, lbl) in labels.into_iter().enumerate() {
                    if self.selected_node.is_some() && lbl.text == self.label_input {
                        let id = Id::new(("node_panel", i, &lbl.text));
                        let panel = FloatingNodePanel::new(id, lbl.center).title("Network");
                        let _resp = panel.show(ctx, |ui, _ctx| {
                            // Integrated label input
                            let _ = label_editor_row(ui, &mut self.label_input);
                            ui.add_space(8.0);

                            // Modular, extensible sections
                            collapsible_section(ui, "Connected routers", true, |ui| {
                                ui.label("No data");
                            });

                            ui.add_space(8.0);

                            collapsible_section(ui, "Protocol Data", true, |ui| {
                                collapsible_section(ui, "OSPF", true, |ui| {
                                    ui.label("No data");
                                });
                            });
                        });
                    }
                }
            }
        });
    }

    // Replace the partition for the currently polled source and rebuild the graph from the union.
    async fn refresh_from_source(&mut self, ui: &mut egui::Ui) {
        self.label_input.clear();
        self.selected_node = None;

        let now = Instant::now();

        // Fetch SourceId first so we can mark it lost if node fetch fails.
        let src_id = self.topo.fetch_source_id().await;
        match src_id {
            Ok(src) => {
                match self.topo.fetch_nodes().await {
                    Ok(nodes) => {
                        self.store.replace_partition(src, nodes, now);
                        let merged = self.store.union_nodes(self.live_view_only);
                        self.graph = NetworkGraph::build_new(merged);
                        egui_graphs::reset::<egui_graphs::LayoutStateRandom>(ui, None);
                    }
                    Err(e) => {
                        // Acquisition failed after we identified the source: mark it lost and rebuild.
                        self.store.mark_lost(&src, now);
                        let merged = self.store.union_nodes(self.live_view_only);
                        self.graph = NetworkGraph::build_new(merged);
                        eprintln!("Failed to fetch topology nodes: {:?}", e);
                    }
                }
            }
            Err(e) => {
                // Could not even read the SourceId; nothing to update, just log.
                eprintln!("Failed to fetch source id: {:?}", e);
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.read_data();
        self.render(ctx);
        self.update_data();
    }
}

fn info_icon(ui: &mut egui::Ui, tip: &str) {
    ui.add_space(4.0);
    ui.small_button("ℹ").on_hover_text(tip);
}
