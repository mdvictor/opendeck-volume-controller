use openaction::*;
use openaction::global_events::{GlobalEventHandler, DidReceiveGlobalSettingsEvent, set_global_event_handler};

use serde::{Deserialize, Serialize};

use crate::{
    audio::{self, pulse::pulse_monitor::refresh_audio_applications, *},
    gfx::{self},
    mixer,
    utils::{self, ButtonPressControl, ControllerKind},
};
use std::{collections::HashMap, sync::LazyLock};
use tokio::sync::Mutex;

/// Default volume adjustment applied per key press / dial tick, in
/// percentage points, until the user configures a different `volume_step`.
fn default_volume_step() -> f32 {
    5.0
}

pub static COLUMN_TO_CHANNEL_MAP: LazyLock<Mutex<HashMap<(ControllerKind, u8), u8>>> =
    LazyLock::new(|| Mutex::const_new(HashMap::new()));

pub static BUTTON_PRESS_CONTROL: LazyLock<Mutex<ButtonPressControl>> =
    LazyLock::new(|| Mutex::const_new(ButtonPressControl::new()));

pub static SHARED_SETTINGS: LazyLock<Mutex<VolumeControllerSettings>> =
    LazyLock::new(|| Mutex::const_new(VolumeControllerSettings::default()));

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct VolumeControllerSettings {
    pub show_sys_mixer: bool,
    pub ignored_apps_list: Vec<String>,
    /// Volume adjustment applied per key press / dial tick, in percentage points.
    pub volume_step: f32,
}

impl Default for VolumeControllerSettings {
    fn default() -> Self {
        Self {
            show_sys_mixer: false,
            ignored_apps_list: Vec::new(),
            volume_step: default_volume_step(),
        }
    }
}

