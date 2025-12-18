use std::cell::RefCell;
use std::collections::HashMap;

use egui::{Color32, Pos2, Shape, Stroke};
use egui_graphs::{DisplayEdge, DisplayNode, DrawContext, EdgeProps};
use petgraph::csr::EdgeIndex;
use petgraph::{EdgeType, stable_graph::IndexType};
use uuid::Uuid;

use crate::gui::app;
use crate::gui::node_shape::NetworkGraphNodeShape;
use crate::network::edge::{Edge as NetEdge, EdgeKind, EdgeMetric};

#[derive(Clone, Debug)]
pub struct EdgeEvent {
    pub src_uuid: Uuid,
    pub dst_uuid: Uuid,
    pub kind: EdgeKind,
    pub is_manual: bool,
}

thread_local! {
    static EDGE_EVENTS: RefCell<Vec<EdgeEvent>> = RefCell::new(Vec::new());
    static ANY_GRAPH_HIT: RefCell<bool> = RefCell::new(false);
    static EDGE_LABELS_ENABLED: RefCell<bool> = RefCell::new(false);
    static EDGE_WEIGHTS: RefCell<HashMap<(Uuid, Uuid), f32>> = RefCell::new(HashMap::new());
}

pub fn set_edge_weights(weights: HashMap<(Uuid, Uuid), f32>) {
    EDGE_WEIGHTS.with(|w| *w.borrow_mut() = weights);
}

pub fn insert_edge_weight(src: Uuid, dst: Uuid, weight: f32) {
    EDGE_WEIGHTS.with(|w| {
        let mut weights = w.borrow_mut();
        weights.insert((src, dst), weight);
    });
}

pub fn get_edge_weight(src: Uuid, dst: Uuid) -> Option<f32> {
    EDGE_WEIGHTS.with(|w| w.borrow().get(&(src, dst)).copied())
}

/// Enable/disable edge metric labels globally.
pub fn set_edge_labels_enabled(enabled: bool) {
    EDGE_LABELS_ENABLED.with(|b| *b.borrow_mut() = enabled);
}

/// Read current edge metric label toggle.
pub fn edge_labels_enabled() -> bool {
    EDGE_LABELS_ENABLED.with(|b| *b.borrow())
}

/// Clear the per-frame edge event queue.
pub fn clear_edge_events() {
    EDGE_EVENTS.with(|q| q.borrow_mut().clear());
}

/// Drain the edge event queue, returning all pending events.
pub fn take_edge_events() -> Vec<EdgeEvent> {
    EDGE_EVENTS.with(|q| q.borrow_mut().drain(..).collect())
}

/// Reset the "any hit" marker to false for the frame.
pub fn clear_any_hit() {
    ANY_GRAPH_HIT.with(|f| *f.borrow_mut() = false);
}

/// Mark that at least one graph element was hit (hovered/clicked) in this frame.
pub fn mark_hit() {
    ANY_GRAPH_HIT.with(|f| *f.borrow_mut() = true);
}

/// Read the current value of the "any hit" marker.
pub fn any_hit() -> bool {
    ANY_GRAPH_HIT.with(|f| *f.borrow())
}

/// Custom edge shape that draws a simple line and emits click events when selection changes.
#[derive(Clone, Debug)]
pub struct NetworkGraphEdgeShape {
    selected_prev: bool,
    // Cache current edge identity for animation lookup
    src_uuid: Option<uuid::Uuid>,
    dst_uuid: Option<uuid::Uuid>,
    kind: Option<crate::network::edge::EdgeKind>,
    metric: EdgeMetric,
}

impl Default for NetworkGraphEdgeShape {
    fn default() -> Self {
        Self {
            selected_prev: false,
            src_uuid: None,
            dst_uuid: None,
            kind: None,
            metric: EdgeMetric::None,
        }
    }
}

// Required by the trait bound: Clone + From<EdgeProps<E>>
impl From<EdgeProps<NetEdge>> for NetworkGraphEdgeShape {
    fn from(props: EdgeProps<NetEdge>) -> Self {
        NetworkGraphEdgeShape {
            selected_prev: false,
            src_uuid: Some(props.payload.source_id),
            dst_uuid: Some(props.payload.destination_id),
            kind: Some(props.payload.kind),
            metric: props.payload.metric,
        }
    }
}

