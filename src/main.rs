mod data_aquisition;
mod parsers;
mod network;
mod gui;

use gui::app;
use eframe::egui;

fn main() {
    app::main();
}