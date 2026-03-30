use std::cell::RefCell;
use std::collections::HashSet;

use crate::gui::graph_overlay::{self, PathPreview};

use catppuccin_egui::Theme;
use egui::epaint::TextShape;
use egui::{Color32, Pos2, Shape, Stroke, Vec2, epaint::CircleShape};
use egui::{ColorImage, Context, FontFamily, FontId, TextureId, TextureOptions};
use egui_graphs::{DisplayNode, DrawContext, NodeProps};

use petgraph::{EdgeType, stable_graph::IndexType};
use tiny_skia::Pixmap;
use usvg::Tree;
use uuid::Uuid;

use egui::TextureHandle;

use crate::gui::app;
use crate::network::node::{Node, NodeInfo};
use crate::network::router::RouterId;

pub const DASH_LENGTH: f32 = 8.0;
pub const GAP_LENGTH: f32 = 6.0;

thread_local! {
    static ROUTER_TEX: RefCell<Option<TextureHandle>> = RefCell::new(None);
    static NETWORK_TEX: RefCell<Option<TextureHandle>> = RefCell::new(None);
}

/// Rasterize SVG bytes to a square RGBA buffer at the given target_px (keeps aspect)
fn rasterize_svg(svg_bytes: &[u8], target_px: u32) -> Option<ColorImage> {
    let opt = usvg::Options::default();
    let tree = Tree::from_data(svg_bytes, &opt).ok()?;
    // Fit so that the longest side is target_px; preserve aspect
    let size = tree.size();
    let int = size.to_int_size();
    let max_side = int.width().max(int.height()).max(1) as f32;
    let scale = (target_px as f32 / max_side).max(1.0 / max_side); // avoid 0

    let w = ((int.width() as f32) * scale).ceil().max(1.0) as u32;
    let h = ((int.height() as f32) * scale).ceil().max(1.0) as u32;

    let mut pixmap = Pixmap::new(w, h)?;
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    let mut pm = pixmap.as_mut();
    resvg::render(&tree, transform, &mut pm);

    let data = pixmap.data().to_vec();
    let image = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &data);
    Some(image)
}

fn ensure_router(ctx: &Context) -> TextureHandle {
    ROUTER_TEX.with(|slot| {
        if let Some(tex) = slot.borrow().as_ref() {
            return tex.clone();
        }
        let svg = include_bytes!("resources/router-node.svg");
        // Choose a base texture resolution; 64–128 px works well
        let img = rasterize_svg(svg, 96).expect("Failed to rasterize router-node.svg");
        let tex = ctx.load_texture("router-node", img, TextureOptions::LINEAR);
        *slot.borrow_mut() = Some(tex.clone());
        tex
    })
}

fn ensure_network(ctx: &Context) -> TextureHandle {
    NETWORK_TEX.with(|slot| {
        if let Some(tex) = slot.borrow().as_ref() {
            return tex.clone();
        }
        let svg = include_bytes!("resources/network-node.svg");
        let img = rasterize_svg(svg, 96).expect("Failed to rasterize network-node.svg");
        let tex = ctx.load_texture("network-node", img, TextureOptions::LINEAR);
        *slot.borrow_mut() = Some(tex.clone());
        tex
    })
}

pub fn router_texture_id(ctx: &Context) -> TextureId {
    ensure_router(ctx).id()
}
pub fn network_texture_id(ctx: &Context) -> TextureId {
    ensure_network(ctx).id()
}

#[derive(Debug, Clone)]
enum NodeType {
    Router,
    Network,
}

impl From<&NodeInfo> for NodeType {
    fn from(node_info: &NodeInfo) -> Self {
        match node_info {
            NodeInfo::Router(_) => NodeType::Router,
            NodeInfo::Network(_) => NodeType::Network,
        }
    }
}

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
    pub theme: Theme,
    node_type: NodeType,
}

// Thread-local overlay collector populated during shapes() and consumed after the GraphView is drawn.
pub type LabelOverlay = graph_overlay::LabelOverlay;

