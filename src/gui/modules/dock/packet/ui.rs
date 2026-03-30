use std::net::Ipv4Addr;

use anyhow::anyhow;
use egui::{CollapsingHeader, Response, WidgetText};

use crate::{gui::{actions::AppActions, modules::dock::packet::{parse::{lsa::{header::LsaHeader, lsa::Lsa, payload::{AsExternalLsa, AsExternalTosMetric, LsaPayload, NetworkLsa, RouterLink, RouterLsa, SummaryLsa, SummaryTosMetric}}, span::Span}, state::{Packet, PacketInspectorState}}}, network::node::{Node, NodeInfo}, parsers::isis_parser::core_lsp::{AreaAddressesTlv, ExtendedIpReachabilityTlv, IpReachabilityTlv, IsExtendedReachabilityTlv, IsReachabilityTlv, Lsp, RouterCapabilityTlv, Tlv}};

pub trait SemanticUi {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span));
}

fn label_with_span(ui: &mut egui::Ui, text: impl Into<WidgetText>, span: Span, on_span: &mut dyn FnMut(Span)) -> Response {
    let resp = ui.label(text);
    if resp.hovered() || resp.clicked() {
        on_span(span);
    }
    resp
}

impl SemanticUi for Packet {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        match self {
            Packet::Ospf(lsa) => lsa.ui_semantic(ui, on_span),
            Packet::IsIs(lsp) => lsp.ui_semantic(ui, on_span),
        }
    }
}

impl SemanticUi for Lsa {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        let _ = CollapsingHeader::new("Link State Advertisement")
            .show(ui, |ui| {
                self.header.ui_semantic(ui, on_span);
                self.payload.ui_semantic(ui, on_span);
            });
    }
}

impl SemanticUi for LsaPayload {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        match self {
            LsaPayload::Network(network) => network.ui_semantic(ui, on_span),
            LsaPayload::Router(router) => router.ui_semantic(ui, on_span),
            LsaPayload::Summary(summary) => summary.ui_semantic(ui, on_span),
            LsaPayload::AsExternal(as_external) => as_external.ui_semantic(ui, on_span),
        }
    }
}

impl SemanticUi for LsaHeader {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        let header = CollapsingHeader::new("LSA Header")
            .show(ui, |ui| {
                label_with_span(ui, format!("LS Age: {}", self.ls_age.value), self.ls_age.span, on_span)
                    .on_hover_text("The time in seconds since the LSA was originated.");
                let options = CollapsingHeader::new("Options")
                    .show(ui, |ui| {
                    label_with_span(ui, format!("Raw value: {:#10b}", self.options.value.0), self.options.span, on_span);
                    label_with_span(ui, format!("E-bit: {}", self.options.value.e_bit()), self.options.span, on_span);
                    label_with_span(ui, format!("MC-bit: {}", self.options.value.mc_bit()), self.options.span, on_span);
                    label_with_span(ui, format!("NP-bit: {}", self.options.value.np_bit()), self.options.span, on_span);
                    label_with_span(ui, format!("EA-bit: {}", self.options.value.ea_bit()), self.options.span, on_span);
                    label_with_span(ui, format!("DC-bit: {}", self.options.value.dc_bit()), self.options.span, on_span);
                }).header_response;
                if options.hovered() || options.clicked() {
                    on_span(self.options.span);
                }
                label_with_span(ui, format!("LS Type: {}", self.ls_type.value), self.ls_type.span, on_span);
                label_with_span(ui, format!("LS ID: {}", self.ls_id.value), self.ls_id.span, on_span);
                label_with_span(ui, format!("Advertising Router: {}", self.adv_router.value), self.adv_router.span, on_span);
                label_with_span(ui, format!("LS Sequence Number: {}", self.seq_num.value), self.seq_num.span, on_span);
                label_with_span(ui, format!("Checksum: {:#06x}", self.checksum.value), self.checksum.span, on_span);
                label_with_span(ui, format!("LS length: {}", self.length.value), self.length.span, on_span);
            })
            .header_response;
        if header.hovered() || header.clicked() {
            on_span(self.span);
        }
    }
}

