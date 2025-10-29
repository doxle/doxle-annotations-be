use image::{ImageFormat, imageops::FilterType};
use std::io::Cursor;

/// Thresholds for generating half-width previews
const MIN_FILE_SIZE_BYTES: usize = 1_000_000; // 1MB
const MIN_DIMENSION_PX: u32 = 2048;

/// Determine if image needs a half-width version
pub fn needs_half_width(file_size: usize, width: u32, height: u32) -> bool {
    file_size >= MIN_FILE_SIZE_BYTES || width >= MIN_DIMENSION_PX || height >= MIN_DIMENSION_PX
}

/// Generate half-width version of image
/// Returns (width, height, jpeg_bytes)
pub fn generate_half_width(image_bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    // Load image
    let img = image::load_from_memory(image_bytes)
        .map_err(|e| format!("Failed to load image: {}", e))?;
    
    let (orig_width, orig_height) = (img.width(), img.height());
    
    // Calculate half dimensions
    let new_width = orig_width / 2;
    let new_height = orig_height / 2;
    
    // Resize with high-quality Lanczos3 filter
    let resized = img.resize(new_width, new_height, FilterType::Lanczos3);
    
    // Encode as JPEG with quality 85
    let mut buf = Cursor::new(Vec::new());
    resized.write_to(&mut buf, ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode JPEG: {}", e))?;
    
    Ok((new_width, new_height, buf.into_inner()))
}

/// Get image dimensions without loading full image
pub fn get_dimensions(image_bytes: &[u8]) -> Result<(u32, u32), String> {
    let img = image::load_from_memory(image_bytes)
        .map_err(|e| format!("Failed to load image: {}", e))?;
    Ok((img.width(), img.height()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_half_width() {
        // Small file, small dimensions → No
        assert_eq!(needs_half_width(500_000, 1024, 768), false);
        
        // Large file, small dimensions → Yes
        assert_eq!(needs_half_width(2_000_000, 1024, 768), true);
        
        // Small file, large dimensions → Yes
        assert_eq!(needs_half_width(500_000, 4000, 3000), true);
        
        // Large file, large dimensions → Yes
        assert_eq!(needs_half_width(2_000_000, 4000, 3000), true);
    }
}
