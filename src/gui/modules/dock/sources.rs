use egui::{Checkbox, Ui};
use egui_extras::{Column, TableBuilder};

use crate::gui::modules::dock::{DockActions, DockPanel, DockPanelKind};

#[derive(Default)]
pub struct SourcesPanel;

impl DockPanel for SourcesPanel {
    fn title(&self) -> egui::WidgetText {
        "Sources".into()
    }
    
    fn kind(&self) -> super::DockPanelKind {
        DockPanelKind::Sources
    }

    fn ui(&mut self, ui: &mut egui::Ui, actions: &mut dyn crate::gui::actions::AppActions, dock_actions: &mut dyn DockActions) {
        egui::ScrollArea::vertical()
            .show(ui, |ui| {
                if ui.button("Print store data").clicked() {
                    println!("[dock::sources] Pressed print store data button");
                    let json = actions.store_to_string();
                    match json {
                        Ok(json) => println!("{}", json),
                        Err(err) => println!("Error serializing store data: {}", err)
                    }
                }
                
                let mut sources = actions.list_sources();
                sources.sort_by(|this, other| this.id.as_string().cmp(&other.id.as_string()));
                
                let table = TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .column(Column::auto().at_least(70.0))
                    .column(Column::auto().at_least(70.0))
                    .column(Column::auto().at_least(55.0))
                    .column(Column::auto().at_least(145.0))
                    .column(Column::auto().at_least(40.0))
                    .column(Column::auto().at_least(55.0))
                    .column(Column::auto().at_least(20.0));
                
                table
                    .header(20.0, |mut header| {
                        header.col(|ui| { ui.strong("Source"); });
                        header.col(|ui| { ui.strong("Health"); });
                        header.col(|ui| { ui.strong("#Nodes"); });
                        header.col(|ui| { ui.strong("Last snapshot (s)"); });
                        header.col(|ui| { ui.strong("IfStats"); });
                        header.col(|ui| { ui.strong("Actions"); });
                        header.col(|ui| { ui.strong("Enabled"); });
                    })
                    .body(|mut body| {
                        for source in sources {
                            body.row(22.0, |mut row| {
                                row.col(|ui| { ui.label(source.id.to_string()); });
                                row.col(|ui| { ui.label(source.health.to_string()); });
                                row.col(|ui| { ui.label(source.nodes_count.to_string()); });
                                row.col(|ui| { ui.label(humantime::format_rfc3339_seconds(source.last_snapshot).to_string()); });

                                // IfStats column
                                row.col(|ui| {
                                    let response = ui.link("ℹ");
                                    let tooltip_closure = |ui: &mut Ui| {
                                        ui.set_width(420.0);
                                        ui.label("Interface Stats");
                                        ui.separator();

                                        let stats_table = TableBuilder::new(ui)
                                            .striped(true)
                                            .resizable(false)
                                            .column(Column::auto().at_least(120.0)) // IP address
                                            .column(Column::auto().at_least(70.0))  // RX bytes
                                            .column(Column::auto().at_least(70.0))  // TX bytes
                                            .column(Column::auto().at_least(70.0))  // RX packets
                                            .column(Column::auto().at_least(70.0)); // TX packets

                                        stats_table
                                            .header(18.0, |mut h| {
                                                h.col(|ui| { ui.strong("IP"); });
                                                h.col(|ui| { ui.strong("RX B"); });
                                                h.col(|ui| { ui.strong("TX B"); });
                                                h.col(|ui| { ui.strong("RX Pkts"); });
                                                h.col(|ui| { ui.strong("TX Pkts"); });
                                            })
                                            .body(|mut b| {
                                                for interface in source.interface_stats {
                                                    b.row(18.0, |mut r| {
                                                        r.col(|ui| { ui.label(interface.ip_address.to_string()); });
                                                        r.col(|ui| { ui.label(interface.rx_bytes.map(|v| humanize_bytes(v)).unwrap_or_else(|| "-".to_string())); });
                                                        r.col(|ui| { ui.label(interface.tx_bytes.map(|v| humanize_bytes(v)).unwrap_or_else(|| "-".to_string())); });
                                                        r.col(|ui| { ui.label(interface.rx_packets.map(|v| humanize_packet_count(v)).unwrap_or_else(|| "-".to_string())); });
                                                        r.col(|ui| { ui.label(interface.tx_packets.map(|v| humanize_packet_count(v)).unwrap_or_else(|| "-".to_string())); });
                                                    });
                                                }
                                            });
                                    };
                                    if response.hovered() {
                                        egui::Tooltip::for_widget(&response).show(tooltip_closure);
                                    }
                                });

                                row.col(|ui| {
                                    ui.horizontal(|ui| {
                                        if ui.small_button("🗑").on_hover_text("Remove a source and its partition from the store").clicked() {
                                            if let Err(err) = actions.remove_source(&source.id) {
                                                eprintln!("[dock::sources] Failed to remove source: {}", err);
                                            } else {
                                                let _ = actions.reload_graph();
                                            }
                                        }
                                        if ui.small_button("🗋").on_hover_text("Serialize the source state and print to stdout").clicked() {
                                            let state = actions.source_to_string(&source.id);
                                            println!("{}", state.unwrap_or("Couldn't serialize".to_string()))
                                        }
                                    });
                                });
                                row.col(|ui| {
                                    let original_enabled = actions.is_source_enabled(&source.id);
                                    let mut enabled = original_enabled;
                                    let response = ui.add(
                                        Checkbox::without_text(&mut enabled)
                                    )
                                    .on_hover_text("Temporarily enable/disable source from view");
                                    
                                    if response.changed() && enabled != original_enabled {
                                        actions.toggle_source(&source.id);
                                        
                                        if let Err(e) = actions.reload_graph() {
                                            eprintln!("[dock::sources] Failed to reload graph: {}", e);
                                        }
                                    }
                                });
                            });
                        }
                    })
            });
    }
}

#[allow(unused)]
fn humanize_value(value: u64) -> (f64, String) {
    const UNITS: [&str; 11] = ["", "k", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q"];
    let mut current_value = value as f64;
    let mut unit_index = 0;

    while current_value >= 1000f64 && unit_index < UNITS.len() - 1 {
        current_value /= 1000f64;
        unit_index += 1;
    }

    (current_value, UNITS[unit_index].to_string())
}

#[allow(unused)]
fn humanize_bytes(bytes: u64) -> String {
    let (value, prefix) = humanize_value(bytes);

    format!("{:.2} {}B", value, prefix)
}

#[allow(unused)]
fn humanize_packet_count(count: u64) -> String {
    let (value, prefix) = humanize_value(count);

    format!("{:.2} {}pkts", value, prefix)
}