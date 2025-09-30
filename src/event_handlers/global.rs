use crate::audio::pulse_monitor;

pub struct GlobalEventHandler {}

impl openaction::GlobalEventHandler for GlobalEventHandler {
    fn device_did_connect(
        &self,
        _event: openaction::DeviceDidConnectEvent,
        _outbound: &mut openaction::OutboundEventManager,
    ) -> impl std::future::Future<Output = openaction::EventHandlerResult> + Send {
        async move {
            println!("Stream Deck connected - starting PulseAudio monitoring");
            pulse_monitor::start_pulse_monitoring();
            Ok(())
        }
    }
}
