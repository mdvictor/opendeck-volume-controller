use std::error::Error;

pub trait AudioSystem {
    fn increase_volume(&mut self, app_index: u32, percent: f64) -> Result<(), Box<dyn Error>>;
    fn decrease_volume(&mut self, app_index: u32, percent: f64) -> Result<(), Box<dyn Error>>;
    fn mute_volume(&mut self, app_index: u32, mute: bool) -> Result<(), Box<dyn Error>>;
}
