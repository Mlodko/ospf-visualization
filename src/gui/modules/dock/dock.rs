use std::sync::{Arc, Mutex};
use std::vec::Drain;

use egui::{Align2, Area, Order, Vec2, WidgetText};
use egui_dock::{DockArea, DockState, TabViewer};

use crate::gui::actions::AppActions;
use crate::gui::modules::dock::node_inspector::NodeInspectorDockPanel;
use crate::gui::modules::dock::packet::bytes::BytesPacketInspector;
use crate::gui::modules::dock::packet::semantic::SemanticPacketInspector;
use crate::gui::modules::dock::packet::state::PacketInspectorState;
use crate::gui::modules::dock::sources::SourcesPanel;
use crate::gui::new_app::AppPanel;

/// Main dock panel for the GUI. Defines a dockable area in the GUI and holds its contents.
pub struct Dock {
    dock_state: DockState<Box<dyn DockPanel>>,
    packet_inspector_state: Arc<Mutex<PacketInspectorState>>,
    actions_queue: DockActionQueue,
}

impl Default for Dock {
    fn default() -> Self {
        Self {
            dock_state: DockState::new(vec![]),
            packet_inspector_state: Default::default(),
            actions_queue: Default::default(),
        }
    }
}

impl Dock {
    pub fn new(
        initial_tabs: Vec<Box<dyn DockPanel>>,
        packet_inspector_state: Option<Arc<Mutex<PacketInspectorState>>>,
    ) -> Self {
        let dock_state = DockState::new(initial_tabs);
        let packet_inspector_state =
            packet_inspector_state.unwrap_or_else(|| PacketInspectorState::new_arc());
        Self {
            dock_state,
            packet_inspector_state,
            actions_queue: Default::default(),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, actions: &mut dyn AppActions) {
        self.update_data(actions);
        let dock_style = egui_dock::Style::from_egui(ui.style().as_ref());
        let mut viewer = PanelViewer {
            actions,
            dock_actions: &mut self.actions_queue,
        };
        DockArea::new(&mut self.dock_state)
            .style(dock_style)
            .show_inside(ui, &mut viewer);
        self.process_queue();
    }

    pub fn overlay_add_tab_menu(&mut self, ctx: &egui::Context, dock_height: f32) {
        let kinds = DockPanelKind::all();
        Area::new("dock_add_tab_menu".into())
            .anchor(Align2::LEFT_BOTTOM, Vec2::new(8.0, -dock_height - 8.0))
            .order(Order::Foreground)
            .show(ctx, |ui| {
                ui.menu_button("+", |ui| {
                    for kind in kinds {
                        if ui.button(kind.label()).clicked() {
                            self.actions_queue.focus_or_open(*kind);
                            ui.close();
                        }
                    }
                });
            });
    }

    pub fn dock_state_mut(&mut self) -> &mut DockState<Box<dyn DockPanel>> {
        &mut self.dock_state
    }

    fn update_data(&mut self, actions: &mut dyn AppActions) {
        self.update_packet_inspector_state(actions);
    }

    fn update_packet_inspector_state(&mut self, actions: &mut dyn AppActions) {
        if let Some(node) = actions.selected_node() {
            let mut state = self.packet_inspector_state.lock().unwrap();
            let _ = state.set_packet_from_node(node);
        }
    }

    fn process_queue(&mut self) {
        while let Some(request) = self.actions_queue.pop() {
            match request {
                DockRequest::CloseTab(kind) => {
                    self.close_tab(kind);
                }
                DockRequest::AddTab(panel) => {
                    self.add_tab(panel);
                }
                DockRequest::FocusOrOpen(kind) => {
                    self.focus_or_open(kind);
                }
            }
        }
    }

    fn close_tab(&mut self, kind: DockPanelKind) {
        let tab_to_close = self
            .dock_state
            .iter_all_tabs()
            .find(|((_, _), tab)| tab.kind() == kind);
        if let Some(((surface_idx, node_idx), _)) = tab_to_close {
            self.dock_state.remove_leaf((surface_idx, node_idx));
        } else {
            return;
        }
    }

    fn add_tab(&mut self, panel: Box<dyn DockPanel>) {
        self.dock_state.add_window(vec![panel]);
    }

    fn focus_or_open(&mut self, kind: DockPanelKind) {
        let tab_to_focus = self
            .dock_state
            .iter_all_tabs()
            .find(|((_, _), tab)| tab.kind() == kind);
        if let Some(((surface_idx, node_idx), _)) = tab_to_focus {
            self.dock_state
                .set_focused_node_and_surface((surface_idx, node_idx));
        } else {
            let panel = kind.into_panel(self.packet_inspector_state.clone());
            self.add_tab(panel);
        }
    }
}

enum DockRequest {
    CloseTab(DockPanelKind),
    AddTab(Box<dyn DockPanel>),
    FocusOrOpen(DockPanelKind),
}

#[derive(Default)]
struct DockActionQueue {
    requests: Vec<DockRequest>,
}

impl DockActionQueue {
    fn new() -> Self {
        Self {
            requests: Vec::new(),
        }
    }

