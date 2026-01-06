use openaction::*;
use serde::{Deserialize, Serialize};

use crate::{
    audio::{self, pulse::pulse_monitor::refresh_audio_applications, *},
    gfx::{self},
    mixer,
    utils::{self, ButtonPressControl},
};
use std::{collections::HashMap, sync::LazyLock};
use tokio::sync::Mutex;

const VOLUME_INCREMENT: f64 = 0.1;

pub static COLUMN_TO_CHANNEL_MAP: LazyLock<Mutex<HashMap<u8, u8>>> =
    LazyLock::new(|| Mutex::const_new(HashMap::new()));

pub static BUTTON_PRESS_CONTROL: LazyLock<Mutex<ButtonPressControl>> =
    LazyLock::new(|| Mutex::const_new(ButtonPressControl::new()));

pub static SHARED_SETTINGS: LazyLock<Mutex<VolumeControllerSettings>> =
    LazyLock::new(|| Mutex::const_new(VolumeControllerSettings::default()));

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct VolumeControllerSettings {
    pub show_sys_mixer: bool,
    pub ignored_apps_list: Vec<String>,
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
        instance: &Instance,
        settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        println!("did_receive_settings for instance {}: show_sys_mixer={}, ignored_apps={:?}",
            instance.instance_id, settings.show_sys_mixer, settings.ignored_apps_list);

        // Check if settings actually changed to avoid infinite loops
        let mut cached = SHARED_SETTINGS.lock().await;
        let settings_changed = cached.show_sys_mixer != settings.show_sys_mixer
            || cached.ignored_apps_list != settings.ignored_apps_list;

        if settings_changed {
            println!("Settings changed, broadcasting to all instances");
            *cached = settings.clone();
            drop(cached);

            // Save ignored apps to file
            if let Err(e) = utils::save_ignored_apps(&settings.ignored_apps_list) {
                println!("Warning: Failed to save ignored apps: {}", e);
            }

            // Broadcast settings to all instances so they all stay in sync
            for inst in visible_instances(Self::UUID).await {
                if inst.instance_id != instance.instance_id {
                    println!("Broadcasting to instance {}", inst.instance_id);
                    let _ = inst.set_settings(settings).await;
                }
            }

            // Apply show_sys_mixer setting
            utils::set_show_system_mixer(settings.show_sys_mixer);
            let _ = refresh_audio_applications().await;
        } else {
            drop(cached);
            println!("Settings unchanged, skipping broadcast");
        }

        Ok(())
    }

    async fn will_appear(&self, instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        // Sync with shared settings when appearing
        let shared = SHARED_SETTINGS.lock().await;
        let _ = instance.set_settings(&*shared).await;
        drop(shared);

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

    async fn key_up(
        &self,
        instance: &Instance,
        _settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        let mut press_control = BUTTON_PRESS_CONTROL.lock().await;

        // Validate this is the correct button press
        if let Some(action_id) = press_control.action_id.as_ref() {
            if action_id != &instance.instance_id {
                drop(press_control);
                return Ok(());
            }
        }

        if let Some(duration_ms) = press_control.get_release_time() {
            println!(
                "Button {} held for {} ms",
                instance.instance_id, duration_ms
            );
            drop(press_control);

            let coords = instance.coordinates.expect("coordinates must be present");
            let sd_column = coords.column;

            if duration_ms > 1000 && coords.row == 0 {
                let column_map = COLUMN_TO_CHANNEL_MAP.lock().await;
                let mut channels = mixer::MIXER_CHANNELS.lock().await;

                // Look up the channel index for this SD column
                let Some(&channel_index) = column_map.get(&sd_column) else {
                    return Ok(());
                };

                if let Some(channel) = channels.get_mut(&channel_index) {
                    let app_name = channel.app_name.clone();
                    let uid = channel.uid;
                    let is_device = channel.is_device;

                    channel.mute = false;

                    // Drop locks before potentially blocking operations
                    drop(channels);
                    drop(column_map);

                    {
                        let mut audio_system = audio::create();
                        audio_system
                            .mute_volume(uid, false, is_device)
                            .expect("Failed to unmute");
                    } // audio_system is dropped here

                    // Read cached shared settings, append app, and save back
                    let mut shared_settings = SHARED_SETTINGS.lock().await;
                    if !shared_settings.ignored_apps_list.contains(&app_name) {
                        shared_settings.ignored_apps_list.push(app_name.clone());
                    }
                    let updated_settings = shared_settings.clone();
                    *shared_settings = updated_settings.clone();
                    drop(shared_settings);

                    // Save ignored apps to file
                    if let Err(e) = utils::save_ignored_apps(&updated_settings.ignored_apps_list) {
                        println!("Warning: Failed to save ignored apps: {}", e);
                    }

                    // Broadcast to ALL instances (including this one)
                    for inst in visible_instances(Self::UUID).await {
                        let _ = inst.set_settings(&updated_settings).await;
                    }

                    println!("Added {} to ignored apps list and broadcast to all instances", app_name);
                }
            }
        }

        Ok(())
    }

    async fn key_down(&self, instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        let mut press_control = BUTTON_PRESS_CONTROL.lock().await;
        press_control.set_press_time(instance.instance_id.clone());
        drop(press_control); // Release lock early

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

                    println!("Muting app {}", channel.app_name);
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

                    println!(
                        "Volume up in app {} {}",
                        channel.app_name, channel.vol_percent
                    );
                }
                2 => {
                    let app_uid = channel.uid;
                    let mut audio_system = audio::create();
                    audio_system
                        .decrease_volume(app_uid, VOLUME_INCREMENT, channel.is_device)
                        .expect("Failed to decrease volume");

                    println!(
                        "Volume down in app {} {}",
                        channel.app_name, channel.vol_percent
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

    // Load ignored apps from file and populate shared settings
    let loaded_ignored_apps = utils::load_ignored_apps();
    {
        let mut settings = SHARED_SETTINGS.lock().await;
        settings.ignored_apps_list = loaded_ignored_apps.clone();
        println!("Initialized with {} ignored apps from file", loaded_ignored_apps.len());
    }

    // start listening to changes
    audio::pulse::start_pulse_monitoring();

    // create initial map
    let applications = {
        let mut audio_system = create();
        audio_system
            .list_applications()
            .expect("Error fetching applications from SinkController")
    };

    mixer::create_mixer_channels(applications, &loaded_ignored_apps).await;

    // Register action
    register_action(VolumeControllerAction).await;

    run(std::env::args().collect()).await
}
