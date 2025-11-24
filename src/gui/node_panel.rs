use egui::{
    self, CollapsingHeader, Context, Frame, Id, InnerResponse, Label, Order, Pos2, Response, Ui, Vec2
};

use crate::network::node::{OspfData, OspfPayload, ProtocolData};

/// A reusable floating panel anchored near a node on the canvas.
/// Designed to replace simple text labels with a fully interactive panel.
///
/// Typical usage:
/// - Create from an anchor point (e.g. your node's screen position).
/// - Optionally set a title and custom options (offset, width).
/// - Call `show` with a closure that builds the content.
/// - Use helpers like `collapsible_section` to keep content modular.
///
/// The panel persists a "pinned" flag per `Id` in egui's memory so the user can
/// drag it around and keep it open independently from hover/selection.
///
/// Example (conceptual):
/// let anchor = lbl.center;
/// let id = egui::Id::new(("node_panel", node_index.index()));
/// let res = FloatingNodePanel::new(id, anchor)
///     .title("Network")
///     .show(ctx, |ui, ctx| {
///         ui.label("IP address: 192.168.0.0/24");
///         ui.add_space(8.0);
///         collapsible_section(ui, "Connected routers", true, |ui| {
///             bullet_list(ui, ["192.168.0.1", "192.168.0.2", "192.168.0.3"]);
///         });
///         collapsible_section(ui, "Protocol Data", true, |ui| {
///             collapsible_section(ui, "OSPF", true, |ui| {
///                 bullet_list(ui, ["Designated router ID: 10.0.0.1"]);
///             });
///         });
///     });
///
/// if res.close_clicked {
///     // Caller can hide the panel or clear its pin in their state.
/// }
#[derive(Debug, Clone)]
pub struct FloatingNodePanel {
    id: Id,
    anchor: Pos2,
    title: Option<String>,
    options: NodePanelOptions,
}

/// Rendering and behavior options for the floating panel.
#[derive(Debug, Clone)]
pub struct NodePanelOptions {
    /// Offset applied to the anchor position.
    /// Positive y moves downward. Defaults to slightly above the anchor.
    pub offset: Vec2,
    /// Minimum width of the panel.
    pub min_width: f32,
    /// Egui order for the floating area.
    pub order: Order,
    /// Default pinned state if none persisted yet.
    pub pinned_default: bool,
}

impl Default for NodePanelOptions {
    fn default() -> Self {
        Self {
            // By default, position above the anchor with a small left offset
            offset: Vec2 { x: 12.0, y: -80.0 },
            min_width: 280.0,
            order: Order::Foreground,
            pinned_default: false,
        }
    }
}

/// Response data from the floating panel show call.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct NodePanelResponse {
    /// The panel's response rect.
    pub rect: egui::Rect,
    /// True if the user toggled or left the panel in pinned state.
    pub pinned: bool,
    /// True if the user clicked the close button in the panel header.
    pub close_clicked: bool,
    /// True if the node label was changed in this frame.
    pub label_changed: bool,
    /// The egui response for the entire area; useful for hover detection.
    pub area_response: Response,
}

#[allow(dead_code)]
impl FloatingNodePanel {
    /// Create a new floating node panel anchored at a screen-space position.
    pub fn new(id: Id, anchor: Pos2) -> Self {
        Self {
            id,
            anchor,
            title: None,
            options: NodePanelOptions::default(),
        }
    }

    /// Set a title displayed in the panel header.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Override default options.
    pub fn options(mut self, options: NodePanelOptions) -> Self {
        self.options = options;
        self
    }

    /// Builder convenience: set the minimum width of the panel.
    pub fn min_width(mut self, width: f32) -> Self {
        self.options.min_width = width;
        self
    }

    /// Builder convenience: set the offset relative to the anchor.
    pub fn offset(mut self, offset: Vec2) -> Self {
        self.options.offset = offset;
        self
    }

