mod data_acquisition;
mod gui;
mod network;
mod parsers;
mod topology;
mod adapters; // new parsers

use std::sync::Arc;
use gui::app;

use crate::gui::new_app;

fn main() {
    let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
    new_app::main(rt);
}
