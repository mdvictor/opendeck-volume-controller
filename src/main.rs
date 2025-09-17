use openaction::*;

mod audio;
mod event_handlers;
mod pulse_monitor;
mod switch_profile;
mod utils;

use event_handlers::{ActionEventHandler, GlobalEventHandler};

#[tokio::main]
async fn main() {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        simplelog::Config::default(),
        simplelog::TerminalMode::Stdout,
        simplelog::ColorChoice::Never,
    )
    .unwrap();

    println!("Starting Volume Controller plugin...");

    if let Err(error) = init_plugin(GlobalEventHandler {}, ActionEventHandler {}).await {
        log::error!("Failed to init plugin: {}", error);
    }
}
