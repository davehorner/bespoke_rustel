//! Nightshade's 16px text mesh is 0.16 world units tall before entity scaling.
pub fn world_scale_for_pixels(depth: f32, projection_y: f32, height: f32, pixels: f32) -> Option<f32> {
    if ![depth, projection_y, height, pixels].iter().all(|v| v.is_finite() && *v > 0.0) {
        return None;
    }
    Some((2.0 * depth * pixels / (projection_y * height * 0.16)).clamp(0.05, 32.0))
}

/// Uniform scale that keeps shaped text inside a block's top face.
pub fn face_text_scale(width: f32, depth: f32, text_width: f32, text_height: f32) -> f32 {
    if ![width, depth, text_width, text_height].iter().all(|v| v.is_finite() && *v > 0.0) { return 0.0; }
    (width * 0.85 / text_width).min(depth * 0.75 / text_height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_glyph_fits_short_and_long_blocks_without_stretching() {
        for width in [0.06, 0.3, 5.0] {
            let scale = face_text_scale(width, 0.22, 0.3, 0.16);
            assert!(scale > 0.0);
            assert!(0.3 * scale <= width * 0.85 + 1e-6);
            assert!(0.16 * scale <= 0.22 * 0.75 + 1e-6);
        }
        assert_eq!(face_text_scale(0.1, 0.2, 0.0, 0.16), 0.0);
    }

    #[test]
    fn normal_camera_keeps_labels_readable_instead_of_subpixel() {
        // A 16px glyph at depth 20, 1000px viewport, focal factor 2 should
        // occupy 12px, rather than the old 0.096px at entity scale 0.012.
        assert!((world_scale_for_pixels(20.0, 2.0, 1000.0, 12.0).unwrap() - 1.5).abs() < 1e-6);
    }

    #[test]
    fn label_size_compensates_for_zoom_and_viewport_resize() {
        let normal = world_scale_for_pixels(20.0, 2.0, 1000.0, 12.0).unwrap();
        assert_eq!(world_scale_for_pixels(40.0, 2.0, 1000.0, 12.0), Some(normal * 2.0));
        assert_eq!(world_scale_for_pixels(20.0, 2.0, 2000.0, 12.0), Some(normal * 0.5));
        assert_eq!(world_scale_for_pixels(-1.0, 2.0, 1000.0, 12.0), None);
        assert_eq!(world_scale_for_pixels(20.0, 2.0, 0.0, 12.0), None);
    }
}
