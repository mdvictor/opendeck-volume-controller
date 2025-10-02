use std::collections::HashMap;

use openaction::*;

use crate::{
    audio::{self, *},
    gfx::{self, TRANSPARENT_ICON},
    mixer,
    utils::update_header,
};

// this could be a plugin setting
const VOLUME_INCREMENT: f32 = 0.1;

pub struct VolumeControllerAction;
#[async_trait]
impl Action for VolumeControllerAction {
    const UUID: ActionUuid = "com.victormarin.volume-controller.auto-detection.blank";
    type Settings = HashMap<String, String>;

    async fn will_disappear(
        &self,
        instance: &Instance,
        _: &Self::Settings,
    ) -> OpenActionResult<()> {
        let _ = instance.set_title(Some(""), None);
        let _ = instance.set_image(Some(TRANSPARENT_ICON.as_str()), None);

        Ok(())
    }

    async fn will_appear(&self, instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        let mut channels = mixer::MIXER_CHANNELS.lock().await;
        let column_key = instance.coordinates.column;

        // Skip column 0 as it's reserved TODO make this a setting?
        if column_key == 0 {
            return Ok(());
        }

        let channel = match channels.get_mut(&column_key) {
            Some(ch) => ch,
            None => return Ok(()),
        };

        match instance.coordinates.row {
            0 => {
                update_header(instance, channel).await;
                channel.header_id = Some(instance.instance_id.clone());
            }
            1 | 2 => {
                if let Ok((upper_img, lower_img)) =
                    gfx::get_volume_bar_data_uri_split(channel.vol_percent)
                {
                    let img;
                    if instance.coordinates.row == 1 {
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
        let mut channels = mixer::MIXER_CHANNELS.lock().await;
        let column_key = instance.coordinates.column;

        if let Some(channel) = channels.get_mut(&column_key) {
            match instance.coordinates.row {
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
                        .increase_volume(app_uid, VOLUME_INCREMENT as f64, channel.is_device)
                        .expect("Failed to increase volume");

                    println!("Volume up in app {} {}", channel.name, channel.vol_percent);
                }
                2 => {
                    let app_uid = channel.uid;
                    let mut audio_system = audio::create();
                    audio_system
                        .decrease_volume(app_uid, VOLUME_INCREMENT as f64, channel.is_device)
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