impl VolumeControllerSettings {
    /// `volume_step`, guarding against 0/negative/garbage values that could
    /// come from the property inspector or a stale settings payload.
    fn volume_step_or_default(&self) -> f32 {
        if self.volume_step > 0.0 {
            self.volume_step
        } else {
            default_volume_step()
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct GlobalPluginSettings {
    pub ignored_apps_list: Vec<String>,
}

pub struct GlobalHandler;

#[async_trait]
impl GlobalEventHandler for GlobalHandler {
    async fn plugin_ready(&self) -> OpenActionResult<()> {
        // Request global settings on startup so ignored_apps_list is loaded
        let _ = get_global_settings().await;
        Ok(())
    }

    async fn did_receive_global_settings(&self, event: DidReceiveGlobalSettingsEvent) -> OpenActionResult<()> {
        let global: GlobalPluginSettings = serde_json::from_value(event.payload.settings)
            .unwrap_or_default();

        println!("did_receive_global_settings: {} ignored apps", global.ignored_apps_list.len());

        let mut shared = SHARED_SETTINGS.lock().await;
        if shared.ignored_apps_list != global.ignored_apps_list {
            shared.ignored_apps_list = global.ignored_apps_list.clone();
            drop(shared);

            // Sync ignored_apps_list into all instance settings
            let current = SHARED_SETTINGS.lock().await.clone();
            for inst in visible_instances(VolumeControllerAction::UUID).await {
                let _ = inst.set_settings(&current).await;
            }

            let _ = refresh_audio_applications().await;
        }

        Ok(())
    }
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

        let Some(coords) = instance.coordinates else {
            println!("Warning: Instance {} has no coordinates", instance.instance_id);
            return Ok(());
        };

        let mut column_map = COLUMN_TO_CHANNEL_MAP.lock().await;
        column_map.remove(&(ControllerKind::of(instance), coords.column));

        Ok(())
    }

    async fn did_receive_settings(
        &self,
        instance: &Instance,
        settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        let volume_step = settings.volume_step_or_default();

        println!(
            "did_receive_settings for instance {}: show_sys_mixer={} volume_step={}",
            instance.instance_id, settings.show_sys_mixer, volume_step
        );

        // Check if any shared setting changed to avoid infinite broadcast loops
        let mut cached = SHARED_SETTINGS.lock().await;
        let settings_changed =
            cached.show_sys_mixer != settings.show_sys_mixer || cached.volume_step != volume_step;

        if settings_changed {
            println!("Settings changed, broadcasting to all instances");
            cached.show_sys_mixer = settings.show_sys_mixer;
            cached.volume_step = volume_step;
            let normalized = cached.clone();
            drop(cached);

            // Broadcast the shared settings to every instance (including this
            // one, in case volume_step was clamped above)
            for inst in visible_instances(Self::UUID).await {
                let _ = inst.set_settings(&normalized).await;
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

        let Some(coords) = instance.coordinates else {
            println!("Warning: Instance {} has no coordinates", instance.instance_id);
            return Ok(());
        };

        let controller = ControllerKind::of(instance);

        let mut column_map = COLUMN_TO_CHANNEL_MAP.lock().await;
        let mut channels = mixer::MIXER_CHANNELS.lock().await;

        let key = (controller, coords.column);

        // Calculate next index before entry() call to avoid borrow checker issue
        let next_index = column_map.len() as u8;
        let channel_index = *column_map.entry(key).or_insert(next_index);

        let channel = match channels.get_mut(&channel_index) {
            Some(ch) => ch,
            None => {
                utils::cleanup_sd_column(instance).await;
                return Ok(());
            }
        };

        match controller {
            ControllerKind::Encoder => {
                channel.encoder_id = Some(instance.instance_id.clone());
                utils::update_encoder_feedback(channel, instance).await;
            }
            ControllerKind::Keypad => match coords.row {
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
            },
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

            let Some(coords) = instance.coordinates else {
                println!("Warning: Instance {} has no coordinates", instance.instance_id);
                return Ok(());
            };

            if duration_ms > 1000 && coords.row == 0 {
                let column_map = COLUMN_TO_CHANNEL_MAP.lock().await;
                let channel_index = column_map.get(&(ControllerKind::Keypad, coords.column)).copied();
                drop(column_map);

                if let Some(channel_index) = channel_index {
                    ignore_current_app(channel_index).await;
                }
            }
        }

        Ok(())
    }

    async fn key_down(&self, instance: &Instance, settings: &Self::Settings) -> OpenActionResult<()> {
        let mut press_control = BUTTON_PRESS_CONTROL.lock().await;
        press_control.set_press_time(instance.instance_id.clone());
        drop(press_control); // Release lock early

        let Some(coords) = instance.coordinates else {
            println!("Warning: Instance {} has no coordinates", instance.instance_id);
            return Ok(());
        };

        let step = settings.volume_step_or_default();

        match coords.row {
            0 => toggle_mute_for_column(ControllerKind::Keypad, coords.column).await,
            1 => adjust_volume_for_column(ControllerKind::Keypad, coords.column, step).await,
            2 => adjust_volume_for_column(ControllerKind::Keypad, coords.column, -step).await,
            _ => {}
        }

        Ok(())
    }

    async fn dial_rotate(
        &self,
        instance: &Instance,
        settings: &Self::Settings,
        ticks: i16,
        _pressed: bool,
    ) -> OpenActionResult<()> {
        if ticks == 0 {
            return Ok(());
        }

        let Some(coords) = instance.coordinates else {
            println!("Warning: Instance {} has no coordinates", instance.instance_id);
            return Ok(());
        };

        let step = settings.volume_step_or_default();
        let delta = step * ticks as f32;
        adjust_volume_for_column(ControllerKind::Encoder, coords.column, delta).await;

        Ok(())
    }

    async fn dial_down(&self, instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        let Some(coords) = instance.coordinates else {
            println!("Warning: Instance {} has no coordinates", instance.instance_id);
            return Ok(());
        };

        toggle_mute_for_column(ControllerKind::Encoder, coords.column).await;

        Ok(())
    }

    async fn touch_tap(
        &self,
        instance: &Instance,
        _settings: &Self::Settings,
        _position: (u16, u16),
        hold: bool,
    ) -> OpenActionResult<()> {
        let Some(coords) = instance.coordinates else {
            println!("Warning: Instance {} has no coordinates", instance.instance_id);
            return Ok(());
        };

        if hold {
            // Long touch: add the app to the ignored list, mirroring the Keypad long-press gesture
            let column_map = COLUMN_TO_CHANNEL_MAP.lock().await;
            let channel_index = column_map.get(&(ControllerKind::Encoder, coords.column)).copied();
            drop(column_map);

            if let Some(channel_index) = channel_index {
                ignore_current_app(channel_index).await;
            }
        } else {
            // Short tap: toggle mute, same as pressing the dial
            toggle_mute_for_column(ControllerKind::Encoder, coords.column).await;
        }

        Ok(())
    }
}

/// Apply the same absolute volume to every stream in a group, so all members
/// of a multi-stream app stay in sync instead of drifting apart.
async fn apply_group_volume(member_uids: &[u32], target_percent: f32, is_device: bool, app_name: &str) {
    let mut audio_system = audio::create();
    for &uid in member_uids {
        if let Err(e) = audio_system.set_volume(uid, target_percent, is_device) {
            println!("Warning: Failed to set volume for {}: {}", app_name, e);
        }
    }
}

/// Apply the same mute state to every stream in a group at once.
async fn apply_group_mute(member_uids: &[u32], mute: bool, is_device: bool, app_name: &str) {
    let mut audio_system = audio::create();
    for &uid in member_uids {
        if let Err(e) = audio_system.mute_volume(uid, mute, is_device) {
            println!("Warning: Failed to toggle mute for {}: {}", app_name, e);
        }
    }
}

/// Look up the channel bound to `column` on the given controller, adjust its
/// volume by `delta` percentage points (clamped to 0-100), and push that to
/// every stream in the group.
async fn adjust_volume_for_column(controller: ControllerKind, column: u8, delta: f32) {
    let column_map = COLUMN_TO_CHANNEL_MAP.lock().await;
    let channel_index = column_map.get(&(controller, column)).copied();
    drop(column_map);

    let Some(channel_index) = channel_index else {
        return;
    };

    let mut channels = mixer::MIXER_CHANNELS.lock().await;
    let Some(channel) = channels.get_mut(&channel_index) else {
        return;
    };

    let target = (channel.vol_percent + delta).clamp(0.0, 100.0);
    let member_uids = channel.member_uids.clone();
    let is_device = channel.is_device;
    let app_name = channel.app_name.clone();
    drop(channels);

    apply_group_volume(&member_uids, target, is_device, &app_name).await;
    println!("Volume for {} -> {:.0}%", app_name, target);
}

/// Look up the channel bound to `column` on the given controller, flip its
/// mute state, and push that to every stream in the group at once.
async fn toggle_mute_for_column(controller: ControllerKind, column: u8) {
    let column_map = COLUMN_TO_CHANNEL_MAP.lock().await;
    let channel_index = column_map.get(&(controller, column)).copied();
    drop(column_map);

    let Some(channel_index) = channel_index else {
        return;
    };

    let mut channels = mixer::MIXER_CHANNELS.lock().await;
    let Some(channel) = channels.get_mut(&channel_index) else {
        return;
    };

    channel.mute = !channel.mute;
    let member_uids = channel.member_uids.clone();
    let is_device = channel.is_device;
    let mute = channel.mute;
    let app_name = channel.app_name.clone();
    drop(channels);

    apply_group_mute(&member_uids, mute, is_device, &app_name).await;
    println!("Muting app {} = {}", app_name, mute);
}

/// Unmute and add the app in `channel_index` to the ignored apps list, then
/// broadcast the updated list to global settings and every visible instance.
/// Shared by the Keypad long-press and the Encoder long-touch gestures.
async fn ignore_current_app(channel_index: u8) {
    let mut channels = mixer::MIXER_CHANNELS.lock().await;
    let Some(channel) = channels.get_mut(&channel_index) else {
        return;
    };

    let app_name = channel.app_name.clone();
    let member_uids = channel.member_uids.clone();
    let is_device = channel.is_device;

    channel.mute = false;
    drop(channels);

    apply_group_mute(&member_uids, false, is_device, &app_name).await;

    // Read cached shared settings, append app, and save back
    let updated_settings = {
        let mut shared_settings = SHARED_SETTINGS.lock().await;
        if !shared_settings.ignored_apps_list.contains(&app_name) {
            shared_settings.ignored_apps_list.push(app_name.clone());
        }
        shared_settings.clone()
    };

    // Save ignored apps to global settings
    let global = GlobalPluginSettings {
        ignored_apps_list: updated_settings.ignored_apps_list.clone(),
    };
    let _ = set_global_settings(global).await;

    // Broadcast to ALL instances (including this one)
    for inst in visible_instances(VolumeControllerAction::UUID).await {
        let _ = inst.set_settings(&updated_settings).await;
    }

    println!("Added {} to ignored apps list and broadcast to all instances", app_name);
}

pub async fn init() -> OpenActionResult<()> {
    println!("Stream Deck connected - starting PulseAudio monitoring");

    // start listening to changes
    audio::pulse::start_pulse_monitoring();

    // create initial map (ignored apps will be loaded via did_receive_global_settings)
    let applications = {
        let mut audio_system = create();
        audio_system
            .list_applications()
            .expect("Error fetching applications from SinkController")
    };

    let ignored_apps = SHARED_SETTINGS.lock().await.ignored_apps_list.clone();
    mixer::create_mixer_channels(applications, &ignored_apps).await;

    // Register global event handler and action
    set_global_event_handler(&GlobalHandler);
    register_action(VolumeControllerAction).await;

    run(std::env::args().collect()).await
}