    /// Show the floating panel and build its contents with the provided closure.
    ///
    /// The closure receives the `Ui` for building content and the `Context` in case
    /// you want to access persisted data or global style.
    pub fn show<R>(
        &self,
        ctx: &Context,
        add_contents: impl FnOnce(&mut Ui, &Context) -> R,
    ) -> NodePanelResponse {
        let pos = Pos2 {
            x: self.anchor.x + self.options.offset.x,
            y: self.anchor.y + self.options.offset.y,
        };

        let mut pinned_state = persisted_pin(ctx, self.id).unwrap_or(self.options.pinned_default);
        let mut close_clicked = false;

        // Render an Area so the panel can float above the graph and be draggable when pinned.
        let area: InnerResponse<()> = egui::Area::new(self.id)
            .order(self.options.order)
            .movable(pinned_state)
            .interactable(true)
            .constrain(true)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                // Use a pop-up frame style for a floating feel.
                let frame = Frame::popup(ui.style());

                frame.show(ui, |ui| {
                    // Apply stored width if any; otherwise fallback to configured min_width.
                    ui.set_min_width(self.options.min_width);

                    // Header with title, pin, and close controls.
                    ui.horizontal(|ui| {
                        // Editable label replaces static title; persisted per panel id.
                        let label_id = self.id.with("label_text");
                        let mut label_text = ctx.data_mut(|d| {
                            d.get_persisted::<String>(label_id).unwrap_or_else(|| {
                                self.title.clone().unwrap_or_else(|| "Node".to_string())
                            })
                        });
                        let resp = ui
                            .add(egui::TextEdit::singleline(&mut label_text).desired_width(160.0));
                        if resp.changed() {
                            ctx.data_mut(|d| d.insert_persisted(label_id, label_text.clone()));
                        }

                        // Right-aligned controls
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Close button
                            if ui
                                .add(egui::Button::new("✕").small())
                                .on_hover_text("Close")
                                .clicked()
                            {
                                close_clicked = true;
                            }

                            // Pin/unpin button
                            let pin_label = if pinned_state { "📌" } else { "📍" };
                            if ui
                                .add(egui::Button::new(pin_label).small())
                                .on_hover_text(if pinned_state {
                                    "Unpin (panel will follow selection/hover)"
                                } else {
                                    "Pin (panel becomes draggable and remains visible)"
                                })
                                .clicked()
                            {
                                pinned_state = !pinned_state;
                            }
                        });
                    });

                    ui.add_space(6.0);

                    // Panel body supplied by the caller
                    add_contents(ui, ctx);
                });
            });

        // Persist pin state per-id so it's remembered across frames.
        set_persisted_pin(ctx, self.id, pinned_state);

        NodePanelResponse {
            rect: area.response.rect,
            pinned: pinned_state,
            close_clicked,
            label_changed: false,
            area_response: area.response,
        }
    }

    /// Show variant that edits a provided node label directly in the header.
    /// Returns NodePanelResponse with label_changed indicating whether the label mutated.
    pub fn show_with_label<R>(
        &self,
        ctx: &Context,
        node_label: &mut String,
        add_contents: impl FnOnce(&mut Ui, &Context) -> R,
    ) -> NodePanelResponse {
        let pos = Pos2 {
            x: self.anchor.x + self.options.offset.x,
            y: self.anchor.y + self.options.offset.y,
        };

        let pinned_state = persisted_pin(ctx, self.id).unwrap_or(self.options.pinned_default);
        let close_clicked = false;
        let mut label_changed_flag = false;

        let area: InnerResponse<()> = egui::Area::new(self.id)
            .order(self.options.order)
            .movable(pinned_state)
            .interactable(true)
            .constrain(true)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                let frame = Frame::popup(ui.style());
                frame.show(ui, |ui| {
                    // Dynamic width application (moved from show):
                    let stored_width =
                        ctx.data_mut(|d| d.get_persisted::<f32>(self.id.with("panel_width")));
                    // Allow natural layout first; do not constrain min width here.
                    // Width will be persisted after content renders for use in next frame if desired.

                    ui.horizontal(|ui| {
                        // Direct edit of node label
                        let resp =
                            ui.add(egui::TextEdit::singleline(node_label).desired_width(160.0));
                        if resp.changed() {
                            label_changed_flag = true;
                        }

                        /*
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(egui::Button::new("✕").small())
                                .on_hover_text("Close")
                                .clicked()
                            {
                                close_clicked = true;
                            }
                            let pin_label = if pinned_state { "📌" } else { "📍" };
                            if ui
                                .add(egui::Button::new(pin_label).small())
                                .on_hover_text(if pinned_state {
                                    "Unpin (panel will follow selection/hover)"
                                } else {
                                    "Pin (panel becomes draggable and remains visible)"
                                })
                                .clicked()
                            {
                                pinned_state = !pinned_state;
                            }
                        });
                        */
                    });
                    ui.add_space(6.0);
                    add_contents(ui, ctx);

                    // Dynamic width persistence after content layout
                    let content_width = ui.min_rect().width();
                    let expansion_threshold = 4.0;
                    let should_store = match stored_width {
                        None => true,
                        Some(old) => content_width > old + expansion_threshold,
                    };
                    if should_store {
                        ctx.data_mut(|d| {
                            d.insert_persisted(self.id.with("panel_width"), content_width)
                        });
                    }
                });
            });

        set_persisted_pin(ctx, self.id, pinned_state);

        NodePanelResponse {
            rect: area.response.rect,
            pinned: pinned_state,
            close_clicked,
            label_changed: label_changed_flag,
            area_response: area.response,
        }
    }
}

