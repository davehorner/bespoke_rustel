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

/// Block glyphs fill more of each face than the secondary rail inscriptions.
pub fn block_text_scale(width: f32, height: f32, text_width: f32, text_height: f32) -> f32 {
    if ![width, height, text_width, text_height].iter().all(|v| v.is_finite() && *v > 0.0) { return 0.0; }
    (width * 0.94 / text_width).min(height * 0.9 / text_height)
}

pub struct BlockFace {
    pub offset: [f32; 3],
    pub rotation: [f32; 3],
    pub size: [f32; 2],
}
pub fn block_faces([x, y, z]: [f32; 3]) -> [BlockFace; 6] {
    [
        BlockFace { offset: [0.0, y/2.0+0.006, 0.0], rotation: [-90.0, 0.0, 0.0], size: [x,z] },
        BlockFace { offset: [0.0, -y/2.0-0.006, 0.0], rotation: [90.0, 0.0, 0.0], size: [x,z] },
        BlockFace { offset: [0.0, 0.0, z/2.0+0.006], rotation: [0.0,0.0,0.0], size: [x,y] },
        BlockFace { offset: [0.0, 0.0, -z/2.0-0.006], rotation: [0.0,180.0,0.0], size: [x,y] },
        BlockFace { offset: [x/2.0+0.006, 0.0, 0.0], rotation: [0.0,90.0,0.0], size: [z,y] },
        BlockFace { offset: [-x/2.0-0.006, 0.0, 0.0], rotation: [0.0,-90.0,0.0], size: [z,y] },
    ]
}

pub fn rail_span(child: bool) -> (f32, f32) {
    if child { (-5.5, 3.5) } else { (-9.0, 6.0) }
}

pub fn segment_distance(p: [f32;2], a: [f32;2], b: [f32;2]) -> f32 {
    let dx=b[0]-a[0]; let dy=b[1]-a[1];
    let t=(((p[0]-a[0])*dx+(p[1]-a[1])*dy)/(dx*dx+dy*dy).max(1e-6)).clamp(0.0,1.0);
    ((p[0]-a[0]-t*dx).powi(2)+(p[1]-a[1]-t*dy).powi(2)).sqrt()
}

#[derive(Clone, Copy)]
pub struct LabelRect { pub x:f32, pub y:f32, pub width:f32, pub height:f32 }
impl LabelRect {
    pub fn overlaps(&self, other: &Self) -> bool {
        self.x < other.x+other.width && other.x < self.x+self.width
            && self.y < other.y+other.height && other.y < self.y+self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_faces_are_outside_the_box_and_use_their_own_dimensions() {
        let faces=block_faces([4.0,2.0,1.0]);
        assert_eq!(faces.len(),6);
        assert!(faces[0].offset[1]>1.0 && faces[1].offset[1]< -1.0);
        assert!(faces[2].offset[2]>0.5 && faces[3].offset[2]< -0.5);
        assert!(faces[4].offset[0]>2.0 && faces[5].offset[0]< -2.0);
        assert_eq!(faces[0].size,[4.0,1.0]);
        assert_eq!(faces[2].size,[4.0,2.0]);
        assert_eq!(faces[4].size,[1.0,2.0]);
        let parent=rail_span(false); let child=rail_span(true);
        assert!(child.0>parent.0 && child.1<parent.1);
    }

    #[test]
    fn rail_hit_testing_and_axis_label_clearance() {
        assert_eq!(segment_distance([5.0,3.0],[0.0,0.0],[10.0,0.0]),3.0);
        assert_eq!(segment_distance([13.0,4.0],[0.0,0.0],[10.0,0.0]),5.0);
        let title=LabelRect{x:0.0,y:0.0,width:100.0,height:20.0};
        assert!(title.overlaps(&LabelRect{x:90.0,y:10.0,width:30.0,height:15.0}));
        assert!(!title.overlaps(&LabelRect{x:0.0,y:25.0,width:30.0,height:15.0}));
    }

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
