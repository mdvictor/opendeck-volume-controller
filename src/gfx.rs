use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use image::{Rgba, RgbaImage};
use std::collections::HashMap;
use std::fmt;
use std::io::Cursor;
use std::sync::{LazyLock, Mutex, OnceLock};

static VOLUME_BAR_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

pub static TRANSPARENT_ICON: LazyLock<String> = LazyLock::new(|| {
    const ICON_SIZE: u32 = 144;
    let img = RgbaImage::from_pixel(ICON_SIZE, ICON_SIZE, Rgba([0, 0, 0, 0]));

    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer);
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .expect("Failed to encode transparent icon");

    let base64 = general_purpose::STANDARD.encode(&buffer);
    format!("data:image/png;base64,{}", base64)
});

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

fn get_cache() -> &'static Mutex<HashMap<String, String>> {
    VOLUME_BAR_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

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

/// Blend two colors with alpha blending
fn blend_colors(bg: Rgba<u8>, fg: Rgba<u8>, alpha: f32) -> Rgba<u8> {
    let alpha = alpha.clamp(0.0, 1.0);

    // If background is fully transparent, just return foreground with adjusted alpha
    if bg[3] == 0 {
        return Rgba([fg[0], fg[1], fg[2], (fg[3] as f32 * alpha) as u8]);
    }

    let fg_alpha = (fg[3] as f32 / 255.0) * alpha;
    let bg_alpha = bg[3] as f32 / 255.0;
    let final_alpha = fg_alpha + bg_alpha * (1.0 - fg_alpha);

    if final_alpha == 0.0 {
        return Rgba([0, 0, 0, 0]);
    }

    let r = ((fg[0] as f32 * fg_alpha + bg[0] as f32 * bg_alpha * (1.0 - fg_alpha)) / final_alpha)
        as u8;
    let g = ((fg[1] as f32 * fg_alpha + bg[1] as f32 * bg_alpha * (1.0 - fg_alpha)) / final_alpha)
        as u8;
    let b = ((fg[2] as f32 * fg_alpha + bg[2] as f32 * bg_alpha * (1.0 - fg_alpha)) / final_alpha)
        as u8;
    let a = (final_alpha * 255.0) as u8;

    Rgba([r, g, b, a])
}

/// Calculate signed distance from a point to a rounded rectangle
/// Negative values mean inside, positive values mean outside
fn rounded_rect_distance(
    px: f32,
    py: f32,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
) -> f32 {
    let dx = (px - x - width / 2.0).abs() - (width / 2.0 - radius);
    let dy = (py - y - height / 2.0).abs() - (height / 2.0 - radius);

    let outside_dist = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt();
    let inside_dist = dx.max(dy).min(0.0);

    outside_dist + inside_dist - radius
}