impl SemanticUi for RouterLsa {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        let header = CollapsingHeader::new("Router LSA")
            .show(ui, |ui| {
                let flags = CollapsingHeader::new("Flags")
                    .show(ui, |ui| {
                        label_with_span(ui, format!("Raw value: {:#018b}", self.flags.value.0), self.flags.span, on_span);
                        label_with_span(ui, format!("V-Bit: {}", self.flags.value.v_bit()), self.flags.span, on_span);
                        label_with_span(ui, format!("E-Bit: {}", self.flags.value.e_bit()), self.flags.span, on_span);
                        label_with_span(ui, format!("B-bit: {}", self.flags.value.b_bit()), self.flags.span, on_span);
                    }).header_response;
                if flags.hovered() || flags.clicked() {
                    on_span(self.flags.span);
                }
                
                label_with_span(ui, format!("Link count: {}", self.link_count.value), self.link_count.span, on_span);
                let _ = CollapsingHeader::new("Links")
                    .show(ui, |ui| {
                        self.links.iter().for_each(|link| link.ui_semantic(ui, on_span));
                    });
            }).header_response;
        if header.hovered() || header.clicked() {
            on_span(self.span);
        }
    }
}

impl SemanticUi for RouterLink {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        let title = format!("Type 1 - Router Link ({})", self.id.value);
        let header = CollapsingHeader::new(title)
            .show(ui, |ui| { 
                let id = CollapsingHeader::new(format!("Link ID ({})", self.id.value))
                    .show(ui, |ui| {
                        label_with_span(ui, format!("Raw value: {:#034b}", self.id.value.0), self.id.span, on_span);
                        label_with_span(ui, format!("{}", self.link_id()), self.id.span, on_span);
                    }).header_response;
                if id.hovered() || id.clicked() {
                    on_span(self.id.span);
                }
                
                label_with_span(ui, format!("Link Data: {}", Ipv4Addr::from_bits(self.data.value.0)), self.data.span, on_span);
                label_with_span(ui, format!("Link Type: {}", self.link_type.value), self.link_type.span, on_span);
                label_with_span(ui, format!("TOS count: {}", self.tos_count.value), self.tos_count.span, on_span);
                label_with_span(ui, format!("Metric: {}", self.metric.value), self.metric.span, on_span);
                
                let _ = CollapsingHeader::new("TOS Metrics")
                .show(ui, |ui| {
                    self.tos_metrics.iter().for_each(|tos| {
                        label_with_span(ui, format!("TOS: {}", tos.tos.value), tos.tos.span, on_span);
                        label_with_span(ui, format!("Metric: {}", tos.metric.value), tos.metric.span, on_span);
                    });
                });
            }).header_response;
        
        if header.hovered() || header.clicked() {
            on_span(self.span);
        }
    }
}

impl SemanticUi for NetworkLsa {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        let header = CollapsingHeader::new("Type 2 - Network LSA")
            .show(ui, |ui| {
                label_with_span(ui, format!("Network Mask: {}", Ipv4Addr::from_bits(self.mask.value.0)), self.mask.span, on_span);
                let _ = CollapsingHeader::new("Attached Routers")
                    .show(ui, |ui| {
                        self.attached_routers.iter().for_each(|router| {
                            label_with_span(ui, format!("Router ID: {}", router.value), router.span, on_span);
                        });
                    });
            }).header_response;
        
        if header.hovered() || header.clicked() {
            on_span(self.span);
        }
    }
}

