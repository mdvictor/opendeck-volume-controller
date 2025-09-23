use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use image::{ImageBuffer, Rgb, RgbImage};
use std::io::Cursor;

/// Generate a volume bar image for Stream Deck (144x144)
pub fn generate_volume_bar(volume_percent: f32, background_color: Rgb<u8>) -> RgbImage {
    const WIDTH: u32 = 144;
    const HEIGHT: u32 = 144;
    const BAR_WIDTH: u32 = 12;
    const BAR_HEIGHT: u32 = 100;
    const CIRCLE_RADIUS: u32 = 8;

    // Create the image buffer
    let mut img = ImageBuffer::from_pixel(WIDTH, HEIGHT, background_color);

    // Calculate bar position (centered horizontally)
    let bar_x = (WIDTH - BAR_WIDTH) / 2;
    let bar_y = (HEIGHT - BAR_HEIGHT) / 2;

    // Colors
    let bar_background = Rgb([60, 60, 60]); // Dark gray for empty bar
    let bar_fill = Rgb([0, 255, 0]); // Green for filled portion
    let circle_color = Rgb([255, 255, 255]); // White circle
    let circle_border = Rgb([0, 0, 0]); // Black border

    // Draw the bar background
    draw_filled_rectangle(
        &mut img,
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
        draw_filled_rectangle(&mut img, bar_x, fill_y, BAR_WIDTH, fill_height, bar_fill);
    }

    // Calculate circle position on the bar
    let circle_x = bar_x + BAR_WIDTH / 2;
    let circle_y = fill_y;

    // Draw the circle with border
    draw_filled_circle(
        &mut img,
        circle_x,
        circle_y,
        CIRCLE_RADIUS + 1,
        circle_border,
    );
    draw_filled_circle(&mut img, circle_x, circle_y, CIRCLE_RADIUS, circle_color);

    img
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

// Example usage in your Stream Deck plugin
pub fn get_volume_bar_base64(volume_percent: f32) -> Result<String> {
    let background = Rgb([30, 30, 30]);
    let img = generate_volume_bar(volume_percent, background);

    // Create a buffer to write PNG data to
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer);

    // Encode as PNG
    img.write_to(&mut cursor, image::ImageFormat::Png)?;

    // Convert to base64
    let base64_string = general_purpose::STANDARD.encode(&buffer);

    Ok(base64_string)
}

// Alternative: Get data URI format (ready to use in HTML/CSS)
pub fn get_volume_bar_data_uri(volume_percent: f32) -> Result<String> {
    let base64 = get_volume_bar_base64(volume_percent)?;
    Ok(format!("data:image/png;base64,{}", base64))
}
