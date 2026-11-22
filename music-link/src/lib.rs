//! Shared music messages. Network I/O never runs on a Session or audio thread.
use serde::{Deserialize, Serialize};

pub const SNAPSHOT_TOPIC: &str = "music/snapshot";
pub const COMMAND_TOPIC: &str = "music/command";
pub const RESPONSE_TOPIC: &str = "music/response";
pub const AUDIO_TOPIC: &str = "music/audio";

/// Stable UTF-8 name hash → bright hue. No dependency on declaration/lane order.
/// RGB travels with the event so every client uses exactly the same color.
pub fn named_color(name: &str) -> [u8; 3] {
    let hash = name.bytes().fold(2166136261_u32, |h, b| (h ^ u32::from(b)).wrapping_mul(16777619));
    let hue = hash % 1536;
    let ramp = (hue % 256) as u8;
    let rgb = match hue / 256 {
        0 => [255, ramp, 0], 1 => [255 - ramp, 255, 0],
        2 => [0, 255, ramp], 3 => [0, 255 - ramp, 255],
        4 => [ramp, 0, 255], _ => [255, 0, 255 - ramp],
    };
    rgb.map(|v| 64 + (u16::from(v) * 180 / 255) as u8)
}

#[cfg(test)]
mod named_part_tests {
    use super::*;

    #[test]
    fn block_glyphs_distinguish_notes_from_unpitched_hits() {
        assert_eq!(note_glyph(Some(60.0)), "C4");
        assert_eq!(note_glyph(Some(61.0)), "C#4");
        assert_eq!(note_glyph(Some(69.0)), "A4");
        assert_eq!(note_glyph(Some(-1.0)), "B-2");
        assert_eq!(note_glyph(Some(60.25)), "C4+25c");
        assert_eq!(note_glyph(None), "x");
        assert_eq!(note_glyph(Some(f64::NAN)), "x");
    }

    #[test]
    fn names_have_stable_bounded_colors_and_old_events_still_decode() {
        assert_eq!(named_color("bass"), named_color("bass"));
        assert_ne!(named_color("bass"), named_color("rhythm"));
        assert_ne!(named_color("écho"), named_color("echo"));
        for channel in named_color("any_user_name") { assert!((64..=244).contains(&channel)); }
        let event: Event = serde_json::from_str(r##"{
            "id":1,"lane":"#0","cycle":0,"duration":1,"original_pitch":60,
            "pitch":60,"sound":"sine","gain":0.5
        }"##).unwrap();
        assert!(event.part.is_empty());
    }

    #[test]
    fn expanding_one_parent_scopes_shared_parts_and_never_duplicates_events() {
        let part = NamedPart { name: "moon".into(), color: named_color("moon") };
        let lane = |id: &str| Lane { id: id.into(), source: "$: arrangement".into(), parts: vec![part.clone()], ..Default::default() };
        let event = |lane: &str, part: &str, id| Event { lane: lane.into(), part: part.into(), id,
            part_color: named_color(part), cycle: 0.0, duration: 1.0, original_pitch: None,
            pitch: None, sound: "bd".into(), gain: 1.0, context: vec![], controls: Default::default() };
        let source = Snapshot { lanes: vec![lane("#1"), lane("#2")],
            events: vec![event("#1", "moon", 1), event("#2", "moon", 2), event("#1", "", 3)], ..Default::default() };
        let view = source.visual_lanes(&["#1".into()], "");
        assert_eq!(view.lanes.len(), 3);
        assert!(view.lanes[0].label.contains("arrangement"));
        assert_eq!(view.events.len(), source.events.len());
        assert_eq!(view.events[0].lane, view.lanes[1].id);
        assert_eq!(view.events[1].lane, "#2");
        assert_eq!(view.events[2].lane, "#1");
        let both = source.visual_lanes(&["#1".into(), "#2".into()], "");
        assert_ne!(both.events[0].lane, both.events[1].lane);
        let selected = source.visual_lanes(&["#1".into()], "#1");
        assert_eq!(selected.lanes.len(), 2);
        assert_eq!(selected.events.len(), 2);
        assert_eq!(source.lanes.len(), 2, "projection leaves control targets untouched");
    }

