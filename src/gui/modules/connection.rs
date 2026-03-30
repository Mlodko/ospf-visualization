use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{CollapsingHeader, Id};
use ssh2::DisconnectCode::ProtocolError;
use tokio::sync::Mutex;

use crate::gui::modules::timed_notice::{self, TimedNotice};
use crate::gui::new_app::{self, AppPanel, PollResult};
use crate::gui::actions::ConnectStatus;

const DEFAULT_NOTICE_DURATION: Duration = Duration::from_secs(5);

/// Handles the panel for connecting to SNMP and SSH sources.
pub struct ConnectionsPanel {
    // SNMP source switching state
    snmp_host: String,
    snmp_port: u16,
    snmp_community: String,
    snmp_clear_sources_on_switch: bool,
    snmp_connection_in_progress: bool,

    // SSH source switching state
    ssh_host: String,
    ssh_port: u16,
    ssh_username: String,
    ssh_password: String,
    ssh_clear_sources_on_switch: bool,
    ssh_connection_in_progress: bool,
}

impl Default for ConnectionsPanel {
    fn default() -> Self {
        Self {
            snmp_host: "127.0.0.1".to_string(),
            snmp_port: 1161,
            snmp_community: "public".to_string(),
            snmp_clear_sources_on_switch: false,
            snmp_connection_in_progress: false,

            ssh_host: "127.0.0.1".to_string(),
            ssh_port: 2221,
            ssh_username: "client".to_string(),
            ssh_password: "password".to_string(),
            ssh_clear_sources_on_switch: false,
            ssh_connection_in_progress: false,
        }
    }
}

impl ConnectionsPanel {
    pub fn new(
        snmp_host: String,
        snmp_port: u16,
        snmp_community: String,
        snmp_clear_sources_on_switch: bool,
        ssh_host: String,
        ssh_port: u16,
        ssh_username: String,
        ssh_password: String,
        ssh_clear_sources_on_switch: bool,
    ) -> Self {
        Self {
            snmp_host,
            snmp_port,
            snmp_community,
            snmp_clear_sources_on_switch,

            ssh_host,
            ssh_port,
            ssh_username,
            ssh_password,
            ssh_clear_sources_on_switch,
            ..Default::default()
        }
    }

