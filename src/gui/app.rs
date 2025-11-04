use std::sync::Arc;

use eframe::egui;
use egui::{CentralPanel, Context, SidePanel, TextEdit};
use egui_graphs::{DefaultEdgeShape, FruchtermanReingold, FruchtermanReingoldState, LayoutForceDirected, SettingsInteraction, SettingsNavigation};
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

struct App {
    snmp_client: SnmpClient,
    graph: NetworkGraph,
    label_input: String,
    selected_node: Option<NodeIndex>,
    runtime: Arc<Runtime>,
    layout_state: FruchtermanReingoldState
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
        let mut layout_state = FruchtermanReingoldState::default();
        layout_state.c_repulse = 0.7;
        layout_state.k_scale = 0.5;
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
        });
        CentralPanel::default().show(ctx, |ui| {
            egui_graphs::set_layout_state(ui, self.layout_state.clone(), None);
            let widget = &mut egui_graphs::GraphView::<Node, crate::network::edge::Edge, Directed, DefaultIx, MyNodeShape, DefaultEdgeShape, /*FruchtermanReingoldState, LayoutForceDirected<FruchtermanReingold>*/>::new(&mut self.graph.graph)
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
