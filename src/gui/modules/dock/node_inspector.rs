use egui::{CollapsingHeader, Ui};
use egui_extras::{Column, TableBuilder};

use crate::{gui::{actions::{AppActions, SourceSummary}, modules::dock::{DockActions, DockPanel, DockPanelKind}}, network::{node::{IsIsData, Network, Node, NodeInfo, OspfData, OspfExternalNetPayload, OspfNetworkPayload, OspfPayload, OspfRouterPayload, OspfSummaryNetPayload, PerAreaRouterFacet, ProtocolData}, router::Router}, parsers::isis_parser::core_lsp::{AreaAddressesTlv, ExtendedIpReachabilityTlv, IpReachabilityTlv, IsExtendedReachabilityTlv, IsReachabilityTlv, RouterCapabilityTlv, Tlv}};

#[derive(Default)]
pub struct NodeInspectorDockPanel;

impl DockPanel for NodeInspectorDockPanel {
    fn title(&self) -> egui::WidgetText {
        "Node Inspector".into()
    }

    fn kind(&self) -> DockPanelKind {
        DockPanelKind::NodeInspector
    }

    fn ui(&mut self, ui: &mut egui::Ui, actions: &mut dyn crate::gui::actions::AppActions, dock_actions: &mut dyn DockActions) {
        let node = actions.selected_node();
        
        if !node.is_some() {
            ui.centered_and_justified(|ui| {
                ui.label("No node selected")
            });
            return;
        }
        
        if ui.button("Print Node Data").clicked() {
            println!("{:#?}", &node.unwrap());
        }
        
        render_node(&node.unwrap().clone(), ui, actions, dock_actions);
    }
}

fn render_node(node: &Node, ui: &mut Ui, actions: &mut dyn AppActions, dock_actions: &mut dyn DockActions) {
    CollapsingHeader::new("Node")
        .default_open(true)
        .show(ui, |ui| {
            ui.label(format!("UUID: {}", &node.id));
            ui.label(format!("Label: {}", node.label.as_ref().unwrap_or(&"None".to_string())));
            ui.label(format!("Source router ID: {}", 
                match &node.source_id {
                    Some(id) => id.as_string(),
                    None => "None".to_string()
                }
            ));
            match &node.info {
                NodeInfo::Router(router) => render_router(router, ui, actions, dock_actions),
                NodeInfo::Network(network) => render_network(network, ui, actions, dock_actions),
            }
        });
}

fn render_router(router: &Router, ui: &mut Ui, actions: &mut dyn AppActions, dock_actions: &mut dyn DockActions) {
    CollapsingHeader::new(format!("Router {}", &router.id))
        .show(ui, |ui| {
            ui.label(format!("Router ID: {}", &router.id));
            CollapsingHeader::new("Interfaces")
                .default_open(false)
                .show(ui, |ui| {
                    for int in router.interfaces.iter() {
                        ui.label(format!(" - {}", int));
                    }
                });
            CollapsingHeader::new("Protocol Data")
                .default_open(false)
                .show(ui, |ui| {
                    if let Some(data) = &router.protocol_data {
                        render_protocol_data(data, ui, dock_actions);
                    } else {
                        ui.label("No protocol data available");
                    }
                });
            // If this router is a source this action will return Some() and the section will be rendered
            if let Some(source) = actions.source_summary(&router.id) {
                render_source_summary(source, ui);
            }
        });
}

fn render_network(network: &Network, ui: &mut Ui, _actions: &mut dyn AppActions, dock_actions: &mut dyn DockActions) {
    CollapsingHeader::new(format!("Network {}", network.ip_address))
        .show(ui, |ui| {
            ui.label(format!("IP Address: {}", network.ip_address));
            CollapsingHeader::new("Protocol Data")
                .show(ui, |ui| {
                    if let Some(data) = &network.protocol_data {
                        render_protocol_data(data, ui, dock_actions);
                    } else {
                        ui.label("No protocol data available");
                    }
                });
            CollapsingHeader::new("Attached Routers")
                .show(ui, |ui| {
                    network.attached_routers.iter().for_each(|id| {
                        ui.label(format!(" - {}", id));
                    });
                });
        });
}

fn render_protocol_data(data: &ProtocolData, ui: &mut Ui, dock_actions: &mut dyn DockActions) {
    match data {
        ProtocolData::Ospf(data) => render_ospf_data(data, ui, dock_actions),
        ProtocolData::IsIs(data) => render_isis_data(data, ui),
        ProtocolData::Other(data) => { ui.label(format!("Other: {}", data)); },
    }
}

fn render_ospf_data(data: &OspfData, ui: &mut Ui, dock_actions: &mut dyn DockActions) {
    ui.label("Protocol: OSPF");
    ui.label(format!("Area ID: {}", data.area_id));
    let adv_link = ui.link("Link State Advertisement")
        .on_hover_text("View LSA in packet inspector");
    if adv_link.clicked() {
        dock_actions.focus_or_open(DockPanelKind::PacketInspectorBytes);
        dock_actions.focus_or_open(DockPanelKind::PacketInspectorBytes);
    }
    ui.label(format!("Link State ID: {}", data.link_state_id));
    ui.label(format!("Advertising Router: {}", data.advertising_router));
    render_ospf_payload(&data.payload, ui, dock_actions);
    
}

