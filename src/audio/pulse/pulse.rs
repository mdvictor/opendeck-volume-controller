use crate::audio::{AppInfo, AudioSystem};
use libpulse_binding::volume::ChannelVolumes;
use pulsectl::controllers::{AppControl, DeviceControl, SinkController};
use std::error::Error;

const PA_VOLUME_NORM: u32 = 98304; // 150% in PulseAudio

pub struct PulseAudioSystem {
    controller: SinkController,
}

impl PulseAudioSystem {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            controller: SinkController::create()?,
        })
    }
}

impl AudioSystem for PulseAudioSystem {
    fn list_applications(&mut self) -> Result<Vec<AppInfo>, Box<dyn Error>> {
        let mut res: Vec<AppInfo> = Vec::new();

        // Add the default system sink (main PC audio) only if the global flag is set
        if crate::utils::should_show_system_mixer()
            && let Ok(default_sink) = self.controller.get_default_device()
        {
            res.push(AppInfo {
                uid: default_sink.index,
                app_name: default_sink
                    .description
                    .clone()
                    .unwrap_or("System Audio".to_string()),
                sink_name: Some("System Audio".to_string()),
                mute: default_sink.mute,
                vol_percent: get_pulse_app_volume_percentage(&default_sink.volume),
                icon_name: Some("audio-card".to_string()),
                is_device: true,
                is_multi_sink_app: false,
            });
        }

        // Add individual applications
        let apps = self.controller.list_applications()?;

        let app_names: Vec<String> = apps
            .iter()
            .map(|app| {
                app.proplist
                    .get_str("application.name")
                    .unwrap_or("app_stream".to_string())
                    .to_lowercase()
            })
            .collect();

        res.extend(apps.into_iter().map(|app| {
            let app_name = app
                .proplist
                .get_str("application.name")
                .unwrap_or("app_stream".to_string())
                .to_lowercase();

            let name_count = app_names.iter().filter(|&name| name == &app_name).count();

            AppInfo {
                uid: app.index,
                app_name,
                sink_name: app.name,
                mute: app.mute,
                vol_percent: get_pulse_app_volume_percentage(&app.volume),
                icon_name: app.proplist.get_str("application.icon_name"),
                is_device: false,
                is_multi_sink_app: name_count > 1,
            }
        }));

        Ok(res)
    }

    fn increase_volume(
        &mut self,
        app_index: u32,
        percent: f64,
        is_device: bool,
    ) -> Result<(), Box<dyn Error>> {
        if is_device {
            self.controller
                .increase_device_volume_by_percent(app_index, percent);
        } else {
            self.controller
                .increase_app_volume_by_percent(app_index, percent);
        }
        Ok(())
    }

    fn decrease_volume(
        &mut self,
        app_index: u32,
        percent: f64,
        is_device: bool,
    ) -> Result<(), Box<dyn Error>> {
        if is_device {
            self.controller
                .decrease_device_volume_by_percent(app_index, percent);
        } else {
            self.controller
                .decrease_app_volume_by_percent(app_index, percent);
        }
        Ok(())
    }

    fn mute_volume(
        &mut self,
        app_index: u32,
        mute: bool,
        is_device: bool,
    ) -> Result<(), Box<dyn Error>> {
        if is_device {
            self.controller.set_device_mute_by_index(app_index, mute);
        } else {
            self.controller.set_app_mute(app_index, mute)?;
        }
        Ok(())
    }
}

fn get_pulse_app_volume_percentage(channel_volumes: &ChannelVolumes) -> f32 {
    let channel_count = channel_volumes.len();
    if channel_count == 0 {
        return 0.0;
    }

    // Get average of all channels
    let total_volume: u32 = (0..channel_count)
        .map(|i| channel_volumes.get()[i as usize].0)
        .sum();

    let avg_volume = total_volume as f32 / channel_count as f32;
    let perc = (avg_volume / PA_VOLUME_NORM as f32) * 100.0;

    perc.min(100.0)
}
