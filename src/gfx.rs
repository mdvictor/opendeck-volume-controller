use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use image::{ImageBuffer, Rgb, RgbImage, Rgba, RgbaImage};
use std::collections::HashMap;
use std::fmt;
use std::io::Cursor;
use std::sync::{LazyLock, Mutex, OnceLock};

static VOLUME_BAR_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

pub static TRANSPARENT_ICON: LazyLock<String> = LazyLock::new(|| {
    const ICON_SIZE: u32 = 144;

    // Create a transparent RGBA image
    let img = RgbaImage::from_pixel(ICON_SIZE, ICON_SIZE, Rgba([0, 0, 0, 0]));

    // Encode to PNG
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer);
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .expect("Failed to encode transparent icon");

    // Convert to base64 and return as data URI
    let base64 = general_purpose::STANDARD.encode(&buffer);
    format!("data:image/png;base64,{}", base64)
});

#[derive(Debug)]
pub enum BarPosition {
    Upper,
    Lower,
}

impl fmt::Display for BarPosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BarPosition::Upper => write!(f, "Upper"),
            BarPosition::Lower => write!(f, "Lower"),
        }
    }
}

// Helper function to get or initialize the cache
fn get_cache() -> &'static Mutex<HashMap<String, String>> {
    VOLUME_BAR_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

// Generate a cache key for the volume bar
fn generate_cache_key(volume_percent: f32, position: BarPosition) -> String {
    format!("vol_{:.1}_part_{}", volume_percent, position)
}

pub fn set_cached_value(key: String, value: String) -> Result<(), String> {
    match get_cache().lock() {
        Ok(mut cache) => {
            cache.insert(key, value);
            Ok(())
        }
        Err(_) => Err("Failed to acquire cache lock".to_string()),
    }
}
fn get_cached_value_safe(key: &str) -> Result<Option<String>, String> {
    match get_cache().lock() {
        Ok(cache) => Ok(cache.get(key).cloned()),
        Err(_) => Err("Failed to acquire cache lock".to_string()),
    }
}

/// Generate a volume bar image spanning 2 Stream Deck icons (288x144 total)
/// Returns (top_image, bottom_image) as separate 144x144 images
pub fn generate_volume_bar_split(
    volume_percent: f32,
    background_color: Rgb<u8>,
) -> (RgbImage, RgbImage) {
    const ICON_WIDTH: u32 = 144;
    const ICON_HEIGHT: u32 = 144;
    const TOTAL_HEIGHT: u32 = 288; // 2 icons vertically
    const BAR_WIDTH: u32 = 12;
    const BAR_HEIGHT: u32 = 240; // Taller bar spanning both icons
    const CIRCLE_RADIUS: u32 = 8;

    // Create the full-size image buffer (288x144)
    let mut full_img = ImageBuffer::from_pixel(ICON_WIDTH, TOTAL_HEIGHT, background_color);

    // Calculate bar position (centered horizontally)
    let bar_x = (ICON_WIDTH - BAR_WIDTH) / 2;
    let bar_y = (TOTAL_HEIGHT - BAR_HEIGHT) / 2; // Centered vertically in the full image

    // Colors
    let bar_background = Rgb([60, 60, 60]); // Dark gray for empty bar
    let bar_fill = Rgb([0, 255, 0]); // Green for filled portion
    let circle_color = Rgb([255, 255, 255]); // White circle
    let circle_border = Rgb([0, 0, 0]); // Black border

    // Draw the bar background
    draw_filled_rectangle(
        &mut full_img,
        bar_x,
        bar_y,
        BAR_WIDTH,
        BAR_HEIGHT,
        bar_background,
    );

    // Calculate fill height based on volume percentage
    let fill_height = ((volume_percent / 100.0) * BAR_HEIGHT as f32) as u32;
    let fill_y = bar_y + BAR_HEIGHT - fill_height;

    // Draw the filled portion (from bottom up)
    if fill_height > 0 {
        draw_filled_rectangle(
            &mut full_img,
            bar_x,
            fill_y,
            BAR_WIDTH,
            fill_height,
            bar_fill,
        );
    }

    // Calculate circle position on the bar
    let circle_x = bar_x + BAR_WIDTH / 2;
    let circle_y = fill_y;

    // Draw the circle with border
    draw_filled_circle(
        &mut full_img,
        circle_x,
        circle_y,
        CIRCLE_RADIUS + 1,
        circle_border,
    );
    draw_filled_circle(
        &mut full_img,
        circle_x,
        circle_y,
        CIRCLE_RADIUS,
        circle_color,
    );

    // Split the image into top and bottom halves
    let mut top_img = ImageBuffer::from_pixel(ICON_WIDTH, ICON_HEIGHT, background_color);
    let mut bottom_img = ImageBuffer::from_pixel(ICON_WIDTH, ICON_HEIGHT, background_color);

    // Copy top half (first 144 pixels)
    for y in 0..ICON_HEIGHT {
        for x in 0..ICON_WIDTH {
            let pixel = full_img.get_pixel(x, y);
            top_img.put_pixel(x, y, *pixel);
        }
    }

    // Copy bottom half (last 144 pixels)
    for y in 0..ICON_HEIGHT {
        for x in 0..ICON_WIDTH {
            let pixel = full_img.get_pixel(x, y + ICON_HEIGHT);
            bottom_img.put_pixel(x, y, *pixel);
        }
    }

    (top_img, bottom_img)
}

