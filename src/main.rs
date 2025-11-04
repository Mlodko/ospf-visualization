mod data_aquisition;
mod parsers;
mod network;
mod gui;

use std::sync::Arc;

use gui::app;
use eframe::egui;

fn main() {
    let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
    app::main(rt);
}