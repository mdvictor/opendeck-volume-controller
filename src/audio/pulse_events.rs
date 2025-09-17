use super::events::{AudioEvent, AudioEventListener};
use libpulse_binding::{
    context::{
        Context, FlagSet,
        subscribe::{Facility, InterestMaskSet, Operation},
    },
    mainloop::threaded::Mainloop,
    proplist::Proplist,
};
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

pub struct PulseAudioEventListener {
    // We'll use Arc<Mutex<>> to make this Send + Sync
    inner: Arc<Mutex<Option<PulseEventInner>>>,
}

struct PulseEventInner {
    _mainloop: Mainloop,
    _context: Context,
}

impl PulseAudioEventListener {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }
}

impl AudioEventListener for PulseAudioEventListener {
    fn start_listening(&mut self) -> Result<mpsc::UnboundedReceiver<AudioEvent>, Box<dyn Error>> {
        let (tx, rx) = mpsc::unbounded_channel();

        // Create mainloop
        let mut mainloop = Mainloop::new().ok_or("Failed to create mainloop")?;

        mainloop.start().map_err(|_| "Failed to start mainloop")?;

        // Create proplist for context
        let mut proplist = Proplist::new().ok_or("Failed to create proplist")?;
        proplist
            .set_str("application.name", "Volume Controller")
            .map_err(|e| format!("Failed to set application name: {:?}", e))?;

        // Create context
        let mut context =
            Context::new_with_proplist(&mainloop, "VolumeControllerEvents", &proplist)
                .ok_or("Failed to create context")?;

        // Set up event subscription callback
        let event_sender = tx.clone();

        context.set_subscribe_callback(Some(Box::new(move |facility, operation, _index| {
            let event = match (facility, operation) {
                (Some(Facility::SinkInput), Some(Operation::New)) => {
                    Some(AudioEvent::SinkInputAdded)
                }
                (Some(Facility::SinkInput), Some(Operation::Removed)) => {
                    Some(AudioEvent::SinkInputRemoved)
                }
                (Some(Facility::SinkInput), Some(Operation::Changed)) => {
                    Some(AudioEvent::SinkInputChanged)
                }
                _ => None,
            };

            if let Some(event) = event {
                let _ = event_sender.send(event);
            }
        })));

        // Connect to PulseAudio server
        context
            .connect(None, FlagSet::NOFLAGS, None)
            .map_err(|e| format!("Failed to connect to PulseAudio: {:?}", e))?;

        // Wait for connection to be ready
        let mut attempts = 0;
        loop {
            match context.get_state() {
                libpulse_binding::context::State::Ready => break,
                libpulse_binding::context::State::Failed => {
                    return Err("PulseAudio connection failed".into());
                }
                libpulse_binding::context::State::Terminated => {
                    return Err("PulseAudio connection terminated".into());
                }
                _ => {
                    attempts += 1;
                    if attempts > 100 {
                        // 5 seconds timeout
                        return Err("PulseAudio connection timeout".into());
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }

        // Subscribe to sink input events (application audio streams)
        let interest = InterestMaskSet::SINK_INPUT;
        context.subscribe(interest, |_success| {
            // Callback for subscription result
        });

        // Store the mainloop and context
        let inner = PulseEventInner {
            _mainloop: mainloop,
            _context: context,
        };

        *self.inner.lock().unwrap() = Some(inner);

        println!("PulseAudio event listener started successfully");
        Ok(rx)
    }

    fn stop_listening(&mut self) -> Result<(), Box<dyn Error>> {
        let mut inner_guard = self.inner.lock().unwrap();
        if let Some(_inner) = inner_guard.take() {
            // Context and mainloop will be dropped automatically
            println!("PulseAudio event listener stopped");
        }
        Ok(())
    }
}

impl Drop for PulseAudioEventListener {
    fn drop(&mut self) {
        let _ = self.stop_listening();
    }
}