thread_local! {
    static LABEL_OVERLAY: RefCell<Vec<LabelOverlay>> = RefCell::new(Vec::new());
    static HOVERED_SOURCE_ID: RefCell<Option<RouterId>> = RefCell::new(None);
    // Global toggle for partition highlighting
    static HIGHLIGHT_ENABLED: RefCell<bool> = RefCell::new(true);

    static PATH_HIGHLIGHT: RefCell<HashSet<Uuid>> = RefCell::new(HashSet::new());
}

pub fn clear_path_highlight(ctx: &Context) {
    graph_overlay::clear_path_highlight(ctx);
}

pub fn set_path_highlight(ctx: &Context, uuids: impl Iterator<Item = Uuid>) {
    graph_overlay::set_path_highlight(ctx, uuids);
}

/// Clear the hovered-area state at the start of a frame.
pub fn clear_area_highlight(ctx: &Context) {
    graph_overlay::clear_area_highlight(ctx);
}

/// Enable/disable partition highlighting globally.
pub fn set_partition_highlight_enabled(ctx: &Context, enabled: bool) {
    graph_overlay::set_partition_highlight_enabled(ctx, enabled);
}

/// Read current partition highlighting toggle.
pub fn partition_highlight_enabled(ctx: &Context) -> bool {
    graph_overlay::partition_highlight_enabled(ctx)
}

pub fn clear_label_overlays(ctx: &Context) {
    graph_overlay::clear_label_overlays(ctx);
}

pub fn take_label_overlays(ctx: &Context) -> Vec<LabelOverlay> {
    graph_overlay::take_label_overlays(ctx)
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
            radius: 10f32,
            external: false,
            source_id: payload.source_id.clone(),
            node_uuid: payload.id,
            node_router_id: router_id,
            node_type: NodeType::from(&payload.info),
            theme: app::get_theme(),
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
        let mut shapes = Vec::new();
        let circle_center = ctx.meta.canvas_to_screen_pos(self.pos);
        let circle_radius = ctx.meta.canvas_to_screen_size(self.radius);

        // Path preview
        if let Some(preview) = self.should_draw_path_preview(ctx) {
            self.draw_path_preview(ctx, circle_center, &mut shapes, preview);
        }

        // Partition highlight recompute
        let highlight_on = partition_highlight_enabled(ctx.ctx);
        let hovered_src = graph_overlay::hovered_source_id(ctx.ctx);
        if highlight_on && self.hovered {
            graph_overlay::set_hovered_source_id(ctx.ctx, self.source_id.clone());
        }
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
        let fill = Color32::TRANSPARENT;

        // Smooth fade ring ONLY for origin
        let fade_highlighted = ctx.ctx.animate_bool(
            egui::Id::new(("partition_highlight", self.node_uuid)),
            self.highlighted || self.hovered || self.selected,
        );
        // Neutral stroke using theme (hovered fg for emphasis)
        let hovered_fg = ctx.ctx.style().visuals.widgets.hovered.fg_stroke.color;
        let stroke = Stroke {
            width: 1.0 * fade_highlighted,
            color: hovered_fg.linear_multiply(fade_highlighted),
        };

        // Draw node icon beneath highlight rings
        let half = circle_radius;
        let rect = egui::Rect::from_center_size(circle_center, Vec2::new(half * 2.0, half * 2.0));
        let tex_id: TextureId = match self.node_type {
            NodeType::Router => router_texture_id(ctx.ctx),
            NodeType::Network => network_texture_id(ctx.ctx),
        };
        let uv = egui::Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
        shapes.push(Shape::image(tex_id, rect, uv, self.effective_color(ctx)));

        // Base circle stroke (for highlight fade ring)
        shapes.push(
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
            // Use selection bg_fill for origin ring to align with theme
            let ring_color = ctx
                .ctx
                .style()
                .visuals
                .selection
                .bg_fill
                .linear_multiply(fade_origin);
            let ring_stroke = Stroke {
                width: 2.0 * fade_origin,
                color: ring_color,
            };
            shapes.push(
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
            graph_overlay::push_label_overlay(
                ctx.ctx,
                LabelOverlay {
                    center: circle_center,
                    circle_radius,
                    text: self.label.clone(),
                    color: fill,
                },
            );
        }

        let path_highlighted: bool = graph_overlay::is_path_highlighted(ctx.ctx, &self.node_uuid);

        let fade_path = ctx.ctx.animate_bool(
            egui::Id::new(("path_highlight", self.node_uuid)),
            path_highlighted,
        );

        if fade_path > 0.01 {
            let ring_radius = circle_radius + (2.5 + 0.1 * fade_path);
            // Use hovered bg_fill for path accent
            let ring_color = self.theme.mauve.linear_multiply(fade_path);
            let ring_stroke = Stroke {
                width: 2.0 * fade_path,
                color: ring_color,
            };
            shapes.push(
                CircleShape {
                    center: circle_center,
                    radius: ring_radius,
                    fill: Color32::TRANSPARENT,
                    stroke: ring_stroke,
                }
                .into(),
            );
        }
        
        let galley = self.label_galley(ctx, circle_radius, self.effective_color(ctx));
        
        let label_shape = Self::label_shape(galley, circle_center, circle_radius, self.effective_color(ctx));
        
        shapes.push(label_shape.into());

        shapes
    }

    fn update(&mut self, state: &NodeProps<Node>) {
        self.pos = state.location();
        self.selected = state.selected;
        self.dragged = state.dragged;
        self.hovered = state.hovered;
        self.label = state.label.to_string();
        self.color = state.color();
        self.source_id = state.payload.source_id.clone();
        self.theme = app::get_theme();

        // If highlighting is enabled and this node is hovered, publish its partition (SourceId) for frame-wide highlight
    }
}

