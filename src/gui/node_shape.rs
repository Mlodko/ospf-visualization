use egui::{Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Shape, Stroke, Vec2, epaint::{CircleShape, RectShape, TextShape}};
use egui_graphs::{DisplayNode, DrawContext, NodeProps};
use petgraph::{stable_graph::IndexType, EdgeType};

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

impl<N: Clone> From<NodeProps<N>> for MyNodeShape {
    fn from(node_props: NodeProps<N>) -> Self {
        Self {
            pos: node_props.location(),
            color: node_props.color(),
            label: node_props.label,
            selected: node_props.selected,
            dragged: node_props.dragged,
            hovered: node_props.hovered,
            
            radius: 5f32
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
        let mut res = Vec::with_capacity(3);
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
        
        if !self.is_interacted() {
            return res;
        }
        
        let galley = self.label_galley(ctx, circle_radius, color);
        res.extend(Self::label_shape(
            galley,
            circle_center,
            circle_radius,
            color,
            10f32
        ));
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
    
    fn effective_stroke(&self, ctx: &DrawContext) -> Stroke {
        Stroke::default()
    }
    
    fn label_galley(
        &self,
        ctx: &DrawContext,
        radius: f32,
        color: Color32,
    ) -> std::sync::Arc<egui::Galley> {
        ctx.ctx.fonts_mut(|f| {
            f.layout_no_wrap(
                self.label.clone(),
                FontId::new(radius, FontFamily::Monospace),
                color,
            )
        })
    }
    
    fn label_shape(
        galley: std::sync::Arc<egui::Galley>,
        center: Pos2,
        radius: f32,
        color: Color32,
        circle_padding: f32
    ) -> Vec<Shape> {
        // Top left corner
        let label_pos = Pos2::new(center.x - galley.size().x / 2., center.y - radius * 2. - galley.size().y - circle_padding);
        
        // padding around the text inside the background rectangle
        let pad = Vec2::new(6.0, 4.0);
        let rect_min = Pos2::new(label_pos.x - pad.x, label_pos.y - pad.y);
        let rect_max = Pos2::new(label_pos.x + galley.size().x + pad.x, label_pos.y + galley.size().y + pad.y);
        let rect = Rect::from_min_max(rect_min, rect_max);

        // semi-transparent black background (adjust alpha 0..=255)
        let bg_fill = Color32::from_black_alpha(160);
        // optional border for the background; use Stroke::default() or create a thin stroke
        let bg_stroke = Stroke::new(0.0, Color32::TRANSPARENT);
        
        let bg = RectShape::filled(rect, CornerRadius::ZERO, bg_fill).into();
        
        let text = TextShape::new(label_pos, galley, color).into();
        
        vec![bg, text]
    }
}


fn closest_point_on_circle(center: Pos2, radius: f32, dir: Vec2) -> Pos2 {
    center + dir.normalized() * radius
}

fn is_inside_circle(center: Pos2, radius: f32, pos: Pos2) -> bool {
    let dir = pos - center;
    dir.length() <= radius
}