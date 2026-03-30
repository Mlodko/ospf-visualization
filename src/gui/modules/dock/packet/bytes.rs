use std::sync::{Arc, Mutex};

use egui::{FontId, RichText, TextStyle};

use crate::gui::modules::dock::{
    DockActions, DockPanel, DockPanelKind, packet::{parse::span::Span, state::{Packet, PacketInspectorState}}
};

pub struct BytesPacketInspector {
    state: Arc<Mutex<PacketInspectorState>>,
}

impl BytesPacketInspector {
    pub fn new(state: Arc<Mutex<PacketInspectorState>>) -> Self {
        BytesPacketInspector { state }
    }
}

impl DockPanel for BytesPacketInspector {
    fn title(&self) -> egui::WidgetText {
        "Raw Packet Inspector".into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _actions: &mut dyn crate::gui::actions::AppActions, dock_actions: &mut dyn DockActions) {
        let (raw, span) = {
            let state = self.state.lock().unwrap();
            match state.packet() {
                Some(Packet::Ospf(lsa)) => {
                    let raw = lsa.raw.clone();
                    let span = state.span().copied();
                    (raw, span)
                }
                Some(Packet::IsIs(_)) => {
                    ui.centered_and_justified(|ui| ui.label("Raw LSP not available"));
                    return;
                }
                None => {
                    ui.centered_and_justified(|ui| ui.label("No packet selected"));
                    return;
                }
            }
        };

        render_hex(ui, &raw, span);
    }
    
    fn kind(&self) -> DockPanelKind {
        DockPanelKind::PacketInspectorBytes
    }
}

fn render_hex(ui: &mut egui::Ui, bytes: &[u8], span: Option<Span>) {
    let font_size = ui
        .style()
        .text_styles
        .get(&TextStyle::Monospace)
        .map(|f| f.size)
        .unwrap_or(12.0);
    let font_id = FontId::monospace(font_size);

    let spacing = ui.spacing().item_spacing.x;
    let cell_width = ui.ctx().fonts_mut(|f| {
        f.layout_no_wrap("00 ".into(), font_id.clone(), egui::Color32::WHITE)
            .size()
            .x
    }) + spacing;
    let offset_width = ui.ctx().fonts_mut(|f| {
        f.layout_no_wrap("0000: ".into(), font_id.clone(), egui::Color32::WHITE)
            .size()
            .x
    }) + spacing;

    let usable = (ui.available_width() - offset_width).max(0.0);
    let bytes_per_row = (usable / cell_width).floor().max(1.0) as usize;

    let (sel_start, sel_end) = span
        .map(|s| (s.start, s.end))
        .unwrap_or((usize::MAX, usize::MAX));

    for (row_idx, chunk) in bytes.chunks(bytes_per_row).enumerate() {
        let base = row_idx * bytes_per_row;
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{:04x}:", base)).font(font_id.clone()));

            for (i, b) in chunk.iter().enumerate() {
                let idx = base + i;
                let highlighted = idx >= sel_start && idx < sel_end;

                let text = format!("{:02x} ", b);
                if highlighted {
                    ui.label(
                        RichText::new(text)
                            .font(font_id.clone())
                            .background_color(ui.visuals().selection.bg_fill)
                            .strong(),
                    );
                } else {
                    ui.label(RichText::new(text).font(font_id.clone()));
                }
            }
        });
    }
}
