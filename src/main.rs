mod data_aquisition;
mod gui;
mod network;
mod parsers;
mod topology;

use std::sync::Arc;
use gui::app;

use crate::gui::new_app;

fn main() {
    let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
    new_app::main(rt);
}
