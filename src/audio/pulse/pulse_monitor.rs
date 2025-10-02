use crate::{audio, mixer, utils};
use libpulse_binding::{
    context::{
        Context, FlagSet,
        subscribe::{Facility, InterestMaskSet, Operation},
    },
    mainloop::threaded::Mainloop,
    proplist::Proplist,
};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;

static MONITOR_STARTED: AtomicBool = AtomicBool::new(false);

// Global channel for refresh requests
static REFRESH_CHANNEL: LazyLock<(
    mpsc::UnboundedSender<()>,
    std::sync::Mutex<Option<mpsc::UnboundedReceiver<()>>>,
)> = LazyLock::new(|| {
    let (tx, rx) = mpsc::unbounded_channel();
    (tx, std::sync::Mutex::new(Some(rx)))
});

pub fn start_pulse_monitoring() {
    if MONITOR_STARTED.load(Ordering::Acquire) {
        return; // Already started
    }

    MONITOR_STARTED.store(true, Ordering::Release);

    // Start the refresh processor in tokio runtime
    start_refresh_processor();

    // Start PulseAudio monitoring in a regular thread
    std::thread::spawn(move || {
        println!("Starting PulseAudio monitoring...");

        // Create mainloop
        let mut mainloop = match Mainloop::new() {
            Some(m) => m,
            None => {
                eprintln!("Failed to create PulseAudio mainloop");
                return;
            }
        };

        if mainloop.start().is_err() {
            eprintln!("Failed to start PulseAudio mainloop");
            return;
        }

        // Create context
        let mut proplist = Proplist::new().unwrap();
        proplist
            .set_str("application.name", "Volume Controller")
            .unwrap();

        let mut context =
            match Context::new_with_proplist(&mainloop, "VolumeControllerMonitor", &proplist) {
                Some(c) => c,
                None => {
                    eprintln!("Failed to create PulseAudio context");
                    return;
                }
            };

        // Get the sender for refresh requests
        let refresh_sender = REFRESH_CHANNEL.0.clone();

        // Set up subscription callback
        context.set_subscribe_callback(Some(Box::new(move |facility, operation, _index| {
            match (facility, operation) {
                (Some(Facility::SinkInput), Some(Operation::New)) => {
                    println!("New audio application detected");
                    let _ = refresh_sender.send(());
                }
                (Some(Facility::SinkInput), Some(Operation::Removed)) => {
                    println!("Audio application removed");
                    let _ = refresh_sender.send(());
                }
                (Some(Facility::SinkInput), Some(Operation::Changed)) => {
                    println!("Audio application volume/mute changed");
                    let _ = refresh_sender.send(());
                }
                (Some(Facility::Sink), Some(Operation::Changed)) => {
                    println!("System sink (main PC audio) volume/mute changed");
                    let _ = refresh_sender.send(());
                }
                _ => {}
            }
        })));

        // Connect to PulseAudio
        if context.connect(None, FlagSet::NOFLAGS, None).is_err() {
            eprintln!("Failed to connect to PulseAudio");
            return;
        }

        // Wait for connection
        loop {
            match context.get_state() {
                libpulse_binding::context::State::Ready => break,
                libpulse_binding::context::State::Failed => {
                    eprintln!("PulseAudio connection failed");
                    return;
                }
                libpulse_binding::context::State::Terminated => {
                    eprintln!("PulseAudio connection terminated");
                    return;
                }
                _ => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }

        // Subscribe to sink input events and sink events
        context.subscribe(
            InterestMaskSet::SINK_INPUT | InterestMaskSet::SINK,
            |_success| {},
        );

        println!("PulseAudio monitoring started successfully");

        // Keep the context and mainloop alive
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
}

fn start_refresh_processor() {
    // Take the receiver from the global channel
    let receiver = REFRESH_CHANNEL.1.lock().unwrap().take();

    if let Some(mut receiver) = receiver {
        tokio::spawn(async move {
            loop {
                // Wait for first refresh request
                if receiver.recv().await.is_none() {
                    break;
                }

                // Debounce: wait 100ms and drain all pending refresh requests
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                // Drain any additional refresh requests that came in during debounce period
                while receiver.try_recv().is_ok() {
                    // Just drain them
                }

                println!("Processing debounced refresh request...");
                match refresh_audio_applications().await {
                    Ok(_) => println!("Audio applications refreshed successfully"),
                    Err(e) => eprintln!("Failed to refresh audio applications: {:?}", e),
                }
            }
        });
    }
}

pub async fn refresh_audio_applications() -> Result<(), Box<dyn std::error::Error>> {
    // Get current applications (same logic as manual-detection)
    let applications = {
        let mut audio_system = audio::create();
        audio_system
            .list_applications()
            .map_err(|e| format!("Error fetching applications: {:?}", e))?
    };

    // Update mixers and Stream Deck buttons
    mixer::update_mixer_channels(applications).await;
    utils::update_stream_deck_buttons().await;

    Ok(())
}
