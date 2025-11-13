mod data_aquisition;
mod gui;
mod network;
mod parsers;
mod topology;

use std::sync::Arc;
use gui::app;

fn main() {
    let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
    app::main(rt);
}
