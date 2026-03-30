use crate::gui::tools::{
    icons::{IconId, load_icon},
    tray::{Tool, ToolId},
};

pub struct SelectTool;

impl Tool for SelectTool {
    fn icon(&self, ctx: &egui::Context) -> egui::TextureId {
        let tint = ctx.theme().default_visuals().text_color();
        load_icon(ctx, IconId::Select, 64, Some(tint)).id()
    }

    fn name(&self) -> &str {
        "Select Tool"
    }

    fn description(&self) -> &str {
        "Click to select a node, drag to move it."
    }

    fn on_click(
        &self,
        _ctx: &egui::Context,
        _pos: egui::Pos2,
        _actions: &mut dyn crate::gui::actions::AppActions,
    ) {
        // Do nothing, it's the default.
    }
    
    fn id(&self) -> ToolId {
        ToolId::SelectTool
    }
}