pub fn protocol_data_section(ui: &mut Ui, protocol_data: &Option<ProtocolData>) {
    if let Some(protocol_data) = protocol_data {
        collapsible_section(ui, "Routing Protocol Data", false, |ui| {
            let protocol_data = protocol_data;
            match protocol_data {
                ProtocolData::Ospf(data) => ospf_protocol_data_section(ui, data),
                _ => (),
            }
        });
    }
}

fn ospf_protocol_data_section(ui: &mut Ui, data: &OspfData) {
    collapsible_section(ui, "OSPF", false, |ui| {
        ui.add(label_no_wrap(format!("Area ID: {}", data.area_id)));
        ui.add(label_no_wrap(format!(
            "Advertising Router ID: {}",
            data.advertising_router
        )));
        ui.add(label_no_wrap(format!("Link State ID: {}", data.link_state_id)));
        if let Some(sum) = data.checksum {
            ui.add(label_no_wrap(format!("LSA checksum: {:x}", sum)));
        }
        ospf_payload_section(ui, &data.payload);
    });
}
fn ospf_payload_section(ui: &mut Ui, payload: &OspfPayload) {
    match payload {
        OspfPayload::Router(router) => {
            let tags = router.to_str_tags().join(", ");
            ui.label(format!("Tags: {}", tags));
            collapsible_section(ui, "Link Counts", false, |ui| {
                let counts = [
                    format!("Point to Point: {}", router.p2p_link_count),
                    format!("Transit: {}", router.transit_link_count),
                    format!("Stub: {}", router.stub_link_count),
                ];
                bullet_list(ui, counts);
            });
            collapsible_section(ui, "Link Metrics", false, |ui| {
                let metrics: Vec<String> = router
                    .link_metrics
                    .iter()
                    .map(|(addr, metric)| format!("{} : {}", addr, metric))
                    .collect();
                bullet_list(ui, metrics);
            });
        }
        _ => (),
    }
}

/// Convenience: Render a collapsible section with a standard grouped frame.
/// Use this to keep panels modular and extensible.
pub fn collapsible_section(
    ui: &mut Ui,
    title: impl Into<egui::WidgetText>,
    default_open: bool,
    add_contents: impl FnOnce(&mut Ui),
) {
    let collapsing = CollapsingHeader::new(title).default_open(default_open);

    collapsing.show(ui, |ui| {
        // Group frame to visually separate the section body
        Frame::group(ui.style()).show(ui, |ui| {
            //ui.set_width(ui.available_width());
            add_contents(ui);
        });
    });
}

pub fn label_no_wrap(text: impl Into<egui::WidgetText>) -> Label {
    Label::new(text).wrap_mode(egui::TextWrapMode::Extend)
}

/// Tiny helper to render a bullet point list.
pub fn bullet_list<I, S>(ui: &mut Ui, items: I)
where
    I: IntoIterator<Item = S>,
    S: ToString,
{
    for s in items {
        ui.horizontal(|ui| {
            ui.label("•");
            ui.label(s.to_string());
        });
    }
}

/// Internal: read persisted pin state.
fn persisted_pin(ctx: &Context, id: Id) -> Option<bool> {
    ctx.data_mut(|d| d.get_persisted::<bool>(id))
}

/// Internal: write persisted pin state.
fn set_persisted_pin(ctx: &Context, id: Id, value: bool) {
    ctx.data_mut(|d| d.insert_persisted(id, value));
}
