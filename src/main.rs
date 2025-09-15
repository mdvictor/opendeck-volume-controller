use openaction::*;
use pulsectl::controllers::{AppControl, SinkController};
use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;

#[derive(Clone)]
struct VolumeApplicationColumn {
    header_context: String,
    volume_up_context: String,
    volume_down_context: String,
    app_index: u32,
    app_name: String,
    is_app: bool,
}

static VOLUME_APPLICATION_COLUMNS: LazyLock<Mutex<HashMap<u8, VolumeApplicationColumn>>> =
    LazyLock::new(|| Mutex::const_new(HashMap::new()));

struct GlobalEventHandler {}
impl openaction::GlobalEventHandler for GlobalEventHandler {}

struct ActionEventHandler {}

impl openaction::ActionEventHandler for ActionEventHandler {
    async fn will_appear(
        &self,
        event: AppearEvent,
        outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        let mut columns = VOLUME_APPLICATION_COLUMNS.lock().await;
        let column_key = event.payload.coordinates.column;

        // Skip column 0 as it's reserved
        if column_key == 0 {
            return Ok(());
        }

        // Get or create the column - if it doesn't exist, create a default one
        let column = columns.entry(column_key).or_insert_with(|| {
            // Fetch applications and get the one at this column position
            let mut controller = SinkController::create().unwrap();
            let applications = controller
                .list_applications()
                .expect("Error fetching applications from SinkController");

            let app_index = (column_key - 1) as usize;

            if let Some(app) = applications.get(app_index) {
                VolumeApplicationColumn {
                    header_context: String::new(),
                    volume_up_context: String::new(),
                    volume_down_context: String::new(),
                    app_index: app.index,
                    is_app: true,
                    app_name: app
                        .name
                        .as_ref()
                        .unwrap_or(&"Unknown App".to_string())
                        .clone(),
                }
            } else {
                // No app at this index
                VolumeApplicationColumn {
                    header_context: String::new(),
                    volume_up_context: String::new(),
                    volume_down_context: String::new(),
                    app_index: 0,
                    is_app: false,
                    app_name: "No App".to_string(),
                }
            }
        });

        if !column.is_app {
            return Ok(());
        }

        // Set the context based on the row
        match event.payload.coordinates.row {
            0 => {
                column.header_context = event.context.clone();
                outbound
                    .set_title(
                        column.header_context.clone(),
                        Some(column.app_name.clone()),
                        Some(0),
                    )
                    .await?;
            }
            1 => {
                column.volume_up_context = event.context.clone();
                outbound
                    .set_title(
                        column.volume_up_context.clone(),
                        Some("+".to_string()),
                        Some(0),
                    )
                    .await?;
            }
            2 => {
                column.volume_down_context = event.context.clone();
                outbound
                    .set_title(
                        column.volume_down_context.clone(),
                        Some("-".to_string()),
                        Some(0),
                    )
                    .await?;
            }
            _ => {} // Ignore other rows
        }

        println!("EEEEV WIL APP {:?}", event);

        Ok(())
    }

    async fn key_up(
        &self,
        _event: KeyEvent,
        _outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        Ok(())
    }

    async fn key_down(
        &self,
        _event: KeyEvent,
        _outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        Ok(())
    }
}

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