/// Generate a volume bar image spanning 2 Stream Deck icons (288x144 total)
/// Returns (top_image, bottom_image) as separate 144x144 images
pub fn generate_volume_bar_split(volume_percent: f32) -> (RgbaImage, RgbaImage) {
    const ICON_WIDTH: u32 = 144;
    const ICON_HEIGHT: u32 = 144;
    const TOTAL_HEIGHT: u32 = 288;
    const BAR_WIDTH: u32 = 25;
    const BAR_HEIGHT: u32 = 240;
    const CIRCLE_RADIUS: u32 = 20;
    const OUTLINE_THICKNESS: u32 = 6;

    let mut full_img = RgbaImage::from_pixel(ICON_WIDTH, TOTAL_HEIGHT, Rgba([0, 0, 0, 0]));

    let bar_x = (ICON_WIDTH - BAR_WIDTH) / 2;
    let bar_y = (TOTAL_HEIGHT - BAR_HEIGHT) / 2;

    let bar_fill = Rgba([255, 255, 255, 255]);
    let bar_outline = Rgba([255, 255, 255, 255]);
    let circle_outline = Rgba([255, 255, 255, 255]);
    let transparent = Rgba([0, 0, 0, 0]);

    draw_rounded_rect_outline_only_aa(
        &mut full_img,
        bar_x,
        bar_y,
        BAR_WIDTH,
        BAR_HEIGHT,
        BAR_WIDTH / 2,
        bar_outline,
        OUTLINE_THICKNESS,
    );

    // Calculate and draw the filled portion
    let fill_height = ((volume_percent / 100.0) * BAR_HEIGHT as f32) as u32;
    let fill_y = bar_y + BAR_HEIGHT - fill_height;

    if fill_height > OUTLINE_THICKNESS {
        for py in fill_y.max(bar_y + OUTLINE_THICKNESS)..(bar_y + BAR_HEIGHT - OUTLINE_THICKNESS) {
            for px in (bar_x + OUTLINE_THICKNESS)..(bar_x + BAR_WIDTH - OUTLINE_THICKNESS) {
                if px < full_img.width() && py < full_img.height() {
                    full_img.put_pixel(px, py, bar_fill);
                }
            }
        }
    }

    // Draw the volume indicator circle
    let circle_x = bar_x + BAR_WIDTH / 2;
    let circle_y = fill_y;

    // this hides what's underneath the volume pointer
    draw_filled_circle_no_aa(
        &mut full_img,
        circle_x,
        circle_y,
        CIRCLE_RADIUS.saturating_sub(OUTLINE_THICKNESS),
        transparent,
    );

    draw_hollow_circle(
        &mut full_img,
        circle_x,
        circle_y,
        CIRCLE_RADIUS,
        circle_outline,
        OUTLINE_THICKNESS,
    );

    // Split into top and bottom images
    let mut top_img = RgbaImage::from_pixel(ICON_WIDTH, ICON_HEIGHT, Rgba([0, 0, 0, 0]));
    let mut bottom_img = RgbaImage::from_pixel(ICON_WIDTH, ICON_HEIGHT, Rgba([0, 0, 0, 0]));

    for y in 0..ICON_HEIGHT {
        for x in 0..ICON_WIDTH {
            top_img.put_pixel(x, y, *full_img.get_pixel(x, y));
            bottom_img.put_pixel(x, y, *full_img.get_pixel(x, y + ICON_HEIGHT));
        }
    }

    (top_img, bottom_img)
}

