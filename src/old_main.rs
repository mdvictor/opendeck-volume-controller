use openaction::*;
use pulsectl::controllers::AppControl;
use pulsectl::controllers::SinkController;
use serde_json::json;
use tokio::sync::Mutex;

#[derive(Clone)]
struct KeyInfo {
    context: String,
    row: u8,
    column: u8,
}

static KEY_CONTEXTS: Mutex<Vec<(String, KeyInfo)>> = Mutex::const_new(vec![]);

struct GlobalEventHandler {}
impl openaction::GlobalEventHandler for GlobalEventHandler {}

struct ActionEventHandler {}
impl openaction::ActionEventHandler for ActionEventHandler {
    async fn will_appear(
        &self,
        event: AppearEvent,
        _outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        let key = format!(
            "{},{}",
            event.payload.coordinates.row, event.payload.coordinates.column
        );

        let key_info = KeyInfo {
            context: event.context.clone(),
            row: event.payload.coordinates.row,
            column: event.payload.coordinates.column,
        };

        println!("EEEEV WIL APP {:?}", event);

        println!(
            "WE HAVE CONTEXT !! -- {} {} {} {}",
            key.clone(),
            key_info.row,
            key_info.column,
            key_info.context,
        );

        KEY_CONTEXTS.lock().await.push((key, key_info));

        Ok(())
    }

    async fn key_up(
        &self,
        event: KeyEvent,
        outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        println!("loasdasd1111g key_up");

        outbound
            .send_event(json!({
                "event": "setTitle",
                "context": event.context.clone(),
                "payload": {
                    "title": "Switching..."
                }
            }))
            .await?;

        let switch_event = json!({
            "event": "switchProfile",
            "device": event.device,
            "profile": "Sound",
        });
        let result = outbound.send_event(switch_event).await;

        println!("send_event returned: {:?}", result);

        Ok(())
    }

    async fn key_down(
        &self,
        event: KeyEvent,
        _outbound: &mut openaction::OutboundEventManager,
    ) -> EventHandlerResult {
        println!("KEY_DOWN_PRINTLN -- {:?}", event);
        log::error!("KEY_DOWN_LOGGAAA123 -- {:?}", event);

        // // Create the image as Some(String) with your base64 data
        // let image: Option<String> = Some(
        //     "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAJAAAACQBAMAAAAVaP+LAAAAG1BMVEX/abT/////e73/xuL/odD/2ez/7PX/tNn/jsaGP7scAAAACXBIWXMAAA7EAAAOxAGVKw4bAAAA40lEQVRoge3RwWrCQBSF4Us06GPIlIhLIaIusyndDgqSpeALCNp9oC/uHZmbMgHL1JXC/y2OzCEeEkYEAAAAAAAAeHMfaSR1sRzWj40PIX8jqcvjoP7DpdSYfFoktVz9oH5s2oR//PQhjcjC6unSW50hPOf7kJUUrdWleKvzhkadhf40VWdDWx2Kdd7QVx9q01o9PuuQ1TlDeskxgvnZhvTefV/nDJV1vd7cQ49Fu7KhfV2fvmOd90bOVe4eeqy6UROHnHM7F+usoSTacHF2CLf2j09LYqZf99wQAAAAAAAA8MJuuusbS85w4IYAAAAASUVORK5CYII=".to_string(),
        // );

        // // Create a state value - this could represent different button states
        // // For example: 0 = default, 1 = active/pressed, 2 = muted, etc.
        // let state: Option<u16> = Some(0);

        // outbound
        //     .set_image(event.context.clone(), image.clone(), state)
        //     .await?;

        // let keyContexts = KEY_CONTEXTS.lock().await;

        // for (key, keyInfo) in keyContexts.iter() {
        //     outbound
        //         .set_image(keyInfo.context.clone(), image.clone(), state)
        //         .await?;
        // }

        Ok(())
    }
}

#[tokio::main]
async fn main() {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        simplelog::Config::default(),
        simplelog::TerminalMode::Stdout,
        simplelog::ColorChoice::Never,
    )
    .unwrap();

    let mut handler = SinkController::create().unwrap();

    let devices = handler.list_applications().expect("Didnt work");

    println!("Playback devices: ");

    for dev in devices.clone() {
        log::error!(
            "[{}] {}, Volume: {} | {:?}",
            dev.index,
            dev.name.as_ref().unwrap(),
            dev.volume.print(),
            dev.proplist
        );
        println!(
            "[{}] {}, Volume: {} | {:?}",
            dev.index,
            dev.name.as_ref().unwrap(),
            dev.volume.print(),
            dev.proplist,
        );
    }

    if let Err(error) = init_plugin(GlobalEventHandler {}, ActionEventHandler {}).await {
        log::error!("Failed to init plugin: {}", error);
    }
}
