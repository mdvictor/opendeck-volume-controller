use openaction::*;

#[derive(serde::Serialize)]
struct SwitchProfileEvent {
    event: &'static str,
    device: String,
    profile: String,
}

pub async fn run(outbound: &mut OutboundEventManager, profile: String) -> EventHandlerResult {
    outbound
        .send_event(SwitchProfileEvent {
            event: "switchProfile",
            device: "sd-DL08M2A38870".to_string(),
            profile,
        })
        .await?;

    println!("SENT SWITCH PROFILE EVENT");
    Ok(())
}