impl<Ty: EdgeType, Ix: IndexType>
    DisplayEdge<crate::network::node::Node, NetEdge, Ty, Ix, NetworkGraphNodeShape>
    for NetworkGraphEdgeShape
{
    fn shapes(
        &mut self,
        start: &egui_graphs::Node<
            crate::network::node::Node,
            NetEdge,
            Ty,
            Ix,
            NetworkGraphNodeShape,
        >,
        end: &egui_graphs::Node<crate::network::node::Node, NetEdge, Ty, Ix, NetworkGraphNodeShape>,
        ctx: &DrawContext,
    ) -> Vec<Shape> {
        // Compute endpoints on node boundaries in canvas space
        let a = start.props().location();
        let b = end.props().location();
        let a_boundary = <NetworkGraphNodeShape as DisplayNode<
            crate::network::node::Node,
            NetEdge,
            Ty,
            Ix,
        >>::closest_boundary_point(start.display(), b - a);
        let b_boundary = <NetworkGraphNodeShape as DisplayNode<
            crate::network::node::Node,
            NetEdge,
            Ty,
            Ix,
        >>::closest_boundary_point(end.display(), a - b);
        let a_screen = ctx.meta.canvas_to_screen_pos(a_boundary);
        let b_screen = ctx.meta.canvas_to_screen_pos(b_boundary);

        let mut base = ctx.ctx.style().visuals.widgets.inactive.fg_stroke.color;

        // Default: no animation
        let traffic_width_modifier = 2.5;
        let base_width = 1.5f32 * (1.0 + traffic_width_modifier * get_edge_weight(self.src_uuid.unwrap(), self.dst_uuid.unwrap()).unwrap_or(0.0));
        let mut alpha_factor = 1.0f32;
        let mut width_scale = 1.0f32 * base_width;

        // Use cached identity (set in update()) to query animation state
        if let (Some(src), Some(dst), Some(kind)) = (self.src_uuid, self.dst_uuid, self.kind) {
            if let Some(anim) = crate::gui::edge_anim::get_anim(src, dst, kind) {
                // 300ms fade, using your ease_in_out_cubic
                let duration = std::time::Duration::from_millis(300);
                let p = anim.eased_progress(duration, ease_in_out_cubic);
                let theme = app::get_theme();
                match anim.phase {
                    crate::gui::edge_anim::EdgeAnimPhase::Creating => {
                        alpha_factor = p;
                        width_scale = 0.5 + 0.5 * p;
                        // Blend toward hovered bg_fill for a theme-aware “appearing” accent
                        let accent = theme.teal;
                        base = egui::Color32::from_rgb(
                            ((base.r() as u16 * 2 + accent.r() as u16) / 3) as u8,
                            ((base.g() as u16 * 2 + accent.g() as u16) / 3) as u8,
                            ((base.b() as u16 * 2 + accent.b() as u16) / 3) as u8,
                        );
                    }
                    crate::gui::edge_anim::EdgeAnimPhase::Destroying => {
                        let inv = 1.0 - p;
                        alpha_factor = inv;
                        width_scale = 0.5 + 0.5 * inv;
                        // Blend toward inactive bg_fill for a theme-aware “fading” accent
                        let accent = theme.red;
                        base = egui::Color32::from_rgb(
                            ((base.r() as u16 * 2 + accent.r() as u16) / 3) as u8,
                            ((base.g() as u16 * 2 + accent.g() as u16) / 3) as u8,
                            ((base.b() as u16 * 2 + accent.b() as u16) / 3) as u8,
                        );
                    }
                }
            }
        }

        let color = egui::Color32::from_rgba_unmultiplied(
            base.r(),
            base.g(),
            base.b(),
            (alpha_factor * 255.0) as u8,
        );
        let stroke = egui::Stroke {
            width: width_scale,
            color,
        };
        
        let line_length = (b_screen - a_screen).length();
        
        let mut shapes = match self.kind {
            Some(EdgeKind::Membership) => vec![Shape::line_segment([a_screen, b_screen], stroke)],
            _ => Shape::dashed_line(&[a_screen, b_screen], stroke, line_length / 10.0, line_length / 5.0)
        };
        // Optional metric label:
        if edge_labels_enabled() {
            println!("Metric label enabled");
            // Midpoint in screen space:
            let mid = egui::pos2(
                (a_screen.x + b_screen.x) * 0.5,
                (a_screen.y + b_screen.y) * 0.5,
            );
            // Offset the label slightly perpendicular to the edge, so it doesn't overlap the line:
            let dir = b_screen - a_screen;
            let n = egui::vec2(-dir.y, dir.x); // perpendicular
            let nlen = n.length();
            let offset = if nlen > 0.0 {
                n * (8.0 / nlen)
            } else {
                egui::vec2(0.0, 0.0)
            };
            let label_pos = mid + offset;

            // Fetch a human-readable metric string from the edge payload:
            // Adjust to your actual payload fields.
            let metric_text = match self.metric {
                EdgeMetric::Ospf(m) => Some(format!("OSPF: {}", m)),
                EdgeMetric::IsIs(m) => Some(format!("IS-IS: {}", m)),
                EdgeMetric::Manual(m) => Some(format!("Manual: {}", m)),
                _ => None,
            };

            if let Some(metric_text) = metric_text {
                println!("[edge_shape] Metric text: {}", metric_text);
                // Use egui font system to layout the text:
                let base_text = ctx.ctx.style().visuals.widgets.inactive.fg_stroke.color;
                let text_color = Color32::from_rgba_unmultiplied(
                    base_text.r(),
                    base_text.g(),
                    base_text.b(),
                    230,
                );
                ctx.ctx.fonts_mut(|fonts| {
                    let galley = fonts.layout_no_wrap(
                        metric_text,
                        egui::FontId::proportional(12.0),
                        text_color,
                    );
                    shapes.push(Shape::galley(label_pos, galley, text_color));
                });
            }
        }
        shapes
    }

    fn update(&mut self, props: &EdgeProps<NetEdge>) {
        // Cache identity for shapes() to use when looking up animation state
        self.src_uuid = Some(props.payload.source_id);
        self.dst_uuid = Some(props.payload.destination_id);
        self.kind = Some(props.payload.kind);
        self.metric = props.payload.metric.clone();

        // Emit event when selection transitions from false -> true.
        if props.selected && !self.selected_prev {
            crate::gui::edge_shape::EDGE_EVENTS.with(|q| {
                q.borrow_mut().push(crate::gui::edge_shape::EdgeEvent {
                    src_uuid: props.payload.source_id,
                    dst_uuid: props.payload.destination_id,
                    kind: props.payload.kind,
                    is_manual: props.payload.protocol_tag.as_deref() == Some("MANUAL"),
                });
            });
            crate::gui::edge_shape::mark_hit();
        }
        self.selected_prev = props.selected;
    }

    fn is_inside(
        &self,
        start: &egui_graphs::Node<
            crate::network::node::Node,
            NetEdge,
            Ty,
            Ix,
            NetworkGraphNodeShape,
        >,
        end: &egui_graphs::Node<crate::network::node::Node, NetEdge, Ty, Ix, NetworkGraphNodeShape>,
        pos: Pos2,
    ) -> bool {
        // pos is in canvas coordinates. Do a simple segment distance test (in canvas space).
        println!("Is inside triggered!");
        let a = start.props().location();
        let b = end.props().location();
        let a_boundary = <NetworkGraphNodeShape as DisplayNode<
            crate::network::node::Node,
            NetEdge,
            Ty,
            Ix,
        >>::closest_boundary_point(start.display(), b - a);
        let b_boundary = <NetworkGraphNodeShape as DisplayNode<
            crate::network::node::Node,
            NetEdge,
            Ty,
            Ix,
        >>::closest_boundary_point(end.display(), a - b);
        let dist = distance_point_to_segment(pos, a_boundary, b_boundary);
        let inside = dist <= 6.0;
        if inside {
            mark_hit();
        }
        inside
    }
}

fn distance_point_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ap = p - a;
    let ab = b - a;
    let ab_len2 = ab.length_sq();
    if ab_len2 <= f32::EPSILON {
        return ap.length();
    }
    let t = (ap.dot(ab) / ab_len2).clamp(0.0, 1.0);
    let closest = a + ab * t;
    (p - closest).length()
}

fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t.powi(3)
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}
