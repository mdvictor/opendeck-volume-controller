use openaction::{EventHandlerResult, OutboundEventManager};
use pulsectl::controllers::types::ApplicationInfo;
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct VolumeApplicationColumn {
    pub header_context: String,
    pub volume_up_context: String,
    pub volume_down_context: String,
    pub app_uid: u32,
    pub app_name: String,
    pub app_mute: bool,
    pub volume_percentage: f32,
}

pub static VOLUME_APPLICATION_COLUMNS: LazyLock<Mutex<HashMap<u8, VolumeApplicationColumn>>> =
    LazyLock::new(|| Mutex::const_new(HashMap::new()));

pub async fn clear_screen(outbound: &mut OutboundEventManager) -> EventHandlerResult {
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

pub async fn create_application_volume_columns(applications: Vec<crate::audio::traits::AppInfo>) {
    let mut columns = VOLUME_APPLICATION_COLUMNS.lock().await;

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
                app_uid: app.uid,
                app_name: app.name.clone(),
                app_mute: app.mute,
                volume_percentage: app.volume_percentage,
            },
        );

        col_key += 1;
    }

    println!("I AM DONE COLUMNING: {:?}", columns);
}

pub fn get_application_name(app: &ApplicationInfo) -> String {
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
