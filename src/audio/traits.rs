use std::error::Error;

#[derive(Debug)]
pub struct AppInfo {
    pub uid: u32,
    pub name: String,
    pub mute: bool,
    pub volume_percentage: f32,
    pub icon_name: Option<String>,
    pub is_sink: bool,
}

pub trait AudioSystem {
    fn list_applications(&mut self) -> Result<Vec<AppInfo>, Box<dyn Error>>;
    fn increase_volume(
        &mut self,
        app_index: u32,
        percent: f64,
        is_sink: bool,
    ) -> Result<(), Box<dyn Error>>;
    fn decrease_volume(
        &mut self,
        app_index: u32,
        percent: f64,
        is_sink: bool,
    ) -> Result<(), Box<dyn Error>>;
    fn mute_volume(
        &mut self,
        app_index: u32,
        mute: bool,
        is_sink: bool,
    ) -> Result<(), Box<dyn Error>>;
}