fn render_ospf_payload(payload: &OspfPayload, ui: &mut Ui, dock_actions: &mut dyn DockActions) {
    match payload {
        OspfPayload::Router(payload) => render_ospf_router_payload(payload, ui, dock_actions),
        OspfPayload::Network(payload) => render_ospf_network_payload(payload, ui, dock_actions),
        OspfPayload::SummaryNetwork(payload) => render_ospf_summary_payload(payload, ui, dock_actions),
    }
}

fn render_ospf_router_payload(payload: &OspfRouterPayload, ui: &mut Ui, _dock_actions: &mut dyn DockActions) {
    CollapsingHeader::new("Flags")
        .show(ui, |ui| {
            ui.label(format!("ABR: {}", payload.is_abr));
            ui.label(format!("ASBR: {}", payload.is_asbr));
            ui.label(format!("Virtual Link Endpoint: {}", payload.is_virtual_link_endpoint));
            ui.label(format!("NSSA Capable: {}", payload.is_nssa_capable));
        });
    CollapsingHeader::new("Link Counts")
        .show(ui, |ui| {
            let total_links = payload.p2p_link_count + payload.transit_link_count + payload.stub_link_count;
            ui.label(format!("Total Links: {}", total_links));
            ui.label(format!("P2P Links: {}", payload.p2p_link_count));
            ui.label(format!("Transit Links: {}", payload.transit_link_count));
            ui.label(format!("Stub Links: {}", payload.stub_link_count));
        });
    CollapsingHeader::new("Link Metrics")
        .show(ui, |ui| {
            for (ip, metric) in payload.link_metrics.iter() {
                ui.label(format!(" - {}: {}", ip, metric));
            }
        });
    CollapsingHeader::new("Per Area Router Facets")
        .show(ui, |ui| {
            per_area_facets_table(ui, &payload.per_area_facets);
        });
    CollapsingHeader::new("Virtual Links")
        .show(ui, |ui| {
            for link in payload.virtual_links.iter() {
                ui.label(format!(" - {}", &link.peer_router_id));
            }
        });
    
}

fn per_area_facets_table(ui: &mut Ui, facets: &[PerAreaRouterFacet]) {
    let table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .column(Column::auto().at_least(140.0)) // Area ID
        .column(Column::auto().at_least(80.0))  // P2P
        .column(Column::auto().at_least(80.0))  // Transit
        .column(Column::auto().at_least(80.0)); // Stub

    table
        .header(20.0, |mut header| {
            header.col(|ui| { ui.strong("Area ID"); });
            header.col(|ui| { ui.strong("P2P"); });
            header.col(|ui| { ui.strong("Transit"); });
            header.col(|ui| { ui.strong("Stub"); });
        })
        .body(|mut body| {
            for facet in facets {
                body.row(18.0, |mut row| {
                    row.col(|ui| { ui.label(facet.area_id.to_string()); });
                    row.col(|ui| { ui.label(facet.p2p_link_count.to_string()); });
                    row.col(|ui| { ui.label(facet.transit_link_count.to_string()); });
                    row.col(|ui| { ui.label(facet.stub_link_count.to_string()); });
                });
            }
        });
}

fn render_ospf_network_payload(payload: &OspfNetworkPayload, ui: &mut Ui, dock_actions: &mut dyn DockActions) {
    if let Some(dr_id) = &payload.designated_router_id {
        ui.label(format!("Designated Router ID: {}", dr_id));
    }
    CollapsingHeader::new("Summaries")
        .show(ui, |ui| {
            for (i, summary) in payload.summaries.iter().enumerate() {
                CollapsingHeader::new(format!("Summary #{}", i))
                    .show(ui, |ui| {
                        render_ospf_summary_payload(summary, ui, dock_actions);
                    });
            }
        });
    CollapsingHeader::new("External Routes")
        .show(ui, |ui| {
            for (i, external) in payload.externals.iter().enumerate() {
                CollapsingHeader::new(format!("External Route #{}", i))
                    .show(ui, |ui| {
                        render_ospf_external_payload(external, ui);
                    });
            }
        });
}

fn render_ospf_summary_payload(payload: &OspfSummaryNetPayload, ui: &mut Ui, _dock_actions: &mut dyn DockActions) {
    ui.label(format!("Metric: {}", payload.metric));
    ui.label(format!("Origin ABR: {}", payload.origin_abr));
}

fn render_ospf_external_payload(payload: &OspfExternalNetPayload, ui: &mut Ui) {
    ui.label(format!("Metric: {}", payload.metric));
    ui.label(format!("Origin ASBR: {}", payload.origin_asbr));
    if let Some(route_tag) = payload.route_tag {
        ui.label(format!("Route Tag: {:#010x}", route_tag));
    }
    if let Some(addr) = payload.forwarding_address {
        ui.label(format!("Forwarding Address: {}", addr));
    }
}