/// Draw a filled rectangle
fn draw_filled_rectangle(
    img: &mut RgbImage,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    color: Rgb<u8>,
) {
    for dy in 0..height {
        for dx in 0..width {
            let px = x + dx;
            let py = y + dy;
            if px < img.width() && py < img.height() {
                img.put_pixel(px, py, color);
            }
        }
    }
}

/// Draw a filled circle
fn draw_filled_circle(
    img: &mut RgbImage,
    center_x: u32,
    center_y: u32,
    radius: u32,
    color: Rgb<u8>,
) {
    let cx = center_x as i32;
    let cy = center_y as i32;
    let r = radius as i32;

    for y in (cy - r)..=(cy + r) {
        for x in (cx - r)..=(cx + r) {
            let dx = x - cx;
            let dy = y - cy;
            let distance_squared = dx * dx + dy * dy;

            if distance_squared <= r * r && x >= 0 && y >= 0 {
                let px = x as u32;
                let py = y as u32;
                if px < img.width() && py < img.height() {
                    img.put_pixel(px, py, color);
                }
            }
        }
    }
}

/// Get base64 encoded volume bar images for 2 vertical Stream Deck icons
/// Returns (top_image_base64, bottom_image_base64)
pub fn get_volume_bar_base64_split(volume_percent: f32) -> Result<(String, String)> {
    let background = Rgb([30, 30, 30]);
    let (top_img, bottom_img) = generate_volume_bar_split(volume_percent, background);

    // Encode top image
    let mut top_buffer = Vec::new();
    let mut top_cursor = Cursor::new(&mut top_buffer);
    top_img.write_to(&mut top_cursor, image::ImageFormat::Png)?;
    let top_base64 = general_purpose::STANDARD.encode(&top_buffer);

    // Encode bottom image
    let mut bottom_buffer = Vec::new();
    let mut bottom_cursor = Cursor::new(&mut bottom_buffer);
    bottom_img.write_to(&mut bottom_cursor, image::ImageFormat::Png)?;
    let bottom_base64 = general_purpose::STANDARD.encode(&bottom_buffer);

    Ok((top_base64, bottom_base64))
}

/// Get data URI format for split volume bar images spanning 2 vertical Stream Deck icons
/// Returns (top_image_data_uri, bottom_image_data_uri)
pub fn get_volume_bar_data_uri_split(volume_percent: f32) -> Result<(String, String)> {
    // Check cache for both images
    let upper_key = generate_cache_key(volume_percent, BarPosition::Upper);
    let lower_key = generate_cache_key(volume_percent, BarPosition::Lower);

    if let (Ok(Some(cached_upper)), Ok(Some(cached_lower))) = (
        get_cached_value_safe(&upper_key),
        get_cached_value_safe(&lower_key),
    ) {
        return Ok((cached_upper, cached_lower));
    }

    // Generate both images
    let (top_base64, bottom_base64) = get_volume_bar_base64_split(volume_percent)?;
    let top_data_uri = format!("data:image/png;base64,{}", top_base64);
    let bottom_data_uri = format!("data:image/png;base64,{}", bottom_base64);

    // Cache both images
    let _ = set_cached_value(upper_key, top_data_uri.clone());
    let _ = set_cached_value(lower_key, bottom_data_uri.clone());

    Ok((top_data_uri, bottom_data_uri))
}
