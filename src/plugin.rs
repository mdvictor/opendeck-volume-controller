use openaction::*;
use serde::{Deserialize, Serialize};

use crate::{
    audio::{self, pulse::pulse_monitor::refresh_audio_applications, *},
    gfx::{self},
    mixer,
    utils::{self},
};
use std::{collections::HashMap, sync::LazyLock};
use tokio::sync::Mutex;

const VOLUME_INCREMENT: f64 = 0.1;

pub static COLUMN_TO_CHANNEL_MAP: LazyLock<Mutex<HashMap<u8, u8>>> =
    LazyLock::new(|| Mutex::const_new(HashMap::new()));

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct VolumeControllerSettings {
    pub show_sys_mixer: bool,
}

pub struct VolumeControllerAction;
#[async_trait]
impl Action for VolumeControllerAction {
    const UUID: ActionUuid = "com.victormarin.volume-controller.volctrl";
    type Settings = VolumeControllerSettings;

    async fn will_disappear(
        &self,
        instance: &Instance,
        _: &Self::Settings,
    ) -> OpenActionResult<()> {
        utils::cleanup_sd_column(instance).await;

        let mut column_map = COLUMN_TO_CHANNEL_MAP.lock().await;
        column_map.remove(
            &instance
                .coordinates
                .expect("coordinates must be present")
                .column,
        );

        Ok(())
    }

    async fn did_receive_settings(
        &self,
        _instance: &Instance,
        settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        utils::set_show_system_mixer(settings.show_sys_mixer);
        let _ = refresh_audio_applications().await;
        Ok(())
    }

    async fn will_appear(&self, instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        let mut column_map = COLUMN_TO_CHANNEL_MAP.lock().await;
        let mut channels = mixer::MIXER_CHANNELS.lock().await;
        let coords = instance.coordinates.expect("coordinates must be present");

        let sd_column = coords.column;

        // Calculate next index before entry() call to avoid borrow checker issue
        let next_index = column_map.len() as u8;
        let channel_index = *column_map.entry(sd_column).or_insert(next_index);

        let channel = match channels.get_mut(&channel_index) {
            Some(ch) => ch,
            None => {
                utils::cleanup_sd_column(instance).await;
                return Ok(());
            }
        };

        match coords.row {
            0 => {
                utils::update_header(instance, channel).await;
                channel.header_id = Some(instance.instance_id.clone());
            }
            1 | 2 => {
                if let Ok((upper_img, lower_img)) =
                    gfx::get_volume_bar_data_uri_split(channel.vol_percent)
                {
                    let img;
                    if coords.row == 1 {
                        channel.upper_vol_btn_id = Some(instance.instance_id.clone());
                        img = upper_img;
                    } else {
                        channel.lower_vol_btn_id = Some(instance.instance_id.clone());
                        img = lower_img;
                    };
                    instance.set_image(Some(img), None).await?;
                }
            }
            _ => {} // Ignore other rows
        }

        Ok(())
    }

    async fn key_down(&self, instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        let column_map = COLUMN_TO_CHANNEL_MAP.lock().await;
        let mut channels = mixer::MIXER_CHANNELS.lock().await;
        let coords = instance.coordinates.expect("coordinates must be present");

        let sd_column = coords.column;

        // Look up the channel index for this SD column
        let Some(&channel_index) = column_map.get(&sd_column) else {
            return Ok(());
        };

        if let Some(channel) = channels.get_mut(&channel_index) {
            match coords.row {
                0 => {
                    channel.mute = !channel.mute;
                    let mut audio_system = audio::create();
                    audio_system
                        .mute_volume(channel.uid, channel.mute, channel.is_device)
                        .expect("Failed to mute");

                    println!("Muting app {}", channel.name);
                }
                1 => {
                    let app_uid = channel.uid;

                    if channel.vol_percent >= 100.0 {
                        return Ok(());
                    }

                    let mut audio_system = audio::create();
                    audio_system
                        .increase_volume(app_uid, VOLUME_INCREMENT, channel.is_device)
                        .expect("Failed to increase volume");

                    println!("Volume up in app {} {}", channel.name, channel.vol_percent);
                }
                2 => {
                    let app_uid = channel.uid;
                    let mut audio_system = audio::create();
                    audio_system
                        .decrease_volume(app_uid, VOLUME_INCREMENT, channel.is_device)
                        .expect("Failed to decrease volume");

                    println!(
                        "Volume down in app {} {}",
                        channel.name, channel.vol_percent
                    );
                }
                _ => {}
            }
        }

        Ok(())
    }
}

pub async fn init() -> OpenActionResult<()> {
    println!("Stream Deck connected - starting PulseAudio monitoring");
    // start listening to changes
    audio::pulse::start_pulse_monitoring();

    // create initial map
    let applications = {
        let mut audio_system = create();
        audio_system
            .list_applications()
            .expect("Error fetching applications from SinkController")
    };
    mixer::create_mixer_channels(applications).await;

    register_action(VolumeControllerAction).await;

    run(std::env::args().collect()).await
}
