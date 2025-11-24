use std::cell::RefCell;

use egui::{
    Color32, Pos2, Shape, Stroke, Vec2,
    epaint::CircleShape,
};
use egui_graphs::{DisplayNode, DrawContext, NodeProps};
use petgraph::{EdgeType, stable_graph::IndexType};

use crate::network::router::RouterId;
use crate::network::{
    node::{Node, NodeInfo}
};

#[derive(Clone)]
#[allow(dead_code)]
pub struct NetworkGraphNodeShape {
    pub label: String,
    pub pos: Pos2,
    pub radius: f32,
    pub color: Option<Color32>,
    pub selected: bool,
    pub dragged: bool,
    pub hovered: bool,
    pub highlighted: bool,
    pub external: bool,
    pub source_id: Option<RouterId>,
    pub node_uuid: uuid::Uuid, // stable id for animation
    pub node_router_id: Option<RouterId>,
}

// Thread-local overlay collector populated during shapes() and consumed after the GraphView is drawn.
#[derive(Clone)]
#[allow(dead_code)]
pub struct LabelOverlay {
    pub center: Pos2,
    pub circle_radius: f32,
    pub text: String,
    pub color: Color32,
}

thread_local! {
    static LABEL_OVERLAY: RefCell<Vec<LabelOverlay>> = RefCell::new(Vec::new());
    // Current hovered OSPF area for the frame, set by hovered node during update()
    static HOVERED_SOURCE_ID: RefCell<Option<RouterId>> = RefCell::new(None);
    // Global toggle for partition highlighting
    static HIGHLIGHT_ENABLED: RefCell<bool> = RefCell::new(true);
}

/// Clear the hovered-area state at the start of a frame.
pub fn clear_area_highlight() {
    HOVERED_SOURCE_ID.with(|v| *v.borrow_mut() = None);
}

/// Enable/disable partition highlighting globally.
pub fn set_partition_highlight_enabled(enabled: bool) {
    HIGHLIGHT_ENABLED.with(|v| *v.borrow_mut() = enabled);
}

/// Read current partition highlighting toggle.
pub fn partition_highlight_enabled() -> bool {
    HIGHLIGHT_ENABLED.with(|v| *v.borrow())
}

pub fn clear_label_overlays() {
    LABEL_OVERLAY.with(|v| v.borrow_mut().clear());
}

pub fn take_label_overlays() -> Vec<LabelOverlay> {
    LABEL_OVERLAY.with(|v| v.borrow_mut().drain(..).collect())
}

impl From<NodeProps<Node>> for NetworkGraphNodeShape {
    fn from(node_props: NodeProps<Node>) -> Self {
        let payload = &node_props.payload;
        let router_id = if let NodeInfo::Router(router) = &payload.info {
            Some(router.id.clone())
        } else {
            None
        };
        Self {
            pos: node_props.location(),
            color: node_props.color(),
            label: node_props.label,
            selected: node_props.selected,
            dragged: node_props.dragged,
            hovered: node_props.hovered,
            highlighted: false,
            radius: 5f32,
            external: false,
            source_id: payload.source_id.clone(),
            node_uuid: payload.id,
            node_router_id: router_id,
        }
    }
}

impl<E: Clone, Ty: EdgeType, Ix: IndexType> DisplayNode<Node, E, Ty, Ix> for NetworkGraphNodeShape {
    fn closest_boundary_point(&self, dir: Vec2) -> Pos2 {
        closest_point_on_circle(self.pos, self.radius, dir)
    }

    fn is_inside(&self, pos: Pos2) -> bool {
        is_inside_circle(self.pos, self.radius, pos)
    }