    #[test]
    fn core_expression_keeps_sample_colons_and_multiline_expression() {
        let lane = |source: &str| Lane { source: source.into(), ..Default::default() };
        assert_eq!(lane("s(\"bd:1\")").display_name(), "s(\"bd:1\")");
        assert_eq!(lane("$: \n arrangement.slow(2);").display_name(), "arrangement.slow(2)");
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AudioFrame {
    pub version: u32,
    pub session: String,
    pub generation: u64,
    pub sequence: u64,
    pub stream_id: u64,
    pub epoch: u64,
    pub end_frame: u64,
    pub sample_rate: u32,
    pub device_time: f64,
    pub scope: Vec<f32>,
    pub spectrum: Vec<f32>,
    pub sides: Vec<[f32; 2]>,
}

impl AudioFrame {
    pub fn valid(&self) -> bool {
        self.version == 1
            && self.session.len() <= 128
            && self.sample_rate > 0
            && self.device_time.is_finite()
            && self.scope.len() == 512
            && self.spectrum.len() == 512
            && self.sides.len() <= 4096
            && self
                .scope
                .iter()
                .chain(self.spectrum.iter())
                .all(|v| v.is_finite())
            && self.sides.iter().flatten().all(|v| v.is_finite())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Lane {
    pub id: String,
    pub label: String,
    /// Complete score statement for this lane, including its label and chain.
    #[serde(default)]
    pub source: String,
    pub line: usize,
    pub transpose: i32,
    #[serde(default)]
    pub parts: Vec<NamedPart>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NamedPart {
    pub name: String,
    pub color: [u8; 3],
}

impl Lane {
    pub fn display_name(&self) -> String {
        let source = self.source.trim();
        let body = source.split_once(':').filter(|(label, _)| !label.is_empty() && label.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$'))
            .map_or(source, |(_, body)| body.trim());
        let first = body.lines().next().unwrap_or("").trim().trim_end_matches(';');
        if first.is_empty() { self.label.clone() } else { first.chars().take(96).collect() }
    }
}

fn virtual_id(parent: &str, part: &str) -> String {
    format!("~{}:{parent}{part}", parent.len())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    /// Top-level const pattern binding, retained through composition.
    #[serde(default)]
    pub part: String,
    #[serde(default)]
    pub part_color: [u8; 3],
    #[serde(default)]
    pub context: Vec<(usize, usize)>,
    #[serde(default)]
    pub controls: std::collections::BTreeMap<String, f64>,
    pub id: u64,
    pub lane: String,
    pub cycle: f64,
    pub duration: f64,
    pub original_pitch: Option<f64>,
    pub pitch: Option<f64>,
    pub sound: String,
    pub gain: f64,
}

/// Scientific pitch name for timeline glyphs. Unpitched samples are hits (`x`),
/// never the arbitrary height the renderer uses to position a drum block.
pub fn note_glyph(pitch: Option<f64>) -> String {
    let Some(pitch) = pitch.filter(|p| p.is_finite() && (-256.0..=256.0).contains(p)) else { return "x".into(); };
    let midi = pitch.round() as i32;
    let names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let name = format!("{}{}", names[midi.rem_euclid(12) as usize], midi.div_euclid(12)-1);
    let cents = ((pitch - f64::from(midi)) * 100.0).round() as i32;
    if cents == 0 { name } else { format!("{name}{cents:+}c") }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LaneAudioStream {
    pub mapping: String,
    /// Stable slots are never reassigned during a session, including retired lanes.
    pub lanes: std::collections::BTreeMap<String, u8>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Binding {
    pub name: String,
    pub color: [u8; 3],
    pub from: usize,
    pub to: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub bindings: Vec<Binding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane_audio: Option<LaneAudioStream>,
    #[serde(default)]
    pub solo_lane: Option<String>,
    /// Exact evaluated document; context ranges are UTF-8 byte offsets here.
    #[serde(default)]
    pub source: String,
    pub version: u32,
    pub session: String,
    pub generation: u64,
    pub cycle: f64,
    pub cps: f64,
    pub lanes: Vec<Lane>,
    pub events: Vec<Event>,
}

impl Snapshot {
    /// A visual-only projection. Commands always use the original snapshot IDs.
    /// Parent rows hold unnamed events when expanded; each named child appears
    /// exactly once. Length-prefixed IDs scope identical names to their parents.
    pub fn visual_lanes(&self, expanded: &[String], selected: &str) -> Self {
        let mut output = self.clone();
        output.lanes.clear();
        let mut destinations = std::collections::BTreeMap::new();
        for lane in self.lanes.iter().filter(|l| selected.is_empty() || l.id == selected).take(64) {
            let mut core = lane.clone();
            let open = expanded.contains(&lane.id) && !lane.parts.is_empty();
            core.label = format!("{} {} / {}", if lane.parts.is_empty() { "·" } else if open { "−" } else { "+" }, lane.id, lane.display_name());
            output.lanes.push(core);
            if open {
                for part in lane.parts.iter().take(64) {
                    if output.lanes.len() >= 256 { break; }
                    let id = virtual_id(&lane.id, &part.name);
                    destinations.insert((lane.id.clone(), part.name.clone()), id.clone());
                    output.lanes.push(Lane { id, label: format!("  ↳ {}", part.name), source: part.name.clone(), ..Default::default() });
                }
            }
        }
        output.events.retain_mut(|event| {
            if !selected.is_empty() && event.lane != selected { return false; }
            if let Some(id) = destinations.get(&(event.lane.clone(), event.part.clone())) { event.lane = id.clone(); }
            true
        });
        output
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Command {
    pub request: String,
    pub session: String,
    pub generation: u64,
    pub lane: String,
    #[serde(default)]
    pub semitones: i32,
    /// None is a transpose command; Some enables/disables lane audition.
    #[serde(default)]
    pub solo: Option<bool>,
}

impl Command {
    pub fn validate(&self, snapshot: &Snapshot) -> Result<(), &'static str> {
        if self.session != snapshot.session || self.generation != snapshot.generation {
            return Err("stale session or score generation");
        }
        if self.solo != Some(false) && !snapshot.lanes.iter().any(|lane| lane.id == self.lane) {
            return Err("unknown lane");
        }
        if !(-48..=48).contains(&self.semitones) || self.request.len() > 128 {
            return Err("invalid transpose or request id");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Response {
    pub request: String,
    pub accepted: bool,
    pub message: String,
}

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn solo_commands_are_distinct_from_transpose_and_generation_checked() {
        let snapshot = Snapshot {
            session: "s".into(),
            generation: 2,
            lanes: vec![Lane {
                id: "#0".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut command: Command = serde_json::from_value(
            serde_json::json!({"request":"r","session":"s","generation":2,"lane":"#0","solo":true}),
        )
        .unwrap();
        assert_eq!(command.solo, Some(true));
        assert!(command.validate(&snapshot).is_ok());
        command.generation = 1;
        assert!(command.validate(&snapshot).is_err());
        command.generation = 2;
        command.lane = "#gone".into();
        assert!(command.validate(&snapshot).is_err());
        command.solo = Some(false);
        assert!(command.validate(&snapshot).is_ok());
        let legacy:Command=serde_json::from_value(serde_json::json!({"request":"r","session":"s","generation":2,"lane":"#0","semitones":7})).unwrap();
        assert!(legacy.solo.is_none());
        assert_eq!(legacy.semitones, 7);
    }
    #[test]
    fn audio_frames_are_bounded_and_reject_nonfinite_data() {
        let mut frame = AudioFrame {
            version: 1,
            sample_rate: 48_000,
            session: "test".into(),
            scope: vec![0.0; 512],
            spectrum: vec![-120.0; 512],
            sides: vec![[0.1, -0.1]; 1024],
            ..Default::default()
        };
        assert!(frame.valid());
        frame.scope[0] = f32::NAN;
        assert!(!frame.valid());
        frame.scope[0] = 0.0;
        frame.sides.resize(4097, [0.0, 0.0]);
        assert!(!frame.valid());
    }
    #[test]
    fn commands_cannot_address_a_replaced_score_or_missing_lane() {
        let snapshot = Snapshot {
            session: "a".into(),
            generation: 3,
            lanes: vec![Lane {
                id: "#7".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut command = Command {
            request: "r".into(),
            session: "a".into(),
            generation: 3,
            lane: "#7".into(),
            semitones: 7,
            solo: None,
        };
        assert!(command.validate(&snapshot).is_ok());
        command.generation = 2;
        assert!(command.validate(&snapshot).is_err());
        command.generation = 3;
        command.lane = "#8".into();
        assert!(command.validate(&snapshot).is_err());
    }
}
