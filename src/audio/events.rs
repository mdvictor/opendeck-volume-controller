use std::error::Error;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum AudioEvent {
    SinkInputAdded,
    SinkInputRemoved,
    SinkInputChanged,
}

pub trait AudioEventListener {
    fn start_listening(&mut self) -> Result<mpsc::UnboundedReceiver<AudioEvent>, Box<dyn Error>>;
    fn stop_listening(&mut self) -> Result<(), Box<dyn Error>>;
}
