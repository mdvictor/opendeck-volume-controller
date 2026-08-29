use openaction::{Action, Instance, visible_instances};
use tux_icons::icon_fetcher::IconFetcher;

use std::collections::HashSet;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use crate::gfx::{DIAL_IDLE_ICON, TRANSPARENT_ICON};
use crate::mixer::{self, MixerChannel};
use crate::plugin::{COLUMN_TO_CHANNEL_MAP, VolumeControllerAction};

// Global flag to track if system mixer should be shown
static SHOW_SYSTEM_MIXER: AtomicBool = AtomicBool::new(false);

/// Built-in OpenDeck touchscreen layouts used on the dial:
/// - `$B1`: icon + title + value + volume bar, used while a channel is assigned.
/// - `$X1`: just a centered icon (no bar, no value; title item exists but is
///   disabled), used while the dial has no app to show.
const ENCODER_LAYOUT_ACTIVE: &str = "$B1";
const ENCODER_LAYOUT_IDLE: &str = "$X1";

/// Instance IDs currently switched to `$X1`, so we only send a
/// `setFeedbackLayout` (which forces a full re-render) when a dial actually
/// transitions between "has an app" and "idle", not on every refresh.
static IDLE_ENCODER_LAYOUTS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::const_new(HashSet::new()));

/// Which kind of physical control an action instance is bound to.
/// Keypad columns and Encoder (dial) columns are numbered independently by
/// OpenDeck, so they're kept as separate namespaces when mapping to mixer channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ControllerKind {
    Keypad,
    Encoder,
}

impl From<&str> for ControllerKind {
    fn from(value: &str) -> Self {
        match value {
            "Encoder" => ControllerKind::Encoder,
            _ => ControllerKind::Keypad,
        }
    }
}

impl ControllerKind {
    pub fn of(instance: &Instance) -> Self {
        ControllerKind::from(instance.controller.as_str())
    }
}

pub struct ButtonPressControl {
    pub action_id: Option<String>,
    time_ms: Option<u128>,
}

impl ButtonPressControl {
    pub fn new() -> Self {
        ButtonPressControl {
            action_id: None,
            time_ms: None,
        }
    }

    pub fn set_press_time(&mut self, action_id: String) {
        self.action_id = Some(action_id);
        self.time_ms = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        );
    }

    pub fn get_release_time(&mut self) -> Option<u128> {
        self.action_id.as_ref()?;
        self.action_id = None;

        if let Some(press_time) = self.time_ms.take() {
            let release_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis();
            let duration = release_time - press_time;
            return Some(duration);
        }
        None
    }
}

// Public getter for the global show_system_mixer flag
pub fn should_show_system_mixer() -> bool {
    SHOW_SYSTEM_MIXER.load(Ordering::Relaxed)
}

// Set the global flag
pub fn set_show_system_mixer(value: bool) {
    SHOW_SYSTEM_MIXER.store(value, Ordering::Relaxed);
}

/// Row count of the Keypad grid only. Encoder (dial) instances always report
/// row 0 and would otherwise make a hybrid device look like it has a single row.
pub async fn get_device_row_count() -> Option<u8> {
    let instances = visible_instances(VolumeControllerAction::UUID).await;

    let max_row = instances
        .iter()
        .filter(|i| ControllerKind::of(i) == ControllerKind::Keypad)
        .filter_map(|i| i.coordinates.as_ref())
        .map(|coords| coords.row)
        .max()?;

    Some(max_row + 1)
}

