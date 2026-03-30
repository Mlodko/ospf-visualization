use std::sync::{Arc, Mutex};

use crate::gui::modules::dock::{DockActions, DockPanel, DockPanelKind, packet::{state::{Packet, PacketInspectorState}, ui::SemanticUi}};

pub struct SemanticPacketInspector {
    state: Arc<Mutex<PacketInspectorState>>
}

impl SemanticPacketInspector {
    pub fn new(state: Arc<Mutex<PacketInspectorState>>) -> Self {
        Self { state }
    }
}

impl DockPanel for SemanticPacketInspector {
    fn title(&self) -> egui::WidgetText {
        "Semantic Packet Inspector".into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, actions: &mut dyn crate::gui::actions::AppActions, dock_actions: &mut dyn DockActions) {
        let packet = {
            let state = self.state.lock().unwrap();
            state.packet().cloned()
        };
        
        match packet {
            Some(Packet::Ospf(lsa)) => {
                lsa.ui_semantic(ui, &mut |span| {
                    if let Ok(mut state) = self.state.lock() {
                        state.set_span(span);
                    }
                });
            }
            Some(Packet::IsIs(lsp)) => {
                lsp.ui_semantic(ui, &mut |_span| {});
            }
            None => {
                ui.centered_and_justified(|ui| ui.label("No packet selected."));
            }
        }
    }
    
    fn kind(&self) -> DockPanelKind {
        DockPanelKind::PacketInspectorSemantic
    }
}