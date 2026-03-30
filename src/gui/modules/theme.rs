use crate::gui::new_app::{AppPanel, ThemeId};


pub struct ThemeSelect;

impl ThemeSelect {
    pub fn ui(ctx: &egui::Context, ui: &mut egui::Ui, actions: &mut dyn crate::gui::actions::AppActions) {
        egui::ComboBox::from_label("Theme")
            .selected_text(actions.theme().name())
            .show_ui(ui, |ui| {
                for theme in ThemeId::all() {
                    if ui.selectable_value(&mut actions.theme(), theme, theme.name()).clicked() {
                        actions.set_theme(theme);
                    }
                }
            });
    }
}