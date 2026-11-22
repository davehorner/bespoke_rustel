//! Geometry shared by radial browser views; mirrors Bespoke RustelViewModes.cpp.
use std::f64::consts::TAU;

pub fn spiral(age: f64, pitch: f64, low: f64, high: f64, radius: f64) -> (f64, f64) {
    let r = radius * (0.12 + (age + 1.5) * 0.39 + (pitch - low) / (high - low) * 0.08);
    let angle = age * TAU - TAU * 0.25;
    (angle.cos() * r, angle.sin() * r)
}
pub fn wheel(pitch: f64, low: f64, high: f64, radius: f64) -> (f64, f64) {
    let angle = pitch.rem_euclid(12.0) / 12.0 * TAU - TAU * 0.25;
    let r = radius * (0.3 + 0.65 * (pitch - low) / (high - low));
    (angle.cos() * r, angle.sin() * r)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spiral_tracks_time_while_pitch_only_offsets_radius() {
        let at = spiral(0.0, 36.0, 36.0, 84.0, 100.0);
        assert!(at.0.abs() < 1e-10);
        assert!((at.1 + 70.5).abs() < 1e-10);
        let later = spiral(0.25, 36.0, 36.0, 84.0, 100.0);
        assert!((later.0 - 80.25).abs() < 1e-10);
        assert!(later.1.abs() < 1e-10);
        let higher = spiral(0.25, 84.0, 36.0, 84.0, 100.0);
        assert!((higher.0 - later.0 - 8.0).abs() < 1e-10);
        assert!(higher.1.abs() < 1e-10);
    }
    #[test]
    fn wheel_keeps_pitch_class_angle_and_separates_octaves() {
        let a = wheel(60.0, 36.0, 84.0, 100.0);
        let b = wheel(72.0, 36.0, 84.0, 100.0);
        assert!(a.0.abs() < 1e-10 && b.0.abs() < 1e-10);
        assert!(b.1 < a.1);
        let next = wheel(63.0, 36.0, 84.0, 100.0);
        assert!(next.0 > 0.0 && next.1.abs() < 1e-10);
    }
}
