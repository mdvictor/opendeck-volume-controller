use super::traits::AudioSystem;
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
