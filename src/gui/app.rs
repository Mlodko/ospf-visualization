use eframe::egui;
use egui_graphs::{FruchtermanReingold, FruchtermanReingoldState, FruchtermanReingoldWithCenterGravityState, GraphView, SettingsInteraction, SettingsNavigation, SettingsStyle};
use petgraph::{Directed, Undirected, csr::DefaultIx};

use crate::{gui::node_shape::MyNodeShape, network::{network_graph::NetworkGraph, node::Node}};

pub fn main() {
    let native_options = eframe::NativeOptions::default();
    let _ = eframe::run_native("My egui App", native_options, Box::new(|cc| {
        // Run async initializer synchronously at startup.
        let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        let app = rt.block_on(App::new(cc));
        Ok(Box::new(app) as Box<dyn eframe::App>)
    }));
}

struct App {
    graph: NetworkGraph
}

impl App {
    async fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let _ = cc; // silence unused variable warning for now

        let mut snmp_client = crate::data_aquisition::snmp::SnmpClient::default();
        let nodes: Vec<Node> = crate::parsers::ospf_parser::snmp::query_router(&mut snmp_client).await.expect("Here").into_iter()
            .map(|entry| entry.try_into())
            .filter_map(|result| {
                match result {
                    Ok(node) => Some(node),
                    Err(_) => None,
                }
            })
            .collect();
        let graph = NetworkGraph::build_new(nodes);
        Self { graph }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
            egui::CentralPanel::default().show(ctx, |ui| {
                // pass the widget by value; give it a mutable reference to the internal graph
                type L = egui_graphs::LayoutForceDirected<egui_graphs::FruchtermanReingold>;
                type S = egui_graphs::FruchtermanReingoldState;
                let mut graph_view = egui_graphs::GraphView::<Node, crate::network::edge::Edge, Directed, DefaultIx, MyNodeShape, _, S, L>::new(&mut self.graph.graph)
                    .with_navigations(&SettingsNavigation::default().with_zoom_and_pan_enabled(true))
                    .with_interactions(&SettingsInteraction::default().with_node_selection_enabled(true))
                ;
                ui.add(&mut graph_view);
            });
        }
}