pub async fn update_stream_deck_buttons() {
    let column_map = COLUMN_TO_CHANNEL_MAP.lock().await;
    let mut channels = mixer::MIXER_CHANNELS.lock().await;
    let row_count = get_device_row_count().await;

    for instance in visible_instances(VolumeControllerAction::UUID).await {
        let Some(coords) = instance.coordinates else {
            continue;
        };
        let controller = ControllerKind::of(&instance);

        let Some(&channel_index) = column_map.get(&(controller, coords.column)) else {
            continue;
        };

        let Some(channel) = channels.get_mut(&channel_index) else {
            match controller {
                ControllerKind::Encoder => cleanup_sd_column(&instance).await,
                ControllerKind::Keypad => {
                    if let Some(rows) = row_count {
                        if rows >= 3 {
                            cleanup_sd_column(&instance).await;
                        } else {
                            // TODO check if there are mini (2x3) devices too and call appropriate cleanup fn
                        }
                    }
                }
            }
            continue;
        };

        match controller {
            ControllerKind::Encoder => {
                channel.encoder_id = Some(instance.instance_id.clone());
                update_encoder_feedback(channel, &instance).await;
            }
            ControllerKind::Keypad => {
                match coords.row {
                    0 => channel.header_id = Some(instance.instance_id.clone()),
                    1 => channel.upper_vol_btn_id = Some(instance.instance_id.clone()),
                    2 => channel.lower_vol_btn_id = Some(instance.instance_id.clone()),
                    _ => {}
                }

                if let Some(rows) = row_count {
                    if rows >= 3 {
                        update_sd_column(channel, &instance).await;
                    } else {
                        // TODO same logic as in cleanup for mini (2x3) devices (appropriate update fn)
                    }
                }
            }
        }
    }
}

/// Push the current channel state (icon, app name, volume%) to a dial's
/// touchscreen segment via the `$B1` built-in layout (icon + title + value + bar).
pub async fn update_encoder_feedback(channel: &MixerChannel, instance: &Instance) {
    // If this dial was showing the idle placeholder, switch its layout back
    // to the full $B1 (icon/title/value/bar) before pushing real data.
    {
        let mut idle_layouts = IDLE_ENCODER_LAYOUTS.lock().await;
        if idle_layouts.remove(&instance.instance_id) {
            let _ = instance
                .set_feedback_layout(ENCODER_LAYOUT_ACTIVE.to_string())
                .await;
        }
    }

    let icon_uri = if channel.mute {
        channel.icon_uri_mute.clone()
    } else {
        channel.icon_uri.clone()
    };

    let title = to_title_case(&channel.app_name);

    let value = if channel.mute {
        "Muted".to_string()
    } else {
        format!("{:.0}%", channel.vol_percent)
    };

    let indicator_value = if channel.mute {
        0
    } else {
        channel.vol_percent.round() as i64
    };

    let feedback = serde_json::json!({
        "icon": icon_uri,
        "title": title,
        "value": value,
        "indicator": { "value": indicator_value },
    });

    let _ = instance.set_feedback(&feedback).await;
}

