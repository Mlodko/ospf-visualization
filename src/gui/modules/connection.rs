use egui::CollapsingHeader;

use crate::gui::actions::ConnectStatus;
use crate::gui::new_app::AppPanel;

#[derive(Default)]
struct GnmiConnectionFields {
    uri: String,
    username: String,
    password: String,
    source_id: String,
    interface_stats: String,
    clear_sources_on_switch: bool,
}

/// Handles the panel for connecting to gNMI sources.
pub struct ConnectionsPanel {
    ospf_gnmi: GnmiConnectionFields,
    isis_gnmi: GnmiConnectionFields,
}

impl Default for ConnectionsPanel {
    fn default() -> Self {
        Self {
            ospf_gnmi: GnmiConnectionFields {
                uri: "http://127.0.0.1:57401".to_string(),
                username: "admin".to_string(),
                source_id: "0.0.0.0".to_string(),
                ..Default::default()
            },
            isis_gnmi: GnmiConnectionFields {
                uri: "http://127.0.0.1:57401".to_string(),
                username: "admin".to_string(),
                source_id: "0000.0000.0000".to_string(),
                ..Default::default()
            },
        }
    }
}

impl ConnectionsPanel {
    fn render_gnmi(
        _ctx: &egui::Context,
        ui: &mut egui::Ui,
        actions: &mut dyn crate::gui::actions::AppActions,
        fields: &mut GnmiConnectionFields,
        protocol: crate::gui::autopoll::ProtocolKind,
    ) {
        ui.label("Endpoint URI");
        ui.text_edit_singleline(&mut fields.uri);
        ui.label("Username");
        ui.text_edit_singleline(&mut fields.username);
        ui.label("Password");
        ui.add(egui::TextEdit::singleline(&mut fields.password).password(true));
        ui.label("Router ID");
        ui.text_edit_singleline(&mut fields.source_id);
        ui.label("Interface stats: ip,tx_bytes,tx_packets,rx_bytes,rx_packets per line");
        ui.label("Use - for an unavailable counter.");
        ui.text_edit_multiline(&mut fields.interface_stats);
        ui.checkbox(
            &mut fields.clear_sources_on_switch,
            "Clear previous sources on connect",
        );

        match actions.gnmi_connect_status().clone() {
            ConnectStatus::Pending => {
                ui.add_enabled_ui(false, |ui| ui.button("Connecting..."));
            }
            ConnectStatus::Failure(error) => {
                ui.colored_label(egui::Color32::RED, error);
                if ui.button("Connect").clicked() {
                    actions.connect_gnmi(
                        fields.uri.clone(),
                        fields.username.clone(),
                        fields.password.clone(),
                        fields.source_id.clone(),
                        fields.interface_stats.clone(),
                        protocol.clone(),
                        fields.clear_sources_on_switch,
                    );
                }
            }
            ConnectStatus::Idle | ConnectStatus::Success => {
                if matches!(actions.gnmi_connect_status(), ConnectStatus::Success) {
                    ui.label("gNMI connected");
                }
                if ui.button("Connect").clicked() {
                    actions.connect_gnmi(
                        fields.uri.clone(),
                        fields.username.clone(),
                        fields.password.clone(),
                        fields.source_id.clone(),
                        fields.interface_stats.clone(),
                        protocol.clone(),
                        fields.clear_sources_on_switch,
                    );
                }
            }
        }
    }
}

impl AppPanel for ConnectionsPanel {
    fn ui(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        actions: &mut dyn crate::gui::actions::AppActions,
    ) {
        CollapsingHeader::new("Connect")
            .default_open(true)
            .show(ui, |ui| {
                CollapsingHeader::new("OSPF via gNMI")
                    .default_open(true)
                    .show(ui, |ui| {
                        Self::render_gnmi(
                            ctx,
                            ui,
                            actions,
                            &mut self.ospf_gnmi,
                            crate::gui::autopoll::ProtocolKind::Ospf,
                        );
                    });
                CollapsingHeader::new("IS-IS via gNMI")
                    .default_open(true)
                    .show(ui, |ui| {
                        Self::render_gnmi(
                            ctx,
                            ui,
                            actions,
                            &mut self.isis_gnmi,
                            crate::gui::autopoll::ProtocolKind::Isis,
                        );
                    });
            });
    }
}
