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
    pub app_name: String,
    pub sink_name: Option<String>,
    pub mute: bool,
    pub vol_percent: f32,
    pub icon_uri: String,
    pub icon_uri_mute: String,
    pub uses_default_icon: bool,
    pub is_device: bool,
    pub is_multi_sink_app: bool,
}

pub static MIXER_CHANNELS: LazyLock<Mutex<HashMap<u8, MixerChannel>>> =
    LazyLock::new(|| Mutex::const_new(HashMap::new()));

pub async fn create_mixer_channels(
    applications: Vec<crate::audio::audio_system::AppInfo>,
    ignored_apps: &[String],
) {
    let mut channels = MIXER_CHANNELS.lock().await;

    let mut col_key = 0;
    for app in applications.into_iter() {
        if ignored_apps.contains(&app.app_name) {
            println!("Skipping ignored app: {}", app.app_name);
            continue;
        }

        let (icon_uri, icon_uri_mute, uses_default_icon) =
            get_app_icon_uri(app.icon_name, app.app_name.clone());

        channels.insert(
            col_key as u8,
            MixerChannel {
                header_id: None,
                upper_vol_btn_id: None,
                lower_vol_btn_id: None,
                uid: app.uid,
                app_name: app.app_name.clone(),
                sink_name: app.sink_name.clone(),
                mute: app.mute,
                vol_percent: app.vol_percent,
                icon_uri,
                icon_uri_mute,
                uses_default_icon,
                is_device: app.is_device,
                is_multi_sink_app: app.is_multi_sink_app,
            },
        );

        col_key += 1;
    }
}

pub async fn update_mixer_channels(
    applications: Vec<crate::audio::audio_system::AppInfo>,
    ignored_apps: &[String],
) {
    let mut channels = MIXER_CHANNELS.lock().await;

    let mut col_key = 0;
    for app in applications {
        if ignored_apps.contains(&app.app_name) {
            println!("Skipping ignored app: {}", app.app_name);
            continue;
        }

        if let Some(channel) = channels.get_mut(&col_key) {
            // Check if we need to update the channel
            let needs_update = channel.uid != app.uid
                || channel.app_name != app.app_name
                || channel.sink_name != app.sink_name
                || channel.mute != app.mute
                || (channel.vol_percent - app.vol_percent).abs() > 0.01
                || channel.is_device != app.is_device
                || channel.is_multi_sink_app != app.is_multi_sink_app;

            if needs_update {
                if channel.uid != app.uid {
                    let (icon_uri, icon_uri_mute, uses_default_icon) =
                        get_app_icon_uri(app.icon_name, app.app_name.clone());
                    channel.icon_uri = icon_uri;
                    channel.icon_uri_mute = icon_uri_mute;
                    channel.uses_default_icon = uses_default_icon;
                }

                // Update the channel data
                channel.uid = app.uid;
                channel.app_name = app.app_name;
                channel.sink_name = app.sink_name;
                channel.mute = app.mute;
                channel.vol_percent = app.vol_percent;
                channel.is_device = app.is_device;
                channel.is_multi_sink_app = app.is_multi_sink_app;
            }
        } else {
            // Insert new channel if it doesn't exist
            let (icon_uri, icon_uri_mute, uses_default_icon) =
                get_app_icon_uri(app.icon_name, app.app_name.clone());

            channels.insert(
                col_key,
                MixerChannel {
                    header_id: None,
                    upper_vol_btn_id: None,
                    lower_vol_btn_id: None,
                    uid: app.uid,
                    app_name: app.app_name,
                    sink_name: app.sink_name,
                    mute: app.mute,
                    vol_percent: app.vol_percent,
                    icon_uri,
                    icon_uri_mute,
                    uses_default_icon,
                    is_device: app.is_device,
                    is_multi_sink_app: app.is_multi_sink_app,
                },
            );
        }

        col_key += 1;
    }

    // Remove channels that no longer have corresponding apps
    channels.retain(|&key, _| key < col_key);

    println!(
        "Updated mixer channels (filtered {} ignored apps)",
        ignored_apps.len()
    );
}