/// Capitalize the first letter of each word for display purposes only.
/// `app_name` itself stays lowercase everywhere else, since it's matched
/// verbatim against `ignored_apps_list`.
fn to_title_case(text: &str) -> String {
    text.split(' ')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub async fn update_header(instance: &Instance, channel: &MixerChannel) {
    let icon_uri = if channel.mute {
        channel.icon_uri_mute.clone()
    } else {
        channel.icon_uri.clone()
    };

    let _ = instance.set_image(Some(icon_uri), None).await;

    // Only show a title when we fell back to the generic icon — a real app
    // icon is recognizable enough on its own.
    if channel.uses_default_icon {
        let _ = instance
            .set_title(Some(channel.app_name.clone()), None)
            .await;
    } else {
        let _ = instance.set_title(Some(""), None).await;
    }
}

/// Get application icon as base64 data URIs
/// If icon_name is None, returns the default wave-sound.png icon
/// Otherwise, attempts to find and encode the system icon for the given icon name
/// Returns (normal_icon_uri, muted_icon_uri, uses_default_icon)
pub fn get_app_icon_uri(
    icon_name: Option<String>,
    fallback_icon_name: String,
) -> (String, String, bool) {
    use base64::{Engine as _, engine::general_purpose};
    use std::path::PathBuf;

    let fetcher = IconFetcher::new();
    let mut uses_default_icon = false;

    let icon_path = if let Some(name) = icon_name {
        fetcher
            .get_icon_path(name)
            .or_else(|| fetcher.get_icon_path(fallback_icon_name.clone()))
            .unwrap_or_else(|| PathBuf::from("img/wave-sound.png"))
    } else {
        fetcher
            .get_icon_path(fallback_icon_name)
            .unwrap_or_else(|| {
                // Use default
                uses_default_icon = true;
                PathBuf::from("img/wave-sound.png")
            })
    };

    let image_data = std::fs::read(&icon_path).expect("Failed to read icon file");

    let mime_type = match icon_path.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("xpm") => "image/x-xpm",
        _ => "image/png",
    };

    let base64_normal = general_purpose::STANDARD.encode(&image_data);
    let normal_uri = format!("data:{};base64,{}", mime_type, base64_normal);

    // grayscale on mute
    let muted_uri = if mime_type == "image/svg+xml" {
        if let Ok(svg_string) = String::from_utf8(image_data.clone()) {
            let grayscale_svg = add_grayscale_filter_to_svg(svg_string);
            let base64_gray = general_purpose::STANDARD.encode(grayscale_svg.as_bytes());
            format!("data:image/svg+xml;base64,{}", base64_gray)
        } else {
            normal_uri.clone()
        }
    } else if let Ok(img) = image::load_from_memory(&image_data) {
        let gray_img = image::DynamicImage::ImageLuma8(img.to_luma8());
        let mut buffer = std::io::Cursor::new(Vec::new());
        if gray_img
            .write_to(&mut buffer, image::ImageFormat::Png)
            .is_ok()
        {
            let gray_data = buffer.into_inner();
            let base64_gray = general_purpose::STANDARD.encode(&gray_data);
            format!("data:image/png;base64,{}", base64_gray)
        } else {
            normal_uri.clone()
        }
    } else {
        normal_uri.clone()
    };

    (normal_uri, muted_uri, uses_default_icon)
}

pub async fn cleanup_sd_column(instance: &Instance) {
    if ControllerKind::of(instance) == ControllerKind::Encoder {
        // No app assigned to this dial: switch to the minimal, centered-icon
        // layout so the name/percentage/bar don't show at all, and display a
        // dimmed placeholder icon instead of a blank segment.
        {
            let mut idle_layouts = IDLE_ENCODER_LAYOUTS.lock().await;
            if idle_layouts.insert(instance.instance_id.clone()) {
                let _ = instance
                    .set_feedback_layout(ENCODER_LAYOUT_IDLE.to_string())
                    .await;
            }
        }

        let feedback = serde_json::json!({
            "icon": DIAL_IDLE_ICON.as_str(),
            // $X1 still has a "title" item; an empty value would fall back
            // to showing the action's own name, so disable it outright.
            "title": { "enabled": false },
        });
        let _ = instance.set_feedback(&feedback).await;
        return;
    }

    let _ = instance.set_title(Some(""), None).await;
    let _ = instance
        .set_image(Some(TRANSPARENT_ICON.as_str()), None)
        .await;
}

/// Add a grayscale CSS filter to an SVG
fn add_grayscale_filter_to_svg(svg: String) -> String {
    // Check if the SVG already has a <defs> section
    if let Some(svg_tag_end) = svg.find('>') {
        let before_close = &svg[..svg_tag_end + 1];
        let after_open = &svg[svg_tag_end + 1..];

        // Simply reduce opacity instead of using filters (avoids blur)
        if before_close.contains("opacity=") {
            svg
        } else {
            let svg_tag_modified = before_close.replace("<svg", r#"<svg opacity="0.4""#);
            format!("{}{}", svg_tag_modified, after_open)
        }
    } else {
        svg
    }
}

async fn update_sd_column(channel: &MixerChannel, instance: &Instance) {
    let Some(coords) = instance.coordinates else {
        return;
    };

    match coords.row {
        0 => {
            update_header(instance, channel).await;
        }
        1 | 2 => {
            // Update volume buttons with bar graphics
            if let Ok((upper_img, lower_img)) =
                crate::gfx::get_volume_bar_data_uri_split(channel.vol_percent)
            {
                if coords.row == 1 {
                    let _ = instance.set_image(Some(upper_img), None).await;
                } else {
                    let _ = instance.set_image(Some(lower_img), None).await;
                }
            }
        }
        _ => {}
    }
}
