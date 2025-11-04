use std::sync::Arc;

use eframe::egui;
use egui::{CentralPanel, CollapsingHeader, Context, Separator, SidePanel, TextEdit};
use egui_graphs::{DefaultEdgeShape, FruchtermanReingold, FruchtermanReingoldState, FruchtermanReingoldWithCenterGravity, FruchtermanReingoldWithCenterGravityState, LayoutForceDirected, SettingsInteraction, SettingsNavigation};
use petgraph::{Directed, csr::DefaultIx, graph::NodeIndex};
use tokio::runtime::Runtime;
use crate::{data_aquisition::snmp::SnmpClient, gui::node_shape::MyNodeShape, network::{network_graph::NetworkGraph, node::Node}};

pub fn main(rt: Arc<Runtime>) {
    let native_options = eframe::NativeOptions::default();
    let result = eframe::run_native("My egui App", native_options, Box::new(|cc| {
        let app = rt.block_on(App::new(cc, rt.clone()));
        Ok(Box::new(app) as Box<dyn eframe::App>)
    }));
    
    if let Err(e) = result {
        println!("{}", e);
    }
}

type Layout = FruchtermanReingoldWithCenterGravity;
type LayoutState = FruchtermanReingoldWithCenterGravityState;

struct App {
    snmp_client: SnmpClient,
    graph: NetworkGraph,
    label_input: String,
    selected_node: Option<NodeIndex>,
    runtime: Arc<Runtime>,
    layout_state: LayoutState
}

impl App {
    async fn new(cc: &eframe::CreationContext<'_>, runtime: Arc<Runtime>) -> Self {
        let _ = cc; // silence unused variable warning for now

        let mut snmp_client = crate::data_aquisition::snmp::SnmpClient::default();
        let nodes: Vec<Node> = crate::parsers::ospf_parser::snmp::query_router(&mut snmp_client).await.expect("Here").into_iter()
            .map(|entry| entry.try_into())
            .filter_map(|result| {
                result.ok()
            })
            .collect();
        let graph = NetworkGraph::build_new(nodes);
        let layout_state = LayoutState::default();
        Self {snmp_client, graph , label_input: String::default(), selected_node: Option::default(), runtime, layout_state }
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
            
            self.graph.graph.node_mut(index).expect("Failed to get mutable node in update_data")
                .set_label(self.label_input.clone());
        }
    }
    
    fn render(&mut self, ctx: &Context) {
        SidePanel::right("right_panel").show(ctx, |ui| {
            ui.label("Change node label");
            ui.add_enabled_ui(self.selected_node.is_some(), |ui| {
                TextEdit::multiline(&mut self.label_input)
                    .hint_text("Select node")
                    .show(ui)
            });
            if ui.button("Reset and reload graph").clicked() {
                //let rt = tokio::runtime::Runtime::new().expect("Failed to initialize runtime");
                let rt = self.runtime.clone();
                rt.block_on(self.reset(ui));
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
            let widget = &mut egui_graphs::GraphView::<Node, crate::network::edge::Edge, Directed, DefaultIx, MyNodeShape, DefaultEdgeShape, LayoutState, LayoutForceDirected<Layout>>::new(&mut self.graph.graph)
                .with_navigations(&SettingsNavigation::default().with_zoom_and_pan_enabled(false).with_fit_to_screen_enabled(true))
                .with_interactions(&SettingsInteraction::default().with_node_selection_enabled(true))
            ;
            ui.add(widget);
        });
    }
    
    async fn reset(&mut self, ui: &mut egui::Ui) {
        println!("resetting");
        self.label_input = String::default();
        self.selected_node = Option::default();
        let nodes: Vec<Node> = crate::parsers::ospf_parser::snmp::query_router(&mut self.snmp_client).await.expect("Here").into_iter()
            .map(|entry| entry.try_into())
            .filter_map(|result| {
                result.ok()
            })
            .collect();
        let graph = NetworkGraph::build_new(nodes);
        println!("built new graph");
        self.graph = graph;
        egui_graphs::reset::<egui_graphs::LayoutStateRandom>(ui, None);
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
