use openaction::{Action, Instance, visible_instances};
use tux_icons::icon_fetcher::IconFetcher;

use std::sync::atomic::{AtomicBool, Ordering};

use crate::gfx::TRANSPARENT_ICON;
use crate::mixer::{self, MixerChannel};
use crate::plugin::{COLUMN_TO_CHANNEL_MAP, VolumeControllerAction};

// Global flag to track if system mixer should be shown
static SHOW_SYSTEM_MIXER: AtomicBool = AtomicBool::new(false);

// Public getter for the global show_system_mixer flag
pub fn should_show_system_mixer() -> bool {
    SHOW_SYSTEM_MIXER.load(Ordering::Relaxed)
}

// Set the global flag
pub fn set_show_system_mixer(value: bool) {
    SHOW_SYSTEM_MIXER.store(value, Ordering::Relaxed);
}

pub async fn get_device_row_count() -> Option<u8> {
    let instances = visible_instances(VolumeControllerAction::UUID).await;
    if instances.is_empty() {
        return None;
    }

    let max_row = instances.iter().map(|i| i.coordinates.row).max()?;

    Some(max_row + 1)
}

pub async fn update_stream_deck_buttons() {
    let column_map = COLUMN_TO_CHANNEL_MAP.lock().await;
    let mut channels = mixer::MIXER_CHANNELS.lock().await;
    let row_count = get_device_row_count().await;

    for instance in visible_instances(VolumeControllerAction::UUID).await {
        let sd_column = instance.coordinates.column;

        let Some(&channel_index) = column_map.get(&sd_column) else {
            continue;
        };

        let Some(channel) = channels.get_mut(&channel_index) else {
            if let Some(rows) = row_count {
                if rows >= 3 {
                    cleanup_sd_column(&instance).await;
                } else {
                    // TODO check if there are knobs too and call appropiate cleanup fn
                    // update_sd_column_with_knob(&instance).await;
                }
            }
            continue;
        };

        match instance.coordinates.row {
            0 => channel.header_id = Some(instance.instance_id.clone()),
            1 => channel.upper_vol_btn_id = Some(instance.instance_id.clone()),
            2 => channel.lower_vol_btn_id = Some(instance.instance_id.clone()),
            _ => {}
        }

        if let Some(rows) = row_count {
            if rows >= 3 {
                update_sd_column(channel, &instance).await;
            } else {
                // TODO same logic as in cleanup for knobs
                // update_sd_column_with_knob(&instance).await;
            }
        }
    }
}

pub async fn update_header(instance: &Instance, channel: &MixerChannel) {
    let icon_uri = if channel.mute {
        channel.icon_uri_mute.clone()
    } else {
        channel.icon_uri.clone()
    };

    let _ = instance.set_image(Some(icon_uri), None).await;

    if channel.uses_default_icon {
        let _ = instance.set_title(Some(channel.name.clone()), None).await;
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
    } else {
        if let Ok(img) = image::load_from_memory(&image_data) {
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
        }
    };

    (normal_uri, muted_uri, uses_default_icon)
}

pub async fn cleanup_sd_column(instance: &Instance) {
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
        let svg_with_opacity = if before_close.contains("opacity=") {
            svg
        } else {
            let svg_tag_modified = before_close.replace("<svg", r#"<svg opacity="0.4""#);
            format!("{}{}", svg_tag_modified, after_open)
        };

        svg_with_opacity
    } else {
        svg
    }
}

async fn update_sd_column(channel: &MixerChannel, instance: &Instance) {
    match instance.coordinates.row {
        0 => {
            update_header(instance, channel).await;
        }
        1 | 2 => {
            // Update volume buttons with bar graphics
            if let Ok((upper_img, lower_img)) =
                crate::gfx::get_volume_bar_data_uri_split(channel.vol_percent)
            {
                if instance.coordinates.row == 1 {
                    let _ = instance.set_image(Some(upper_img), None).await;
                } else {
                    let _ = instance.set_image(Some(lower_img), None).await;
                }
            }
        }
        _ => {}
    }
}
