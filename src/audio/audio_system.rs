use std::error::Error;

#[derive(Debug)]
pub struct AppInfo {
    pub uid: u32,
    pub app_name: String,
    pub sink_name: Option<String>,
    pub mute: bool,
    pub vol_percent: f32,
    pub icon_name: Option<String>,
    pub is_device: bool,
    pub is_multi_sink_app: bool,
}

pub trait AudioSystem {
    fn list_applications(&mut self) -> Result<Vec<AppInfo>, Box<dyn Error>>;
    fn increase_volume(
        &mut self,
        app_index: u32,
        percent: f64,
        is_device: bool,
    ) -> Result<(), Box<dyn Error>>;
    fn decrease_volume(
        &mut self,
        app_index: u32,
        percent: f64,
        is_device: bool,
    ) -> Result<(), Box<dyn Error>>;
    fn mute_volume(
        &mut self,
        app_index: u32,
        mute: bool,
        is_device: bool,
    ) -> Result<(), Box<dyn Error>>;
}