impl NetworkGraphNodeShape {
    fn is_interacted(&self) -> bool {
        self.selected
    }

    fn effective_color(&self, _ctx: &DrawContext) -> Color32 {
        let mut base = match self.node_type {
            NodeType::Router => self.theme.blue,
            NodeType::Network => self.theme.green,
        };

        if self.hovered || self.selected {
            base = Color32::from_rgb(
                base.r().saturating_add(40).min(255),
                base.g().saturating_add(100).min(255),
                base.b().saturating_sub(40).max(0),
            );
        }

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

    fn should_draw_path_preview(&self, ctx: &DrawContext) -> Option<PathPreview> {
        graph_overlay::path_preview(ctx.ctx)
            .filter(|preview| preview.start_uuid() == self.node_uuid)
    }

    fn draw_path_preview(
        &self,
        ctx: &DrawContext,
        circle_center: Pos2,
        shapes: &mut Vec<Shape>,
        preview: PathPreview,
    ) {
        let cursor_pos = preview.cursor_screen_pos();

        let base_color = ctx.ctx.style().visuals.widgets.inactive.fg_stroke.color;
        let color = base_color.gamma_multiply(0.6);
        let stroke = Stroke::new(1.5, color);

        // Change this later to animate the line
        let dash_offset = preview.dash_phase();

        let line = egui::Shape::dashed_line_with_offset(
            &[circle_center, cursor_pos],
            stroke,
            &[DASH_LENGTH],
            &[GAP_LENGTH],
            dash_offset,
        );
        shapes.extend(line);
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
    ) -> Shape {
        let label_pos = Pos2::new(center.x - galley.size().x / 2., center.y - radius * 2.);
        TextShape::new(label_pos, galley, color).into()
    }
}

fn closest_point_on_circle(center: Pos2, radius: f32, dir: Vec2) -> Pos2 {
    center + dir.normalized() * (radius + 1.0)
}

fn is_inside_circle(center: Pos2, radius: f32, pos: Pos2) -> bool {
    let dir = pos - center;
    dir.length() <= radius
}
