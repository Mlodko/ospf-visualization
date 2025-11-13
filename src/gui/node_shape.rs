use std::cell::RefCell;

use egui::{
    Color32, Context, CornerRadius, FontFamily, FontId, Pos2, Rect, Shape, Stroke, Vec2,
    epaint::{CircleShape, RectShape, TextShape},
};
use egui_graphs::{DisplayNode, DrawContext, NodeProps};
use petgraph::{EdgeType, stable_graph::IndexType};

#[derive(Clone)]
pub struct MyNodeShape {
    pub label: String,
    pub pos: Pos2,
    pub radius: f32,
    pub color: Option<Color32>,
    pub selected: bool,
    pub dragged: bool,
    pub hovered: bool,
}

// Thread-local overlay collector populated during shapes() and consumed after the GraphView is drawn.
#[derive(Clone)]
pub struct LabelOverlay {
    pub center: Pos2,
    pub circle_radius: f32,
    pub text: String,
    pub color: Color32,
}

thread_local! {
    static LABEL_OVERLAY: RefCell<Vec<LabelOverlay>> = RefCell::new(Vec::new());
}

pub fn clear_label_overlays() {
    LABEL_OVERLAY.with(|v| v.borrow_mut().clear());
}

pub fn take_label_overlays() -> Vec<LabelOverlay> {
    LABEL_OVERLAY.with(|v| v.borrow_mut().drain(..).collect())
}

impl<N: Clone> From<NodeProps<N>> for MyNodeShape {
    fn from(node_props: NodeProps<N>) -> Self {
        Self {
            pos: node_props.location(),
            color: node_props.color(),
            label: node_props.label,
            selected: node_props.selected,
            dragged: node_props.dragged,
            hovered: node_props.hovered,

            radius: 5f32,
        }
    }
}

impl<N: Clone, E: Clone, Ty: EdgeType, Ix: IndexType> DisplayNode<N, E, Ty, Ix> for MyNodeShape {
    fn closest_boundary_point(&self, dir: Vec2) -> Pos2 {
        closest_point_on_circle(self.pos, self.radius, dir)
    }

    fn is_inside(&self, pos: Pos2) -> bool {
        is_inside_circle(self.pos, self.radius, pos)
    }

    fn shapes(&mut self, ctx: &egui_graphs::DrawContext) -> Vec<Shape> {
        // Only return the circle shape to avoid causing egui_graphs edge hit-testing to panic
        // when encountering Rect/Text shapes. For labels, collect overlay draw data instead.
        let mut res = Vec::with_capacity(1);
        let circle_center = ctx.meta.canvas_to_screen_pos(self.pos);
        let circle_radius = ctx.meta.canvas_to_screen_size(self.radius);
        let color = self.effective_color(ctx);
        let stroke = self.effective_stroke(ctx);

        res.push(
            CircleShape {
                center: circle_center,
                radius: circle_radius,
                fill: color,
                stroke,
            }
            .into(),
        );

        // Collect overlay label info for interacted nodes (selected/hovered/dragged).
        if self.is_interacted() {
            LABEL_OVERLAY.with(|v| {
                v.borrow_mut().push(LabelOverlay {
                    center: circle_center,
                    circle_radius,
                    text: self.label.clone(),
                    color,
                });
            });
        }

        res
    }

    fn update(&mut self, state: &NodeProps<N>) {
        self.pos = state.location();
        self.selected = state.selected;
        self.dragged = state.dragged;
        self.hovered = state.hovered;
        self.label = state.label.to_string();
        self.color = state.color();
    }
}

impl MyNodeShape {
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

        style.fg_stroke.color
    }

    fn effective_stroke(&self, _ctx: &DrawContext) -> Stroke {
        Stroke::default()
    }

    pub(crate) fn label_shape(
        galley: std::sync::Arc<egui::Galley>,
        center: Pos2,
        radius: f32,
        color: Color32,
        circle_padding: f32,
    ) -> Vec<Shape> {
        // This helper is retained for reference, but we no longer return these shapes
        // from shapes() because that can cause the edge hit-test panic in egui_graphs.
        let label_pos = Pos2::new(
            center.x - galley.size().x / 2.,
            center.y - radius * 2. - galley.size().y - circle_padding,
        );
        let pad = Vec2::new(6.0, 4.0);
        let rect_min = Pos2::new(label_pos.x - pad.x, label_pos.y - pad.y);
        let rect_max = Pos2::new(
            label_pos.x + galley.size().x + pad.x,
            label_pos.y + galley.size().y + pad.y,
        );
        let rect = Rect::from_min_max(rect_min, rect_max);

        let bg_fill = Color32::from_black_alpha(160);
        let bg = RectShape::filled(rect, CornerRadius::ZERO, bg_fill).into();
        let text = TextShape::new(label_pos, galley, color).into();

        vec![bg, text]
    }
}

pub(crate) fn build_label_galley(
    ctx: &Context,
    text: &str,
    radius: f32,
    color: Color32,
) -> std::sync::Arc<egui::Galley> {
    ctx.fonts_mut(|f| {
        f.layout_no_wrap(
            text.to_owned(),
            FontId::new(radius, FontFamily::Monospace),
            color,
        )
    })
}

fn closest_point_on_circle(center: Pos2, radius: f32, dir: Vec2) -> Pos2 {
    center + dir.normalized() * (radius + 1.0)
}

fn is_inside_circle(center: Pos2, radius: f32, pos: Pos2) -> bool {
    let dir = pos - center;
    dir.length() <= radius
}
