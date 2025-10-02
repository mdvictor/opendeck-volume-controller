use crate::utils::get_app_icon_uri;
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct MixerChannel {
    pub header_id: Option<String>,
    pub upper_vol_btn_id: Option<String>,
    pub lower_vol_btn_id: Option<String>,
    pub uid: u32,
    pub name: String,
    pub mute: bool,
    pub vol_percent: f32,
    pub icon_uri: String,
    pub icon_uri_mute: String,
    pub uses_default_icon: bool,
    pub is_device: bool,
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

pub static MIXER_CHANNELS: LazyLock<Mutex<HashMap<u8, MixerChannel>>> =
    LazyLock::new(|| Mutex::const_new(HashMap::new()));

pub async fn create_mixer_channels(applications: Vec<crate::audio::audio_system::AppInfo>) {
    let mut channels = MIXER_CHANNELS.lock().await;

    let mut col_key = STARTING_COL_KEY;
    for app in applications {
        let (icon_uri, icon_uri_mute, uses_default_icon) =
            get_app_icon_uri(app.icon_name, app.name.clone());

        channels.insert(
            col_key,
            MixerChannel {
                header_id: None,
                upper_vol_btn_id: None,
                lower_vol_btn_id: None,
                uid: app.uid,
                name: app.name.clone(),
                mute: app.mute,
                vol_percent: app.volume_percentage,
                icon_uri,
                icon_uri_mute,
                uses_default_icon,
                is_device: app.is_device,
            },
        );

        col_key += 1;
    }
}

pub async fn update_mixer_channels(applications: Vec<crate::audio::audio_system::AppInfo>) {
    let mut channels = MIXER_CHANNELS.lock().await;

    let mut col_key = STARTING_COL_KEY;
    for app in applications {
        let (icon_uri, icon_uri_mute, uses_default_icon) =
            get_app_icon_uri(app.icon_name, app.name.clone());
        if let Some(channel) = channels.get_mut(&col_key) {
            // Update the channel data
            channel.uid = app.uid;
            channel.name = app.name.clone();
            channel.mute = app.mute;
            channel.vol_percent = app.volume_percentage;
            channel.icon_uri = icon_uri;
            channel.icon_uri_mute = icon_uri_mute;
            channel.uses_default_icon = uses_default_icon;
            channel.is_device = app.is_device;
        } else {
            // Insert new channel if it doesn't exist
            channels.insert(
                col_key,
                MixerChannel {
                    header_id: None,
                    upper_vol_btn_id: None,
                    lower_vol_btn_id: None,
                    uid: app.uid,
                    name: app.name.clone(),
                    mute: app.mute,
                    vol_percent: app.volume_percentage,
                    icon_uri,
                    icon_uri_mute,
                    uses_default_icon,
                    is_device: app.is_device,
                },
            );
        }

        col_key += 1;
    }

    // Remove channels that no longer have corresponding apps
    channels.retain(|&key, _| key < col_key);

    println!("Updated mixer channels");
}
