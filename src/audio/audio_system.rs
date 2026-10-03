use std::error::Error;

/// One logical mixer channel. When multiple PulseAudio sink-inputs belong to
/// the same app instance (e.g. a game opening several audio streams), they're
/// grouped into a single `AppInfo` with all of their uids in `member_uids`,
/// so they're shown and controlled together as one channel.
#[derive(Debug)]
pub struct AppInfo {
    pub member_uids: Vec<u32>,
    pub app_name: String,
    pub mute: bool,
    pub vol_percent: f32,
    pub icon_name: Option<String>,
    pub is_device: bool,
}

pub trait AudioSystem {
    fn list_applications(&mut self) -> Result<Vec<AppInfo>, Box<dyn Error>>;
    /// Set the absolute volume (0-100) of a single stream/device.
    fn set_volume(
        &mut self,
        app_index: u32,
        percent: f32,
        is_device: bool,
    ) -> Result<(), Box<dyn Error>>;
    fn mute_volume(
        &mut self,
        app_index: u32,
        mute: bool,
        is_device: bool,
    ) -> Result<(), Box<dyn Error>>;
}