fn render_isis_data(data: &IsIsData, ui: &mut Ui) {
    ui.label(format!("Level: {}", data.is_level));
    ui.label(format!("LSP ID: {}", data.lsp_id));
    if let Some(net) = &data.net_address {
        ui.label(format!("NET address: {}", net));
    }
    CollapsingHeader::new("TLVs")
        .show(ui, |ui| {
            data.tlvs.iter().for_each(|tlv| {
                render_tlv(tlv, ui);
            });
        });
}

fn render_tlv(tlv: &Tlv, ui: &mut Ui) {
    CollapsingHeader::new(tlv.get_name())
        .show(ui, |ui| {
            match tlv {
                Tlv::AreaAddresses(tlv) => render_tlv_1(tlv, ui),
                Tlv::IsReachability(tlv) => render_tlv_2(tlv, ui),
                Tlv::ExtendedReachability(tlv) => render_tlv_22(tlv, ui),
                Tlv::IpReachability(tlv) => render_tlv_128(tlv, ui),
                Tlv::ExtendedIpReachability(tlv) => render_tlv_135(tlv, ui),
                Tlv::Hostname(hostname) => render_tlv_137(hostname, ui),
                Tlv::RouterCapability(tlv) => render_tlv_242(tlv, ui),
            }
        });
}

fn render_tlv_1(tlv: &AreaAddressesTlv, ui: &mut Ui) {
    tlv.addresses.iter().for_each(|addr| {
        ui.label(format!(" - {}", addr));
    });
}

fn render_tlv_2(tlv: &IsReachabilityTlv, ui: &mut Ui) {
    tlv.neighbors_iter().for_each(|neighbor| {
        ui.label(format!(" - {} (metric: {})", neighbor.system_id, neighbor.metric));
    })
}

fn render_tlv_22(tlv: &IsExtendedReachabilityTlv, ui: &mut Ui) {
    tlv.neighbors.iter().for_each(|neighbor| {
        ui.label(format!(" - {}.{:02} (metric: {})", neighbor.neighbor_id, neighbor.pseudonode_id, neighbor.metric));
    });
}

fn render_tlv_128(tlv: &IpReachabilityTlv, ui: &mut Ui) {
    tlv.prefixes_iter().for_each(|prefix| {
        let up_label = if prefix.up { "[UP]" } else {" [  ] "};
        ui.label(format!(" - {} {} (metric: {})", up_label, prefix.prefix, prefix.metric));
    });
}

fn render_tlv_135(tlv: &ExtendedIpReachabilityTlv, ui: &mut Ui) {
    tlv.neighbors.iter().for_each(|neighbor| {
        let up_label = if neighbor.up_down { "[UP]" } else {" [  ] "};
        ui.label(format!(" - {} {} (metric: {})", up_label, neighbor.prefix, neighbor.metric));
    });
}

fn render_tlv_137(hostname: &String, ui: &mut Ui) {
    ui.label(format!("Hostname: {}", hostname));
}

fn render_tlv_242(tlv: &RouterCapabilityTlv, ui: &mut Ui) {
    let id_label = if let Some(id) = &tlv.te_router_id {
        id.to_string()
    } else {
        "N/A".to_string()
    };
    ui.label(format!("TE Router ID: {}", id_label));
    CollapsingHeader::new("Flags")
        .show(ui, |ui| {
            tlv.flags.iter().for_each(|(name, flag)| {
                ui.label(format!("{}: {}", name, flag));
            });
        });
}

fn render_source_summary(source: SourceSummary, ui: &mut Ui) {
    CollapsingHeader::new("Source Summary")
        .show(ui, |ui| {
            ui.label(format!("Source ID: {}", source.id));
            ui.label(format!("Source Health: {}", source.health.to_string()));
            ui.label(format!("Node Count: {}", source.nodes_count));
            let last_snapshot = format!(
                "{} ({} ago)",
                humantime::format_rfc3339(source.last_snapshot),
                humantime::format_duration(source.last_snapshot.elapsed().unwrap())
            );
            ui.label(format!("Last Snapshot: {}", last_snapshot));
            CollapsingHeader::new("Interface Statistics")
                .show(ui, |ui| {
                    source.interface_stats.iter().for_each(|stat| {
                        CollapsingHeader::new(stat.ip_address.to_string())
                            .show(ui, |ui| {
                                let rx_bytes = stat.rx_bytes.map(|b| b.to_string()).unwrap_or("-".to_string());
                                let tx_bytes = stat.tx_bytes.map(|b| b.to_string()).unwrap_or("-".to_string());
                                let rx_packets = stat.rx_packets.map(|p| p.to_string()).unwrap_or("-".to_string());
                                let tx_packets = stat.tx_packets.map(|p| p.to_string()).unwrap_or("-".to_string());
                                ui.label(format!(
                                    "Bytes Rx/Tx: {}/{}", 
                                    rx_bytes, tx_bytes
                                ));
                                ui.label(format!(
                                    "Packets Rx/Tx: {}/{}", 
                                    rx_packets, tx_packets
                                ));
                            });
                    });
                });
        })
        .header_response
        .on_hover_text("This router is a source of information about the network's topology, expand to see more details");
}

