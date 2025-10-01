use super::traits::{AppInfo, AudioSystem};
use crate::utils::get_application_name;
use libpulse_binding::volume::ChannelVolumes;
use pulsectl::controllers::{AppControl, SinkController};
use std::error::Error;

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
        let apps = self.controller.list_applications()?;
        println!("APPS: {:?}", apps);
        let res: Vec<AppInfo> = apps
            .into_iter()
            .map(|app| AppInfo {
                uid: app.index,
                name: get_application_name(&app),
                mute: app.mute,
                volume_percentage: get_pulse_app_volume_percentage(&app.volume),
                icon_name: app.proplist.get_str("application.icon_name"),
            })
            .collect();

        Ok(res)
    }

    fn increase_volume(&mut self, app_index: u32, percent: f64) -> Result<(), Box<dyn Error>> {
        self.controller
            .increase_app_volume_by_percent(app_index, percent);
        Ok(())
    }

    fn decrease_volume(&mut self, app_index: u32, percent: f64) -> Result<(), Box<dyn Error>> {
        self.controller
            .decrease_app_volume_by_percent(app_index, percent);
        Ok(())
    }

    fn mute_volume(&mut self, app_index: u32, mute: bool) -> Result<(), Box<dyn Error>> {
        self.controller.set_app_mute(app_index, mute)?;
        Ok(())
    }
}

fn get_pulse_app_volume_percentage(channel_volumes: &ChannelVolumes) -> f32 {
    const PA_VOLUME_NORM: u32 = 98304; // 100% in PulseAudio
    let channel_count = channel_volumes.len();

    if channel_count == 0 {
        return 0.0;
    }

    // Get average of all channels
    let total_volume: u32 = (0..channel_count)
        .map(|i| channel_volumes.get()[i as usize].0) // Extract the inner value from Volume(x)
        .sum();

    let avg_volume = total_volume as f32 / channel_count as f32;
    let perc = (avg_volume / PA_VOLUME_NORM as f32) * 100.0;
    perc.min(100.0)
}
