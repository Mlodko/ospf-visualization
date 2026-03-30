use std::collections::{HashMap, HashSet};

use egui::{Color32, Context, Id, Pos2};
use uuid::Uuid;

use crate::network::edge::EdgeKind;
use crate::network::router::RouterId;

/// Per-frame edge interaction event.
#[derive(Clone, Debug)]
pub struct EdgeEvent {
    pub src_uuid: Uuid,
    pub dst_uuid: Uuid,
    pub kind: EdgeKind,
    pub is_manual: bool,
}

/// Overlay label pushed by node shapes during graph rendering.
#[derive(Clone, Debug)]
pub struct LabelOverlay {
    pub center: Pos2,
    pub circle_radius: f32,
    pub text: String,
    pub color: Color32,
}

/// Context-backed overlay state used during graph rendering.
#[derive(Default, Clone, Debug)]
pub struct GraphOverlayState {
    pub label_overlays: Vec<LabelOverlay>,
    pub edge_events: Vec<EdgeEvent>,
    pub any_graph_hit: bool,

    pub hovered_source_id: Option<RouterId>,
    pub highlight_enabled: bool,
    pub path_highlight: HashSet<Uuid>,

    pub edge_labels_enabled: bool,
    pub edge_weights: HashMap<(Uuid, Uuid), f32>,
    
    pub path_preview: Option<PathPreview>,
}

fn overlay_id() -> Id {
    Id::new("graph_overlay_state")
}

/// Access the overlay state stored in the egui `Context`.
/// The state is stored in temporary data and written back after the closure runs.
pub fn with_overlay<R>(ctx: &Context, f: impl FnOnce(&mut GraphOverlayState) -> R) -> R {
    let id = overlay_id();
    ctx.data_mut(|d| {
        let mut state = d.get_temp::<GraphOverlayState>(id).unwrap_or_default();
        let out = f(&mut state);
        d.insert_temp(id, state);
        out
    })
}

/// Clear per-frame overlay state before rendering the graph.
pub fn clear_frame_state(ctx: &Context) {
    with_overlay(ctx, |o| {
        o.label_overlays.clear();
        o.edge_events.clear();
        o.any_graph_hit = false;
        o.hovered_source_id = None;
    });
}

// ---- Label overlays ----

pub fn push_label_overlay(ctx: &Context, overlay: LabelOverlay) {
    with_overlay(ctx, |o| o.label_overlays.push(overlay));
}

pub fn clear_label_overlays(ctx: &Context) {
    with_overlay(ctx, |o| o.label_overlays.clear());
}

pub fn take_label_overlays(ctx: &Context) -> Vec<LabelOverlay> {
    with_overlay(ctx, |o| std::mem::take(&mut o.label_overlays))
}

// ---- Edge events ----

pub fn push_edge_event(ctx: &Context, event: EdgeEvent) {
    with_overlay(ctx, |o| o.edge_events.push(event));
}

pub fn clear_edge_events(ctx: &Context) {
    with_overlay(ctx, |o| o.edge_events.clear());
}

pub fn take_edge_events(ctx: &Context) -> Vec<EdgeEvent> {
    with_overlay(ctx, |o| std::mem::take(&mut o.edge_events))
}

// ---- Any-hit marker ----

pub fn clear_any_hit(ctx: &Context) {
    with_overlay(ctx, |o| o.any_graph_hit = false);
}

pub fn mark_hit(ctx: &Context) {
    with_overlay(ctx, |o| o.any_graph_hit = true);
}

pub fn any_hit(ctx: &Context) -> bool {
    with_overlay(ctx, |o| o.any_graph_hit)
}

// ---- Hover + highlight ----

pub fn clear_area_highlight(ctx: &Context) {
    with_overlay(ctx, |o| o.hovered_source_id = None);
}

pub fn set_hovered_source_id(ctx: &Context, id: Option<RouterId>) {
    with_overlay(ctx, |o| o.hovered_source_id = id);
}

pub fn hovered_source_id(ctx: &Context) -> Option<RouterId> {
    with_overlay(ctx, |o| o.hovered_source_id.clone())
}

pub fn set_partition_highlight_enabled(ctx: &Context, enabled: bool) {
    with_overlay(ctx, |o| o.highlight_enabled = enabled);
}

pub fn partition_highlight_enabled(ctx: &Context) -> bool {
    with_overlay(ctx, |o| o.highlight_enabled)
}

// ---- Path highlight ----

pub fn clear_path_highlight(ctx: &Context) {
    with_overlay(ctx, |o| o.path_highlight.clear());
}

pub fn set_path_highlight(ctx: &Context, uuids: impl Iterator<Item = Uuid>) {
    with_overlay(ctx, |o| o.path_highlight.extend(uuids));
}

pub fn is_path_highlighted(ctx: &Context, id: &Uuid) -> bool {
    with_overlay(ctx, |o| o.path_highlight.contains(id))
}

// ---- Edge labels + weights ----

pub fn set_edge_labels_enabled(ctx: &Context, enabled: bool) {
    with_overlay(ctx, |o| o.edge_labels_enabled = enabled);
}

pub fn edge_labels_enabled(ctx: &Context) -> bool {
    with_overlay(ctx, |o| o.edge_labels_enabled)
}

pub fn set_edge_weights(ctx: &Context, weights: HashMap<(Uuid, Uuid), f32>) {
    with_overlay(ctx, |o| o.edge_weights = weights);
}

pub fn insert_edge_weight(ctx: &Context, src: Uuid, dst: Uuid, weight: f32) {
    with_overlay(ctx, |o| {
        o.edge_weights.insert((src, dst), weight);
    });
}

pub fn get_edge_weight(ctx: &Context, src: Uuid, dst: Uuid) -> Option<f32> {
    with_overlay(ctx, |o| o.edge_weights.get(&(src, dst)).copied())
}

// Path Preview - responsible for the dashed line when selecting a path

#[derive(Debug, Clone, Copy)]
pub struct PathPreview {
    start_uuid: Uuid,
    cursor_screen_pos: Pos2,
    dash_phase: f32
}

impl PathPreview {
    pub fn new(start_uuid: Uuid, cursor_screen_pos: Pos2, dash_phase: f32) -> Self {
        Self {
            start_uuid,
            cursor_screen_pos,
            dash_phase,
        }
    }
    
    pub fn start_uuid(&self) -> Uuid {
        self.start_uuid
    }
    
    pub fn cursor_screen_pos(&self) -> Pos2 {
        self.cursor_screen_pos
    }
    
    pub fn dash_phase(&self) -> f32 {
        self.dash_phase
    }
}

pub fn path_preview(ctx: &Context) -> Option<PathPreview> {
    with_overlay(ctx, |over| {
        over.path_preview
    })
}

pub fn set_path_preview(ctx: &Context, preview: PathPreview) {
    with_overlay(ctx, |over| {
        over.path_preview = Some(preview);
    });
}

pub fn clear_path_preview(ctx: &Context) -> Option<PathPreview> {
    with_overlay(ctx, |overlay| {
        overlay.path_preview.take()
    })
}