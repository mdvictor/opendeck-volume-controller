use openaction::{Action, Instance, visible_instances};
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::Mutex;
use tux_icons::icon_fetcher::IconFetcher;

use crate::gfx::TRANSPARENT_ICON;
use crate::plugin::VCAction;

#[derive(Clone, Debug)]
pub struct VolumeApplicationColumn {
    pub header_id: Option<String>,
    pub upper_vol_btn_id: Option<String>,
    pub lower_vol_btn_id: Option<String>,
    pub uid: u32,
    pub name: String,
    pub mute: bool,
    pub vol_percent: f32,
    pub icon_uri: String,
    pub icon_uri_mute: String,
    pub uses_default_icon: bool,
    pub is_sink: bool,
}

// this should probably be a setting
// currently the volume controller will expect the SD volume apps
// to start on the second column, while the first one would be reserved
// for other actions (e.g.: the cancel btn that would take you back to
// your initial profile)
// but I reckon this is rather limiting? maybe you want all btns to be
// apps, but then you have no way to exit the controller through the SD?
// TODO
const STARTING_COL_KEY: u8 = 1;

pub static VOLUME_APPLICATION_COLUMNS: LazyLock<Mutex<HashMap<u8, VolumeApplicationColumn>>> =
    LazyLock::new(|| Mutex::const_new(HashMap::new()));

pub async fn create_application_volume_columns(applications: Vec<crate::audio::traits::AppInfo>) {
    let mut columns = VOLUME_APPLICATION_COLUMNS.lock().await;

    let mut col_key = STARTING_COL_KEY;
    for app in applications {
        let (icon_uri, icon_uri_mute, uses_default_icon) =
            get_app_icon_uri(app.icon_name, app.name.clone());
        columns.insert(
            col_key,
            VolumeApplicationColumn {
                header_id: None,
                upper_vol_btn_id: None,
                lower_vol_btn_id: None,
                uid: app.uid,
                name: app.name.clone(),
                mute: app.mute,
                vol_percent: app.volume_percentage,
                icon_uri,
                icon_uri_mute,
                uses_default_icon,
                is_sink: app.is_sink,
            },
        );

        col_key += 1;
    }
}

pub async fn update_application_volume_columns(applications: Vec<crate::audio::traits::AppInfo>) {
    let mut columns = VOLUME_APPLICATION_COLUMNS.lock().await;

    let mut col_key = STARTING_COL_KEY;
    for app in applications {
        if let Some(column) = columns.get_mut(&col_key) {
            // Update the column data
            column.uid = app.uid;
            column.name = app.name.clone();
            column.mute = app.mute;
            column.vol_percent = app.volume_percentage;
            column.is_sink = app.is_sink;
        } else {
            let (icon_uri, icon_uri_mute, uses_default_icon) =
                get_app_icon_uri(app.icon_name, app.name.clone());
            // Insert new column if it doesn't exist
            columns.insert(
                col_key,
                VolumeApplicationColumn {
                    header_id: None,
                    upper_vol_btn_id: None,
                    lower_vol_btn_id: None,
                    uid: app.uid,
                    name: app.name.clone(),
                    mute: app.mute,
                    vol_percent: app.volume_percentage,
                    icon_uri,
                    icon_uri_mute,
                    uses_default_icon,
                    is_sink: app.is_sink,
                },
            );
        }
        col_key += 1;
    }

    // Remove columns that no longer have corresponding apps
    columns.retain(|&key, _| key < col_key);

    println!(
        "Updated application volume controllers model: {:?}",
        columns
    );
}

pub async fn update_stream_deck_buttons() {
    let mut columns = VOLUME_APPLICATION_COLUMNS.lock().await;

    for instance in visible_instances(VCAction::UUID).await {
        let Some(column) = columns.get_mut(&instance.coordinates.column) else {
            //TODO switch case for type of device eventually
            cleanup_sd3x5_column(&instance).await;
            continue;
        };

        match instance.coordinates.row {
            0 => column.header_id = Some(instance.instance_id.clone()),
            1 => column.upper_vol_btn_id = Some(instance.instance_id.clone()),
            2 => column.lower_vol_btn_id = Some(instance.instance_id.clone()),
            _ => {}
        }

        //TODO switch case for type of device eventually
        update_sd3x5_btns(column, &instance).await;
    }
}

async fn update_sd3x5_btns(column: &VolumeApplicationColumn, instance: &Instance) {
    match instance.coordinates.row {
        0 => {
            update_header(instance, column).await;
        }
        1 | 2 => {
            // Update volume buttons with bar graphics
            if let Ok((upper_img, lower_img)) =
                crate::gfx::get_volume_bar_data_uri_split(column.vol_percent)
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

async fn cleanup_sd3x5_column(instance: &Instance) {
    let _ = instance.set_title(Some(""), None).await;
    let _ = instance
        .set_image(Some(TRANSPARENT_ICON.as_str()), None)
        .await;
}

pub async fn update_header(instance: &Instance, column: &VolumeApplicationColumn) {
    let icon_uri = if column.mute {
        column.icon_uri_mute.clone()
    } else {
        column.icon_uri.clone()
    };

    let _ = instance.set_image(Some(icon_uri), None).await;

    if column.uses_default_icon {
        let _ = instance.set_title(Some(column.name.clone()), None).await;
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
            .unwrap_or_else(|| PathBuf::from("imgs/wave-sound.png"))
    } else {
        fetcher
            .get_icon_path(fallback_icon_name)
            .unwrap_or_else(|| {
                // Use default
                uses_default_icon = true;
                PathBuf::from("imgs/wave-sound.png")
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

/// Add a grayscale CSS filter to an SVG
fn add_grayscale_filter_to_svg(svg: String) -> String {
    // Check if the SVG already has a <defs> section
    if let Some(svg_tag_end) = svg.find('>') {
        let before_close = &svg[..svg_tag_end + 1];
        let after_open = &svg[svg_tag_end + 1..];

        // Define the grayscale filter
        let filter = r#"<defs><filter id="grayscale"><feColorMatrix type="saturate" values="0"/></filter></defs>"#;

        let svg_with_filter = if before_close.contains("filter=") {
            svg
        } else {
            let svg_tag_modified =
                before_close.replace("<svg", &format!(r#"<svg filter="url(#grayscale)""#));
            format!("{}{}{}", svg_tag_modified, filter, after_open)
        };

        svg_with_filter
    } else {
        svg
    }
}
