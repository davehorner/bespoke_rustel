//! Shared music messages. Network I/O never runs on a Session or audio thread.
use serde::{Deserialize, Serialize};

pub const SNAPSHOT_TOPIC: &str = "music/snapshot";
pub const COMMAND_TOPIC: &str = "music/command";
pub const RESPONSE_TOPIC: &str = "music/response";
pub const AUDIO_TOPIC: &str = "music/audio";

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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LaneAudioStream {
    pub mapping: String,
    /// Stable slots are never reassigned during a session, including retired lanes.
    pub lanes: std::collections::BTreeMap<String, u8>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
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