impl SemanticUi for SummaryLsa {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        let header = CollapsingHeader::new("Type 3 - Summary LSA")
            .show(ui, |ui| {
                label_with_span(ui, format!("Network Mask: {}", Ipv4Addr::from_bits(self.mask.value.0)), self.mask.span, on_span);
                label_with_span(ui, format!("Metric: {}", self.metric_24bit()), self.metric.span, on_span);
                let _ = CollapsingHeader::new("TOS Metrics")
                    .show(ui, |ui| {
                        self.tos_metrics.iter().for_each(|metric| {
                            metric.ui_semantic(ui, on_span);
                        });
                    });
            }).header_response;
        
        if header.hovered() || header.clicked() {
            on_span(self.span);
        }
    }
}

impl SemanticUi for SummaryTosMetric {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        let header = CollapsingHeader::new(format!("TOS {}", self.tos.value))
            .show(ui, |ui| {
                label_with_span(ui, format!("TOS: {}", self.tos.value), self.tos.span, on_span);
                label_with_span(ui, format!("Metric: {}", self.metric_24bit()), self.metric.span, on_span);
            }).header_response;
        
        if header.hovered() || header.clicked() {
            on_span(self.span);
        }
    }
}

impl SemanticUi for AsExternalLsa {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        let header = CollapsingHeader::new("Type 5 - AS External LSA")
            .show(ui, |ui| {
                label_with_span(ui, format!("Network Mask: {}", self.mask.value), self.mask.span, on_span);
                label_with_span(ui, format!("E-Bit: {}", self.e_bit.value), self.e_bit.span, on_span);
                label_with_span(ui, format!("Metric: {}", self.metric_24bit()), self.metric.span, on_span);
                label_with_span(ui, format!("Forwarding Address: {}", self.forwarding_address.value), self.forwarding_address.span, on_span);
                label_with_span(ui, format!("External Route Tag: {} ({:#010x})", self.external_route_tag.value.0, self.external_route_tag.value.0), self.external_route_tag.span, on_span);
                let _ = CollapsingHeader::new("TOS Metrics")
                    .show(ui, |ui| {
                        self.tos_metrics.iter().for_each(|tos| {
                            tos.ui_semantic(ui, on_span);
                        });
                    });
            }).header_response;
        
        if header.hovered() || header.clicked() {
            on_span(self.span);
        }
    }
}

impl SemanticUi for AsExternalTosMetric {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        let header = CollapsingHeader::new(format!("TOS {}", self.tos.value))
            .show(ui, |ui| {
                label_with_span(ui, format!("E-Bit: {}", self.e_bit.value), self.e_bit.span, on_span);
                label_with_span(ui, format!("TOS: {}", self.tos.value), self.tos.span, on_span);
                label_with_span(ui, format!("Metric: {}", self.metric_24bit()), self.metric.span, on_span);
                label_with_span(ui, format!("Forwarding Address: {}", self.forwarding_address.value), self.forwarding_address.span, on_span);
                label_with_span(ui, format!("External Route Tag: {} ({:#010x})", self.external_route_tag.value.0, self.external_route_tag.value.0), self.external_route_tag.span, on_span);
            }).header_response;
        
        if header.hovered() || header.clicked() {
            on_span(self.span);
        }
    }
}

impl SemanticUi for Lsp {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        CollapsingHeader::new("Link State PDU")
            .show(ui, |ui| {
                ui.label(format!("LSP ID: {}", &self.lsp_id));
                ui.label(format!("System ID: {}", &self.system_id));
                ui.label(format!("IS Level: {}", &self.is_level));
                let seq_num = if let Some(seq_num) = &self.sequence_number {
                    seq_num.as_str()
                } else {
                    "N/A"
                };
                ui.label(format!("Sequence Number: {}", seq_num));
                let holdtime = if let Some(holdtime) = &self.holdtime {
                    holdtime.as_str()
                } else {
                    "N/A"
                };
                ui.label(format!("Holdtime: {}", holdtime));
                let area_addr = if let Some(area_addr) = &self.area_addr {
                    &area_addr.to_string()
                } else {
                    "N/A"
                };
                ui.label(format!("Area Address: {}", area_addr));
                let owned = if let Some(owned) = &self.owned {
                    &owned.to_string()
                } else {
                    "N/A"
                };
                ui.label(format!("Owned: {}", owned));
                CollapsingHeader::new("TLVs")
                    .show(ui, |ui| {
                        for tlv in &self.tlvs {
                            tlv.ui_semantic(ui, on_span);
                        }
                    });
            });
    }
}