    fn push(&mut self, request: DockRequest) {
        self.requests.push(request);
    }

    fn pop(&mut self) -> Option<DockRequest> {
        self.requests.pop()
    }

    fn drain(&mut self) -> Drain<DockRequest> {
        self.requests.drain(..)
    }
}

impl DockActions for DockActionQueue {
    fn close_tab(&mut self, kind: DockPanelKind) {
        self.push(DockRequest::CloseTab(kind))
    }

    fn add_tab(&mut self, panel: Box<dyn DockPanel>) {
        self.push(DockRequest::AddTab(panel))
    }

    fn focus_or_open(&mut self, kind: DockPanelKind) {
        self.push(DockRequest::FocusOrOpen(kind))
    }
}

pub trait DockActions {
    fn close_tab(&mut self, kind: DockPanelKind);
    fn add_tab(&mut self, panel: Box<dyn DockPanel>);
    fn focus_or_open(&mut self, kind: DockPanelKind);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DockPanelKind {
    PacketInspectorBytes,
    PacketInspectorSemantic,
    NodeInspector,
    Sources,
}

impl DockPanelKind {
    pub fn into_panel(self, packet_state: Arc<Mutex<PacketInspectorState>>) -> Box<dyn DockPanel> {
        match self {
            DockPanelKind::PacketInspectorBytes => {
                Box::new(BytesPacketInspector::new(packet_state))
            }
            DockPanelKind::PacketInspectorSemantic => {
                Box::new(SemanticPacketInspector::new(packet_state))
            }
            DockPanelKind::NodeInspector => Box::new(NodeInspectorDockPanel),
            DockPanelKind::Sources => Box::new(SourcesPanel),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            DockPanelKind::PacketInspectorBytes => "Packet Inspector (Bytes)",
            DockPanelKind::PacketInspectorSemantic => "Packet Inspector (Semantic)",
            DockPanelKind::NodeInspector => "Node Inspector",
            DockPanelKind::Sources => "Sources",
        }
    }

    pub fn all() -> &'static [DockPanelKind] {
        &[
            DockPanelKind::PacketInspectorBytes,
            DockPanelKind::PacketInspectorSemantic,
            DockPanelKind::NodeInspector,
            DockPanelKind::Sources,
        ]
    }
}

/// Trait for panels that can be used in the Dock.
pub trait DockPanel {
    fn title(&self) -> WidgetText;
    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        actions: &mut dyn AppActions,
        dock_actions: &mut dyn DockActions,
    );
    fn kind(&self) -> DockPanelKind;
}

struct PanelViewer<'a> {
    actions: &'a mut dyn AppActions,
    dock_actions: &'a mut dyn DockActions,
}

impl<'a> TabViewer for PanelViewer<'a> {
    type Tab = Box<dyn DockPanel>;

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        tab.title()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        tab.ui(ui, self.actions, self.dock_actions);
    }
}