    fn shapes(&mut self, ctx: &egui_graphs::DrawContext) -> Vec<Shape> {
        let mut res = Vec::with_capacity(2);
        let circle_center = ctx.meta.canvas_to_screen_pos(self.pos);
        let circle_radius = ctx.meta.canvas_to_screen_size(self.radius);

        // Partition highlight recompute
        let highlight_on = partition_highlight_enabled();
        let hovered_src = HOVERED_SOURCE_ID.with(|v| (*v.borrow()).clone());
        self.highlighted = highlight_on
            && hovered_src.is_some()
            && self.source_id.is_some()
            && self.source_id == hovered_src;

        // Determine origin (the node currently hovered)
        let is_origin = self.highlighted
            && self
                .node_router_id
                .as_ref()
                .is_some_and(|id| hovered_src.is_some_and(|src_id| *id == src_id));

        // Base fill (tint if highlighted)
        let fill = self.effective_color(ctx);

        // Smooth fade ring ONLY for origin
        let fade_highlighted = ctx.ctx.animate_bool(
            egui::Id::new(("partition_highlight", self.node_uuid)),
            self.highlighted,
        );
        // Neutral stroke (no yellow ring here)
        let stroke = Stroke {
            width: 1.0 * fade_highlighted,
            color: Color32::YELLOW.linear_multiply(fade_highlighted),
        };

        res.push(
            CircleShape {
                center: circle_center,
                radius: circle_radius,
                fill,
                stroke,
            }
            .into(),
        );

        let fade_origin = ctx.ctx.animate_bool(
            egui::Id::new(("partition_origin_highlight", self.node_uuid)),
            is_origin,
        );
        if fade_origin > 0.01 {
            let ring_radius = circle_radius * (1.25 + 0.10 * fade_origin);
            let ring_color = Color32::YELLOW.linear_multiply(fade_origin);
            let ring_stroke = Stroke {
                width: 2.0 * fade_origin,
                color: ring_color,
            };
            res.push(
                CircleShape {
                    center: circle_center,
                    radius: ring_radius,
                    fill: Color32::TRANSPARENT,
                    stroke: ring_stroke,
                }
                .into(),
            );
        }

        if self.is_interacted() {
            LABEL_OVERLAY.with(|v| {
                v.borrow_mut().push(LabelOverlay {
                    center: circle_center,
                    circle_radius,
                    text: self.label.clone(),
                    color: fill,
                });
            });
        }

        res
    }

    fn update(&mut self, state: &NodeProps<Node>) {
        self.pos = state.location();
        self.selected = state.selected;
        self.dragged = state.dragged;
        self.hovered = state.hovered;
        self.label = state.label.to_string();
        self.color = state.color();
        self.source_id = state.payload.source_id.clone();

        // If highlighting is enabled and this node is hovered, publish its partition (SourceId) for frame-wide highlight
        if partition_highlight_enabled() && self.hovered {
            HOVERED_SOURCE_ID.with(|v| *v.borrow_mut() = self.source_id.clone());
        }
    }
}

impl NetworkGraphNodeShape {
    fn is_interacted(&self) -> bool {
        self.selected || self.dragged || self.hovered
    }

    fn effective_color(&self, ctx: &DrawContext) -> Color32 {
        if let Some(c) = self.color {
            return c;
        }

        let style = if self.is_interacted() {
            ctx.ctx.style().visuals.widgets.active
        } else {
            ctx.ctx.style().visuals.widgets.inactive
        };

        let mut base = style.fg_stroke.color;
        if self.highlighted {
            // Warm tint to indicate same-area highlight
            base = Color32::from_rgb(
                base.r().saturating_add(40).min(255),
                base.g().saturating_add(100).min(255),
                base.b().saturating_sub(40).max(0),
            );
        }
        base
    }
    
    #[allow(dead_code)]
    fn effective_stroke(&self, _ctx: &DrawContext) -> Stroke {
        if self.highlighted {
            Stroke {
                width: 2.0,
                color: Color32::YELLOW,
            }
        } else {
            Stroke::default()
        }
    }
}


fn closest_point_on_circle(center: Pos2, radius: f32, dir: Vec2) -> Pos2 {
    center + dir.normalized() * (radius + 1.0)
}

fn is_inside_circle(center: Pos2, radius: f32, pos: Pos2) -> bool {
    let dir = pos - center;
    dir.length() <= radius
}
