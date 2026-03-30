use egui::{Align2, Context, FontId, Ui};
use egui_graphs::{LayoutForceDirected, SettingsInteraction, SettingsNavigation, SettingsStyle};
use petgraph::{Directed, csr::DefaultIx};

use crate::{
    gui::{
        actions::AppActions,
        edge_shape::NetworkGraphEdgeShape,
        graph_overlay,
        new_app::{AppPanel, Layout, LayoutState},
        node_shape::NetworkGraphNodeShape,
        tools::path_tool::PathToolState,
    },
    network::{
        edge::Edge,
        network_graph::NetworkGraph,
        node::{Node, NodeInfo},
    },
};

pub type GraphWidget<'a> = egui_graphs::GraphView<
    'a,
    Node,
    Edge,
    Directed,
    DefaultIx,
    NetworkGraphNodeShape,
    NetworkGraphEdgeShape,
    LayoutState,
    LayoutForceDirected<Layout>,
>;

pub struct GraphPanel {
    layout_state: LayoutState,
    edge_events: Vec<graph_overlay::EdgeEvent>,
    label_overlays: Vec<graph_overlay::LabelOverlay>,
}

impl Default for GraphPanel {
    fn default() -> Self {
        Self {
            layout_state: LayoutState::default(),
            edge_events: Vec::new(),
            label_overlays: Vec::new(),
        }
    }
}

impl GraphPanel {
    pub fn new(layout_state: LayoutState) -> Self {
        Self {
            layout_state,
            edge_events: Vec::new(),
            label_overlays: Vec::new(),
        }
    }

    pub fn layout_state(&self) -> &LayoutState {
        &self.layout_state
    }

    pub fn layout_state_mut(&mut self) -> &mut LayoutState {
        &mut self.layout_state
    }

    pub fn take_edge_events(&mut self) -> Vec<graph_overlay::EdgeEvent> {
        std::mem::take(&mut self.edge_events)
    }

    pub fn take_label_overlays(&mut self) -> Vec<graph_overlay::LabelOverlay> {
        std::mem::take(&mut self.label_overlays)
    }

    fn clear_per_frame_cache(&mut self, ctx: &Context, ui: &mut Ui, actions: &mut dyn AppActions) {
        let _ = (ui, actions);
        self.edge_events.clear();
        self.label_overlays.clear();
        graph_overlay::clear_frame_state(ctx);
    }

    fn render_graph(&mut self, ctx: &Context, ui: &mut Ui, actions: &mut dyn AppActions) {
        let _ = actions;
        egui_graphs::set_layout_state(ui, self.layout_state.clone(), None);
        graph_overlay::clear_frame_state(ctx);

        let graph = &mut actions.graph_mut().graph;

        {
            let node_idxs: Vec<_> = graph.nodes_iter().map(|(idx, _)| idx).collect();
            node_idxs.iter().for_each(|idx| {
                let node = graph.node_mut(*idx);
                if let Some(node) = node {
                    let label = match &node.payload().info {
                        NodeInfo::Router(r) => r.id.to_string(),
                        NodeInfo::Network(net) => net.ip_address.to_string(),
                    };
                    node.payload_mut().label = Some(label);
                }
            })
        }

        let widget = &mut GraphWidget::new(graph)
            .with_styles(&SettingsStyle::default().with_labels_always(true))
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

        let _response = ui.add(widget);

        self.edge_events = graph_overlay::take_edge_events(ctx);
        if !self.edge_events.is_empty() {
            ctx.request_repaint();
        }

        self.label_overlays = graph_overlay::take_label_overlays(ctx);
        for overlay in self.label_overlays.iter() {
            ui.painter().text(
                overlay.center,
                Align2::CENTER_CENTER,
                overlay.text.as_str(),
                FontId::proportional(12.0),
                overlay.color,
            );
        }

        PathToolState::with_state(ctx, |state| {
            let is_active =
                actions.get_active_tool() == Some(crate::gui::tools::tray::ToolId::PathTool);
            state.advance_frame(ctx, actions, is_active);
        });
    }
}

impl AppPanel for GraphPanel {
    fn ui(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        actions: &mut dyn crate::gui::actions::AppActions,
    ) {
        egui_graphs::set_layout_state(ui, self.layout_state.clone(), None);
        self.clear_per_frame_cache(ctx, ui, actions);
        self.render_graph(ctx, ui, actions);
    }
}
