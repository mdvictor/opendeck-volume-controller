use openaction::{Action, Instance, visible_instances};
use pulsectl::controllers::types::ApplicationInfo;
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::Mutex;

use crate::gfx::TRANSPARENT_ICON;
use crate::plugin::VCAction;

#[derive(Clone, Debug)]
pub struct VolumeApplicationColumn {
    pub header_id: Option<String>,
    pub upper_vol_btn_id: Option<String>,
    pub lower_vol_btn_id: Option<String>,
    pub uid: u32,
    pub name: String,
    pub mute: bool,
    pub vol_percent: f32,
}

// this should probably be a setting
// currently the volume controller will expect the SD volume apps
// to start on the second column, while the first one would be reserved
// for other actions (e.g.: the cancel btn that would take you back to
// your initial profile)
// but I reckon this is rather limiting? maybe you want all btns to be
// apps, but then you have no way to exit the controller through the SD?
// TODO
const STARTING_COL_KEY: u8 = 1;

pub static VOLUME_APPLICATION_COLUMNS: LazyLock<Mutex<HashMap<u8, VolumeApplicationColumn>>> =
    LazyLock::new(|| Mutex::const_new(HashMap::new()));

pub async fn create_application_volume_columns(applications: Vec<crate::audio::traits::AppInfo>) {
    let mut columns = VOLUME_APPLICATION_COLUMNS.lock().await;

    println!("THERE ARE {} SOUND APPS", applications.len());
    let mut col_key = STARTING_COL_KEY;
    for app in applications {
        println!("DEBUG APP: {:?}", app);
        columns.insert(
            col_key,
            VolumeApplicationColumn {
                header_id: None,
                upper_vol_btn_id: None,
                lower_vol_btn_id: None,
                uid: app.uid,
                name: app.name.clone(),
                mute: app.mute,
                vol_percent: app.volume_percentage,
            },
        );

        col_key += 1;
    }

    println!("I AM DONE COLUMNING: {:?}", columns);
}

pub async fn update_application_volume_columns(applications: Vec<crate::audio::traits::AppInfo>) {
    let mut columns = VOLUME_APPLICATION_COLUMNS.lock().await;

    // Update existing columns with new data
    let mut col_key = STARTING_COL_KEY;
    for app in applications {
        if let Some(column) = columns.get_mut(&col_key) {
            // Update the column data
            column.uid = app.uid;
            column.name = app.name.clone();
            column.mute = app.mute;
            column.vol_percent = app.volume_percentage;
        } else {
            // Insert new column if it doesn't exist
            columns.insert(
                col_key,
                VolumeApplicationColumn {
                    header_id: None,
                    upper_vol_btn_id: None,
                    lower_vol_btn_id: None,
                    uid: app.uid,
                    name: app.name.clone(),
                    mute: app.mute,
                    vol_percent: app.volume_percentage,
                },
            );
        }
        col_key += 1;
    }

    // Remove columns that no longer have corresponding apps
    columns.retain(|&key, _| key < col_key);

    println!(
        "Updated application volume controllers model: {:?}",
        columns
    );
}

pub async fn update_stream_deck_buttons() {
    let mut columns = VOLUME_APPLICATION_COLUMNS.lock().await;

    for instance in visible_instances(VCAction::UUID).await {
        let Some(column) = columns.get_mut(&instance.coordinates.column) else {
            //TODO switch case for type of device eventually
            cleanup_sd3x5_column(&instance).await;
            continue;
        };

        match instance.coordinates.row {
            0 => column.header_id = Some(instance.instance_id.clone()),
            1 => column.upper_vol_btn_id = Some(instance.instance_id.clone()),
            2 => column.lower_vol_btn_id = Some(instance.instance_id.clone()),
            _ => {}
        }

        //TODO switch case for type of device eventually
        update_sd3x5_btns(column, &instance).await;
    }
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

async fn update_sd3x5_btns(column: &VolumeApplicationColumn, instance: &Instance) {
    match instance.coordinates.row {
        0 => {
            // Update header with app name
            let _ = instance.set_title(Some(column.name.clone()), None).await;
        }
        1 | 2 => {
            // Update volume buttons with bar graphics
            if let Ok((upper_img, lower_img)) =
                crate::gfx::get_volume_bar_data_uri_split(column.vol_percent)
            {
                if instance.coordinates.row == 1 {
                    let _ = instance.set_image(Some(upper_img), None).await;
                } else {
                    let _ = instance.set_image(Some(lower_img), None).await;
                }
            }
        }
        _ => {}
    }
}

async fn cleanup_sd3x5_column(instance: &Instance) {
    let _ = instance.set_title(Some(""), None).await;
    let _ = instance
        .set_image(Some(TRANSPARENT_ICON.as_str()), None)
        .await;
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
