use openaction::*;
use pulsectl::controllers::types::ApplicationInfo;
use pulsectl::controllers::{AppControl, SinkController};
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
struct VolumeApplicationColumn {
    header_context: String,
    volume_up_context: String,
    volume_down_context: String,
    app_index: u32,
    app_name: String,
    app_mute: bool,
}

#[derive(serde::Serialize)]
struct SwitchProfileEvent {
    event: &'static str,
    device: String,
    profile: String,
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

        let mut column = match columns.get_mut(&column_key) {
            Some(col) => col,
            None => return Ok(()),
        };

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
        event: KeyEvent,
        outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        Ok(())
    }

    async fn key_down(
        &self,
        event: KeyEvent,
        outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        println!("ACTION: {:?}", event.action);
        match &event.action[..] {
            "com.victormarin.volume-controller.back-to-profile" => {
                println!("I AM DEFINITELY HERE");
                clear_screen(outbound).await.unwrap();
                switch_profile(outbound, "Test".to_string()).await.unwrap();
                Ok(())
            }
            "com.victormarin.volume-controller.auto-detection" => {
                println!("I AM CONNECTED");
                create_application_volume_columns().await;
                switch_profile(outbound, "Sound".to_string()).await.unwrap();
                Ok(())
            }
            "com.victormarin.volume-controller.auto-detection.blank" => {
                let mut columns = VOLUME_APPLICATION_COLUMNS.lock().await;
                let column_key = event.payload.coordinates.column;

                if let Some(column) = columns.get_mut(&column_key) {
                    let mut controller = SinkController::create().unwrap();

                    match event.payload.coordinates.row {
                        0 => {
                            println!("Muting app {}", column.app_name);
                            column.app_mute = !column.app_mute;
                            controller
                                .set_app_mute(column.app_index, column.app_mute)
                                .unwrap();
                        }
                        1 => {
                            println!("Volume up app {}", column.app_name);
                            // Volume up
                            controller.increase_app_volume_by_percent(column.app_index, 0.05);
                        }
                        2 => {
                            println!("Volume down app {}", column.app_name);
                            // Volume down
                            controller.decrease_app_volume_by_percent(column.app_index, 0.05);
                        }
                        _ => {}
                    }
                }

                Ok(())
            }
            _ => Ok(()),
        }
    }
}

async fn switch_profile(
    outbound: &mut OutboundEventManager,
    profile: String,
) -> EventHandlerResult {
    outbound
        .send_event(SwitchProfileEvent {
            event: "switchProfile",
            device: "sd-DL08M2A38870".to_string(),
            profile,
        })
        .await?;

    println!("SENT SWITCH PROFILE EVENT");
    Ok(())
}

async fn clear_screen(outbound: &mut OutboundEventManager) -> EventHandlerResult {
    let mut columns = VOLUME_APPLICATION_COLUMNS.lock().await;

    println!("I AM CLEARING THIS SCREEN --- {:?}", columns);

    for (_, value) in columns.iter() {
        outbound
            .set_title(value.header_context.clone(), Some("".to_string()), Some(0))
            .await
            .expect("Error reseting header");
        outbound
            .set_title(
                value.volume_up_context.clone(),
                Some("".to_string()),
                Some(0),
            )
            .await
            .expect("Error reseting volume up");
        outbound
            .set_title(
                value.volume_down_context.clone(),
                Some("".to_string()),
                Some(0),
            )
            .await
            .expect("Error reseting volume down");
    }

    columns.clear();
    println!("DISOCNECTING");
    Ok(())
}

async fn create_application_volume_columns() {
    let mut columns = VOLUME_APPLICATION_COLUMNS.lock().await;

    let mut controller = SinkController::create().unwrap();
    let applications = controller
        .list_applications()
        .expect("Error fetching applications from SinkController");

    println!("THERE ARE {} APPSSSSS SOUND", applications.len());
    let mut col_key = 1;
    for app in applications {
        println!("DEBUG APP: {:?}", app);
        columns.insert(
            col_key,
            VolumeApplicationColumn {
                header_context: String::new(),
                volume_up_context: String::new(),
                volume_down_context: String::new(),
                app_index: app.index,
                app_name: get_application_name(&app),
                app_mute: app.mute,
            },
        );

        col_key += 1;
    }

    println!("I AM DONE COLUMNING: {:?}", columns);
}

fn get_application_name(app: &ApplicationInfo) -> String {
    // First, check if the main name field has a meaningful value
    if let Some(name) = &app.name {
        if !is_generic_name(name) {
            return name.clone();
        }
    }

    // Access proplist directly (it's not an Option)
    let proplist = &app.proplist;

    // Check application.name first (usually the best)
    if let Some(app_name) = proplist.get_str("application.name") {
        if !is_generic_name(&app_name) {
            return app_name;
        }
    }

    // Check application.process.binary (executable name)
    if let Some(binary) = proplist.get_str("application.process.binary") {
        if !is_generic_name(&binary) {
            return binary;
        }
    }

    // Check media.name (often has song/video titles)
    if let Some(media_name) = proplist.get_str("media.name") {
        if !is_generic_name(&media_name) {
            return format!("Media: {}", media_name);
        }
    }

    // Check application.icon_name (sometimes useful)
    if let Some(icon_name) = proplist.get_str("application.icon_name") {
        if !is_generic_name(&icon_name) {
            return icon_name;
        }
    }

    // Check for browser-specific properties
    if let Some(role) = proplist.get_str("media.role") {
        if role == "music" || role == "video" {
            // For browsers playing media, try to get more specific info
            if let Some(title) = proplist.get_str("media.title") {
                return format!("Browser: {}", title);
            }
            if let Some(artist) = proplist.get_str("media.artist") {
                return format!("Music: {}", artist);
            }
        }
    }

    // Absolute fallback
    app.name
        .as_deref()
        .unwrap_or("Unknown Application")
        .to_string()
}

fn is_generic_name(name: &str) -> bool {
    let generic_names = [
        "Playback",
        "playback",
        "ALSA",
        "PulseAudio",
        "output",
        "sink",
        "stream",
        "",
    ];

    generic_names.contains(&name) || name.trim().is_empty()
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
