use egui::{Area, Button, Frame, Image, ImageButton, Layout, Margin, Order, Pos2, Sense, Stroke, Vec2};

use crate::gui::{actions::AppActions, new_app::AppPanel};

const BUTTON_SIZE: f32 = 36.0;
const SPACING: f32 = 6.0;
const ICON_SIZE: f32 = 28.0;

#[derive(Default)]
pub struct ToolsTray {
    pub pos: Pos2,
    pub tools: Vec<Box<dyn Tool>>,
    pub selected_tool: Option<ToolId>
}

impl ToolsTray {
    pub fn new(pos: Pos2, tools: Vec<Box<dyn Tool>>) -> Self {
        Self { pos, tools, selected_tool: None }
    }

    pub fn ui(&mut self, ctx: &egui::Context, actions: &mut dyn AppActions) {
        let tool_count = self.tools.len().max(1) as f32;
        let width = BUTTON_SIZE + SPACING * 2.0;
        let height = tool_count * BUTTON_SIZE + (tool_count + 1.0) * SPACING;
        let desired = Vec2::new(width, height);

        Area::new("tools_tray".into())
            .order(Order::Foreground)
            .fixed_pos(self.pos)
            .show(ctx, |ui| {
                let (rect, response) = ui.allocate_exact_size(desired, Sense::drag());
                if response.dragged() {
                    self.pos += response.drag_delta();
                }

                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    Frame::group(ui.style())
                        .stroke(Stroke::NONE)
                        .fill(ctx.theme().default_visuals().faint_bg_color)
                        .corner_radius(4.0)
                        .inner_margin(Margin::symmetric(0, SPACING as i8))
                        .show(ui, |ui| {
                        ui.set_width(BUTTON_SIZE + SPACING * 2.0);
                        ui.spacing_mut().item_spacing = egui::vec2(SPACING, SPACING);
                    
                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            let mut tools = std::mem::take(&mut self.tools);
                            for tool in tools.iter_mut() {
                                self.render_tool_button(ui, tool.as_ref(), actions, self.selected_tool == Some(tool.id()));
                            }
                            self.tools = tools;
                        });
                    });

                });
            });
    }
}

impl ToolsTray {
    fn render_tool_button(&mut self, ui: &mut egui::Ui, tool: &dyn Tool, actions: &mut dyn AppActions, selected: bool) {
        let icon_size = Vec2::splat(ICON_SIZE);
        let image = Image::new((tool.icon(ui.ctx()), icon_size));
        let response = ui
            .add_sized([BUTTON_SIZE, BUTTON_SIZE], 
                Button::image(image)
                    .selected(selected)
            )
            .on_hover_ui(|ui| {
                ui.label(egui::RichText::new(tool.name()).heading());
                ui.add_space(4.0);
                ui.label(tool.description());
            });

        if response.clicked() {
            tool.on_click(ui.ctx(), response.rect.center(), actions);
            if self.selected_tool == Some(tool.id()) {
                self.selected_tool = Some(ToolId::SelectTool);
            } else {
                self.selected_tool = Some(tool.id());
            }
            println!("Tool selected: {:?}", self.selected_tool);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolId {
    SelectTool,
    PathTool
}



pub trait Tool {
    fn icon(&self, ctx: &egui::Context) -> egui::TextureId;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn on_click(&self, ctx: &egui::Context, pos: Pos2, actions: &mut dyn AppActions);
    fn id(&self) -> ToolId;
}
