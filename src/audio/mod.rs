pub mod audio_system;
pub mod pulse;

pub use audio_system::{AppInfo, AudioSystem};
pub use pulse::PulseAudioSystem;

pub fn create_audio_system() -> Box<dyn AudioSystem> {
    Box::new(PulseAudioSystem::new().unwrap())
}