impl SemanticUi for Tlv {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        CollapsingHeader::new(self.get_name())
            .show(ui, |ui| {
                match self {
                    Tlv::Hostname(hostname) => {
                        ui.label(format!("Hostname: {}", hostname));
                    }
                    Tlv::AreaAddresses(tlv) => {
                        tlv.ui_semantic(ui, on_span);
                    }
                    Tlv::IsReachability(tlv) => {
                        tlv.ui_semantic(ui, on_span);
                    }
                    Tlv::ExtendedReachability(tlv) => {
                        tlv.ui_semantic(ui, on_span);
                    }
                    Tlv::IpReachability(tlv) => {
                        tlv.ui_semantic(ui, on_span);
                    }
                    Tlv::ExtendedIpReachability(tlv) => {
                        tlv.ui_semantic(ui, on_span);
                    }
                    Tlv::RouterCapability(tlv) => {
                        tlv.ui_semantic(ui, on_span);
                    }
                }       
            });
    }
}

impl SemanticUi for AreaAddressesTlv {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        self.addresses.iter().for_each(|address| {
            ui.label(format!(" - {}", address));
        });
    }
}

impl SemanticUi for IsReachabilityTlv {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        self.neighbors_iter().for_each(|n| {
            ui.label(format!(" - {} (metric: {})", n.system_id, n.metric));
        })
    }
}

impl SemanticUi for IsExtendedReachabilityTlv {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        self.neighbors.iter().for_each(|n| {
            ui.label(format!(" - {}.{:#02} (metric: {})", n.neighbor_id, n.pseudonode_id, n.metric));
        });
    }
}

impl SemanticUi for IpReachabilityTlv {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        self.prefixes_iter().for_each(|p| {
            let up_label = if p.up { "[UP]" } else { "[  ]" };
            ui.label(format!(" - {} {} (metric: {})", up_label, p.prefix, p.metric));
        });
    }
}

impl SemanticUi for ExtendedIpReachabilityTlv {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        self.neighbors.iter().for_each(|p| {
            let up_label = if p.up_down { "[UP]" } else { "[  ]" };
            ui.label(format!(" - {} {} (metric: {})", up_label, p.prefix, p.metric));
        });
    }
}

impl SemanticUi for RouterCapabilityTlv {
    fn ui_semantic(&self, ui: &mut egui::Ui, on_span: &mut dyn FnMut(Span)) {
        let te_r_id = if let Some(id) = &self.te_router_id {
            &id.to_string()
        } else {
            "N/A"
        };
        ui.label(format!("TE Router ID: {}", te_r_id));
        CollapsingHeader::new("Flags")
            .show(ui, |ui| {
                self.flags.iter().for_each(|(name, value)| {
                    ui.label(format!(" - {}: {}", name, value));
                });
            });
    }
}

pub fn combo_box_lsa_selector(ui: &mut egui::Ui, selected_node: &Node, state: &mut PacketInspectorState) -> anyhow::Result<()> {
    let ospf_data = match &selected_node.info {
        NodeInfo::Router(r) => r.protocol_data().map(|d| d.ospf()).flatten(),
        NodeInfo::Network(n) => n.protocol_data().map(|d| d.ospf()).flatten(),
    };
    
    let all_advertisements: Vec<Lsa> = if let Some(data) = ospf_data {
        todo!()
    } else {
        return Err(anyhow!("No OSPF data found"));
    };
    
    Ok(())
}