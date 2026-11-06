mod adapters;
mod data_acquisition;
mod gui;
mod network;
mod parsers;
mod topology; // new parsers

use gui::app;
use std::sync::Arc;

use crate::gui::new_app;

fn main() {
    let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
    new_app::main(rt);
}
