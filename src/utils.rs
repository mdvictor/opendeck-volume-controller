use pulsectl::controllers::types::ApplicationInfo;

pub fn get_application_name(app: &ApplicationInfo) -> String {
    // First, check if the main name field has a meaningful value
    if let Some(name) = &app.name {
        if !is_generic_name(name) {
            return name.clone();
        }
    }

    // Access proplist directly (it's not an Option)
    let proplist = &app.proplist;

    // Check application.name first (usually the best)
    if let Some(app_name) = proplist.get_str("application.name") {
        if !is_generic_name(&app_name) {
            return app_name;
        }
    }

    // Check application.process.binary (executable name)
    if let Some(binary) = proplist.get_str("application.process.binary") {
        if !is_generic_name(&binary) {
            return binary;
        }
    }

    // Check media.name (often has song/video titles)
    if let Some(media_name) = proplist.get_str("media.name") {
        if !is_generic_name(&media_name) {
            return format!("Media: {}", media_name);
        }
    }

    // Check application.icon_name (sometimes useful)
    if let Some(icon_name) = proplist.get_str("application.icon_name") {
        if !is_generic_name(&icon_name) {
            return icon_name;
        }
    }

    // Check for browser-specific properties
    if let Some(role) = proplist.get_str("media.role") {
        if role == "music" || role == "video" {
            // For browsers playing media, try to get more specific info
            if let Some(title) = proplist.get_str("media.title") {
                return format!("Browser: {}", title);
            }
            if let Some(artist) = proplist.get_str("media.artist") {
                return format!("Music: {}", artist);
            }
        }
    }

    // Absolute fallback
    app.name
        .as_deref()
        .unwrap_or("Unknown Application")
        .to_string()
}

fn is_generic_name(name: &str) -> bool {
    let generic_names = [
        "Playback",
        "playback",
        "ALSA",
        "PulseAudio",
        "output",
        "sink",
        "stream",
        "",
    ];

    generic_names.contains(&name) || name.trim().is_empty()
}
