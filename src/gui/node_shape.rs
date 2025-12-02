use std::cell::RefCell;
use std::collections::HashSet;

use egui::{Color32, Pos2, Shape, Stroke, Vec2, epaint::CircleShape};
use egui::{ColorImage, Context, TextureId, TextureOptions};
use egui_graphs::{DisplayNode, DrawContext, NodeProps};

use petgraph::{EdgeType, stable_graph::IndexType};
use tiny_skia::Pixmap;
use usvg::Tree;
use uuid::Uuid;

use egui::TextureHandle;

use crate::network::node::{Node, NodeInfo};
use crate::network::router::RouterId;

thread_local! {
    static ROUTER_TEX: RefCell<Option<TextureHandle>> = RefCell::new(None);
    static NETWORK_TEX: RefCell<Option<TextureHandle>> = RefCell::new(None);
}

// Rasterize SVG bytes to a square RGBA buffer at the given target_px (keeps aspect)
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
    node_type: NodeType,
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

    static PATH_HIGHLIGHT: RefCell<HashSet<Uuid>> = RefCell::new(HashSet::new());
}

pub fn clear_path_highlight() {
    PATH_HIGHLIGHT.with(|v| v.borrow_mut().clear());
}

pub fn set_path_highlight(uuids: impl Iterator<Item = Uuid>) {
    PATH_HIGHLIGHT.with(|v| v.borrow_mut().extend(uuids))
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
            radius: 10f32,
            external: false,
            source_id: payload.source_id.clone(),
            node_uuid: payload.id,
            node_router_id: router_id,
            node_type: NodeType::from(&payload.info),
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
        let mut res = Vec::with_capacity(4);
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
        let fill = Color32::TRANSPARENT;

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

        // Draw node icon beneath highlight rings
        let half = circle_radius;
        let rect = egui::Rect::from_center_size(circle_center, Vec2::new(half * 2.0, half * 2.0));
        let tex_id: TextureId = match self.node_type {
            NodeType::Router => router_texture_id(ctx.ctx),
            NodeType::Network => network_texture_id(ctx.ctx),
        };
        let uv = egui::Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
        res.push(Shape::image(tex_id, rect, uv, self.effective_color(ctx)));

        // Base circle stroke (for highlight fade ring)
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

        let path_highlighted: bool = PATH_HIGHLIGHT.with_borrow(|v| v.contains(&self.node_uuid));

        let fade_path = ctx.ctx.animate_bool(
            egui::Id::new(("path_highlight", self.node_uuid)),
            path_highlighted,
        );

        if fade_path > 0.01 {
            let ring_radius = circle_radius + (2.5 + 0.1 * fade_path);
            let ring_color = Color32::PURPLE.linear_multiply(fade_path);
            let ring_stroke = Stroke {
                width: 2.0 * fade_path,
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