    fn render_ssh(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        actions: &mut dyn crate::gui::actions::AppActions,
    ) {
        ui.horizontal(|ui| {
            ui.label("Host");
            ui.text_edit_singleline(&mut self.ssh_host);
        });
        ui.horizontal(|ui| {
            ui.label("Port");
            let mut port_val = self.ssh_port as i32;
            if ui
                .add(egui::DragValue::new(&mut port_val).range(1..=65535))
                .changed()
            {
                self.ssh_port = port_val as u16;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Username");
            ui.text_edit_singleline(&mut self.ssh_username);
        });
        ui.horizontal(|ui| {
            ui.label("Password");
            ui.text_edit_singleline(&mut self.ssh_password);
        });
        ui.checkbox(
            &mut self.ssh_clear_sources_on_switch,
            "Clear previous sources on connect",
        );
        ui.separator();
        
        match actions.ssh_connect_status().clone() {
            ConnectStatus::Idle => {
                if ui.button("Connect").clicked() {
                    actions.connect_ssh(
                        self.ssh_host.clone(),
                        self.ssh_port,
                        self.ssh_username.clone(),
                        self.ssh_password.clone(),
                        self.ssh_clear_sources_on_switch
                    )
                }
            }
            ConnectStatus::Pending => {
                ui.add_enabled_ui(false, |ui| ui.button("Connecting..."));
            }
            ConnectStatus::Success => {
                if ui.button("Connect").clicked() {
                    actions.connect_ssh(
                        self.ssh_host.clone(),
                        self.snmp_port,
                        self.ssh_username.clone(),
                        self.ssh_password.clone(),
                        self.ssh_clear_sources_on_switch
                    )
                }
                
                let id = Id::new("ssh_success_notice");
                let ttl = DEFAULT_NOTICE_DURATION;
                
                let should_clear = timed_notice::notice_in_temp_storage(ctx, ui, id, |_now| {
                    TimedNotice::new(ttl, "SSH connected".into())
                        .colored(new_app::get_theme().theme().green)
                        .with_create_easing(|t| simple_easing::quart_in_out(t), Duration::from_millis(200))
                        .with_hold_duration(ttl.saturating_sub(Duration::from_millis(1000)))
                        .with_fade_out_easing(|t| simple_easing::cubic_out(t))
                        .with_destroy_easing(|t| simple_easing::expo_out(t), Duration::from_millis(300))
                        .with_extend_easing(|t| simple_easing::expo_out(t), Duration::from_millis(800))
                        .dismiss_on_click(true)
                });
                
                if should_clear {
                    actions.clear_ssh_connect_status();
                }
            }
            ConnectStatus::Failure(err) => {
                if ui.button("Connect").clicked() {
                    actions.connect_ssh(
                        self.ssh_host.clone(),
                        self.snmp_port,
                        self.ssh_username.clone(),
                        self.ssh_password.clone(),
                        self.ssh_clear_sources_on_switch
                    )
                }
                
                let id = Id::new("ssh_failure_notice");
                let ttl = DEFAULT_NOTICE_DURATION;
                
                let should_clear = timed_notice::notice_in_temp_storage(ctx, ui, id, |_now| {
                    TimedNotice::new(ttl, format!("SSH connection failed: {}", err))
                        .colored(new_app::get_theme().theme().red)
                        .with_create_easing(|t| simple_easing::quart_in_out(t), Duration::from_millis(200))
                        .with_hold_duration(ttl.saturating_sub(Duration::from_millis(1000)))
                        .with_fade_out_easing(|t| simple_easing::cubic_out(t))
                        .with_destroy_easing(|t| simple_easing::expo_out(t), Duration::from_millis(300))
                        .with_extend_easing(|t| simple_easing::expo_out(t), Duration::from_millis(800))
                        .dismiss_on_click(true)
                });
                
                if should_clear {
                    actions.clear_ssh_connect_status();
                }
            }
        }
    }

    fn render_snmp(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        actions: &mut dyn crate::gui::actions::AppActions,
    ) {
        ui.horizontal(|ui| {
            ui.label("Host");
            ui.text_edit_singleline(&mut self.snmp_host);
        });
        ui.horizontal(|ui| {
            ui.label("Port");
            let mut port_val = self.snmp_port as i32;
            if ui
                .add(egui::DragValue::new(&mut port_val).range(1..=65535))
                .changed()
            {
                self.snmp_port = port_val as u16;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Community");
            ui.text_edit_singleline(&mut self.snmp_community);
        });
        ui.checkbox(
            &mut self.snmp_clear_sources_on_switch,
            "Clear previous sources on connect",
        );
        ui.separator();
        match actions.snmp_connect_status().clone() {
            ConnectStatus::Idle => {
                if ui.button("Connect").clicked() {
                    actions.connect_snmp(
                        self.snmp_host.clone(),
                        self.snmp_port,
                        self.snmp_community.clone(),
                        self.snmp_clear_sources_on_switch,
                    );
                }
            }
            ConnectStatus::Pending => {
                ui.add_enabled_ui(false, |ui| ui.button("Connecting..."));
            }
            ConnectStatus::Success => {
                if ui.button("Connect").clicked() {
                    actions.connect_snmp(
                        self.snmp_host.clone(),
                        self.snmp_port,
                        self.snmp_community.clone(),
                        self.snmp_clear_sources_on_switch,
                    );
                }

                let id = Id::new("snmp_success_notice");
                let ttl = DEFAULT_NOTICE_DURATION;

                let should_clear = timed_notice::notice_in_temp_storage(ctx, ui, id, |_now| {
                    TimedNotice::new(ttl, "SNMP connected".into())
                        .colored(new_app::get_theme().theme().green)
                        .with_create_easing(|t| simple_easing::quart_in_out(t), Duration::from_millis(200))
                        .with_hold_duration(ttl.saturating_sub(Duration::from_millis(1000)))
                        .with_fade_out_easing(|t| simple_easing::cubic_out(t))
                        .with_destroy_easing(|t| simple_easing::expo_out(t), Duration::from_millis(300))
                        .with_extend_easing(|t| simple_easing::expo_out(t), Duration::from_millis(800))
                        .dismiss_on_click(true)
                });
                
                if should_clear {
                    actions.clear_snmp_connect_status();
                }
            }
            ConnectStatus::Failure(err) => {
                if ui.button("Connect").clicked() {
                    actions.connect_snmp(
                        self.snmp_host.clone(),
                        self.snmp_port,
                        self.snmp_community.clone(),
                        self.snmp_clear_sources_on_switch,
                    );
                }

                let id = Id::new("snmp_failure_notice");
                let ttl = DEFAULT_NOTICE_DURATION;

                let should_clear = timed_notice::notice_in_temp_storage(ctx, ui, id, |_now| {
                    TimedNotice::new(ttl, format!("SNMP connection failed: {}", err))
                        .colored(new_app::get_theme().theme().red)
                        .with_create_easing(|t| simple_easing::quart_in_out(t), Duration::from_millis(200))
                        .with_hold_duration(ttl.saturating_sub(Duration::from_millis(1000)))
                        .with_fade_out_easing(|t| simple_easing::cubic_out(t))
                        .with_destroy_easing(|t| simple_easing::expo_out(t), Duration::from_millis(300))
                        .with_extend_easing(|t| simple_easing::expo_out(t), Duration::from_millis(800))
                        .dismiss_on_click(true)
                });
                
                if should_clear {
                    actions.clear_snmp_connect_status();
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
                CollapsingHeader::new("IS-IS via SSH")
                    .default_open(true)
                    .show(ui, |ui| {
                        self.render_ssh(ctx, ui, actions);
                    });
                CollapsingHeader::new("OSPF via SNMP")
                    .default_open(true)
                    .show(ui, |ui| {
                        self.render_snmp(ctx, ui, actions);
                    });
            });
    }
}
