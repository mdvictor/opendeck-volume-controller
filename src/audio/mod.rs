pub mod pulse;
pub mod traits;

// Re-export the important stuff
pub use pulse::PulseAudioSystem;
pub use traits::AudioSystem;

pub fn create_audio_system() -> Box<dyn AudioSystem> {
    Box::new(PulseAudioSystem::new().unwrap())
}