/// Draw a filled circle without antialiasing
fn draw_filled_circle_no_aa(
    img: &mut RgbaImage,
    center_x: u32,
    center_y: u32,
    radius: u32,
    color: Rgba<u8>,
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

/// Draw a hollow circle with antialiasing
fn draw_hollow_circle(
    img: &mut RgbaImage,
    center_x: u32,
    center_y: u32,
    radius: u32,
    color: Rgba<u8>,
    thickness: u32,
) {
    let cx = center_x as f32;
    let cy = center_y as f32;
    let outer_r = radius as f32;
    let inner_r = (radius as f32) - (thickness as f32);

    let min_x = (cx - outer_r - 1.0).max(0.0) as u32;
    let max_x = (cx + outer_r + 1.0).min(img.width() as f32) as u32;
    let min_y = (cy - outer_r - 1.0).max(0.0) as u32;
    let max_y = (cy + outer_r + 1.0).min(img.height() as f32) as u32;

    for py in min_y..max_y {
        for px in min_x..max_x {
            let dx = px as f32 - cx;
            let dy = py as f32 - cy;
            let distance = (dx * dx + dy * dy).sqrt();

            if distance <= outer_r && distance >= inner_r {
                let mut alpha: f32 = 1.0;

                if distance > outer_r - 1.0 {
                    alpha = alpha.min(outer_r - distance);
                }

                if distance < inner_r + 1.0 {
                    alpha = alpha.min(distance - inner_r);
                }

                if alpha > 0.0 {
                    let bg = img.get_pixel(px, py);
                    let blended = blend_colors(*bg, color, alpha);
                    img.put_pixel(px, py, blended);
                }
            }
        }
    }
}

/// Draw only the outline of a rounded rectangle with antialiasing
fn draw_rounded_rect_outline_only_aa(
    img: &mut RgbaImage,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    corner_radius: u32,
    outline_color: Rgba<u8>,
    outline_thickness: u32,
) {
    let x_f = x as f32;
    let y_f = y as f32;
    let width_f = width as f32;
    let height_f = height as f32;
    let r = corner_radius as f32;
    let thickness = outline_thickness as f32;

    let min_x = (x_f - 1.0).max(0.0) as u32;
    let max_x = (x_f + width_f + 1.0).min(img.width() as f32) as u32;
    let min_y = (y_f - 1.0).max(0.0) as u32;
    let max_y = (y_f + height_f + 1.0).min(img.height() as f32) as u32;

    for py in min_y..max_y {
        for px in min_x..max_x {
            let px_f = px as f32;
            let py_f = py as f32;

            let dist_outer = rounded_rect_distance(px_f, py_f, x_f, y_f, width_f, height_f, r);

            let inner_x = x_f + thickness;
            let inner_y = y_f + thickness;
            let inner_width = width_f - thickness * 2.0;
            let inner_height = height_f - thickness * 2.0;
            let inner_r = (r - thickness).max(0.0);
            let dist_inner = rounded_rect_distance(
                px_f,
                py_f,
                inner_x,
                inner_y,
                inner_width,
                inner_height,
                inner_r,
            );

            if dist_outer <= 0.0 && dist_inner > 0.0 {
                let mut alpha: f32 = 1.0;

                if dist_outer > -1.0 {
                    alpha = alpha.min(-dist_outer);
                }

                if dist_inner < 1.0 {
                    alpha = alpha.min(dist_inner);
                }

                if alpha > 0.0 {
                    let bg = img.get_pixel(px, py);
                    let blended = blend_colors(*bg, outline_color, alpha);
                    img.put_pixel(px, py, blended);
                }
            }
        }
    }
}

/// Get base64 encoded volume bar images for 2 vertical Stream Deck icons
pub fn get_volume_bar_base64_split(volume_percent: f32) -> Result<(String, String)> {
    let (top_img, bottom_img) = generate_volume_bar_split(volume_percent);

    let mut top_buffer = Vec::new();
    let mut top_cursor = Cursor::new(&mut top_buffer);
    top_img.write_to(&mut top_cursor, image::ImageFormat::Png)?;
    let top_base64 = general_purpose::STANDARD.encode(&top_buffer);

    let mut bottom_buffer = Vec::new();
    let mut bottom_cursor = Cursor::new(&mut bottom_buffer);
    bottom_img.write_to(&mut bottom_cursor, image::ImageFormat::Png)?;
    let bottom_base64 = general_purpose::STANDARD.encode(&bottom_buffer);

    Ok((top_base64, bottom_base64))
}

/// Get data URI format for split volume bar images
pub fn get_volume_bar_data_uri_split(volume_percent: f32) -> Result<(String, String)> {
    let upper_key = generate_cache_key(volume_percent, BarPosition::Upper);
    let lower_key = generate_cache_key(volume_percent, BarPosition::Lower);

    if let (Ok(Some(cached_upper)), Ok(Some(cached_lower))) = (
        get_cached_value_safe(&upper_key),
        get_cached_value_safe(&lower_key),
    ) {
        return Ok((cached_upper, cached_lower));
    }

    let (top_base64, bottom_base64) = get_volume_bar_base64_split(volume_percent)?;
    let top_data_uri = format!("data:image/png;base64,{}", top_base64);
    let bottom_data_uri = format!("data:image/png;base64,{}", bottom_base64);

    let _ = set_cached_value(upper_key, top_data_uri.clone());
    let _ = set_cached_value(lower_key, bottom_data_uri.clone());

    Ok((top_data_uri, bottom_data_uri))
}
