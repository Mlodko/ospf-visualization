
use egui::Context;
use uuid::Uuid;

use crate::{gui::{actions::AppActions, graph_overlay::{self, PathPreview}, node_shape, tools::{
    icons::{IconId, load_icon},
    tray::{Tool, ToolId},
}}, network::network_graph::NetworkGraph};



pub struct PathTool;

impl Tool for PathTool {
    fn icon(&self, ctx: &egui::Context) -> egui::TextureId {
        let tint = ctx.theme().default_visuals().text_color();
        load_icon(ctx, IconId::Path, 64, Some(tint)).id()
    }

    fn name(&self) -> &str {
        "Path Tool"
    }

    fn description(&self) -> &str {
        "Display the shortest path between two nodes."
    }

    fn on_click(
        &self,
        ctx: &egui::Context,
        pos: egui::Pos2,
        actions: &mut dyn crate::gui::actions::AppActions,
    ) {
        println!("Path tool clicked at {:?}", pos);
    }
    
    fn id(&self) -> ToolId {
        ToolId::PathTool
    }
}

#[derive(Debug, Clone, Default)]
pub struct PathToolState {
    start: Option<Uuid>,
    end: Option<Uuid>,
    last_selected: Option<Uuid>,
    ignore_selected: Option<Uuid>,
    active_last_frame: bool,
    dash_phase: f32
}

impl PathToolState {
    fn state_id() -> egui::Id {
        egui::Id::new("path_tool_state")
    }

    pub fn with_state<R>(ctx: &Context, f: impl FnOnce(&mut PathToolState) -> R) -> R {
        let id = Self::state_id();
        let mut state = ctx.data(|data| {
            data.get_temp::<PathToolState>(id).unwrap_or_default()
        });
        let out = f(&mut state);
        ctx.data_mut(|data| {
            data.insert_temp(id, state);
        });
        out
    }
    
    pub fn clear_state(ctx: &Context) -> Option<Self> {
        ctx.data_mut(|data| {
            data.remove_temp::<PathToolState>(Self::state_id())
        })
    }
    
    pub fn advance_frame(&mut self, ctx: &Context, actions: &mut dyn AppActions, is_active: bool) {
        let right_clicked = ctx.input(|i| i.pointer.secondary_clicked());
        //println!("[advance_frame] is_active: {}, active_last_frame: {}, right_clicked: {}",
        //is_active, self.active_last_frame, right_clicked);
        if (self.active_last_frame && !is_active) || (right_clicked && is_active) {
            println!("PathTool::advance_frame: clearing state");
            Self::clear_state(ctx);
            *self = Self::default();
            graph_overlay::clear_path_highlight(ctx);
            graph_overlay::clear_path_preview(ctx);
            self.ignore_selected = actions.selected_node().map(|n| n.id);
            actions.clear_node_selection();
            return;
        }
        
        self.active_last_frame = is_active;
        
        if !is_active {
            return;
        }
        
        if let Some(start) = self.start && self.end.is_none() {
            let cursor_pos = ctx.input(|i| i.pointer.latest_pos());
            if let Some(cursor_pos) = cursor_pos {
                const DASH_SPEED: f32 = 30.0;
                let delta_time = ctx.input(|i| i.stable_dt);
                self.dash_phase = (self.dash_phase + delta_time * DASH_SPEED) % (node_shape::DASH_LENGTH + node_shape::GAP_LENGTH);
                graph_overlay::set_path_preview(ctx, PathPreview::new(start, cursor_pos, self.dash_phase));
            } else {
                let _ = graph_overlay::clear_path_preview(ctx);
            }
            ctx.request_repaint();
        }
        
        if actions.selected_node().is_none() {
            self.ignore_selected = None;
        }
        
        if let Some(node) = actions.selected_node() {
            let selected_uuid = node.id;
            if self.ignore_selected == Some(selected_uuid) {
                return;
            } else {
                self.ignore_selected = None;
            }
            if Some(selected_uuid) != self.last_selected {
                println!("Selection changed from last frame");
                if self.start.is_none() {
                    println!("Start node not set, setting to {}", selected_uuid);
                    self.start = Some(selected_uuid);
                    self.ignore_selected = None;
                } else if self.end.is_none() {
                    println!("End node not set, setting to {}", selected_uuid);
                    self.end = Some(selected_uuid);
                    self.ignore_selected = None;
                    if let Some((_cost, path)) = actions.compute_path(
                        self.start.expect("Start node not set, shouldn't happen"), 
                        selected_uuid
                    ) {
                        graph_overlay::set_path_highlight(ctx, path.into_iter());
                        let _ = graph_overlay::clear_path_preview(ctx);
                        ctx.request_repaint();
                    }
                    
                }
                self.last_selected = Some(selected_uuid);
            }
        }
    }
}