use openaction::OpenActionResult;

mod audio;
mod gfx;
mod plugin;
mod utils;

#[tokio::main]
async fn main() -> OpenActionResult<()> {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        simplelog::Config::default(),
        simplelog::TerminalMode::Stdout,
        simplelog::ColorChoice::Never,
    )
    .unwrap();

    println!("Starting Volume Controller plugin...");

    plugin::init().await
}
