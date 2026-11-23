//! Opt-in producer-side telemetry and pointwise transpose overlays.
//! No extra pattern queries and no socket work on the Session thread.
use super::*;
use music_link::{Event, Link, Response, Snapshot};
use rustel_core::Value;
use std::borrow::Cow;
use std::collections::HashMap;

pub(super) struct MusicState {
    link: Option<Link>,
    snapshot: Snapshot,
    offsets: HashMap<String, i32>,
    lane_slots: std::collections::BTreeMap<String, u8>,
    last_publish: Instant,
    #[cfg(feature = "device-audio")]
    audio: Option<MusicAudio>,
}

impl Default for MusicState {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let enabled = std::env::var("RUSTEL_MUSIC_LINK").is_ok_and(|v| v == "1");
        Self {
            link: enabled.then(|| Link::connect().ok()).flatten(),
            snapshot: Snapshot {
                version: 1,
                session: format!(
                    "rustel-{}-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis(),
                    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ),
                ..Default::default()
            },
            offsets: HashMap::new(),
            lane_slots: Default::default(),
            last_publish: Instant::now(),
            #[cfg(feature = "device-audio")]
            audio: None,
        }
    }
}

fn controls(value: &Value) -> Option<Cow<'_, rustel_core::OrderedMap>> {
    match value {
        Value::Object(controls) => Some(Cow::Borrowed(controls)),
        Value::JsValue(_) => match rustel_core::materialize_js_value(value) {
            Value::Object(controls) => Some(Cow::Owned(controls)),
            _ => None,
        },
        _ => None,
    }
}

fn reserve_lane_slot(slots: &mut std::collections::BTreeMap<String,u8>, lane: &str) -> Option<u8> {
    if let Some(&slot) = slots.get(lane) { return Some(slot); }
    if slots.len() >= 15 { return None; }
    let slot = slots.len() as u8;
    slots.insert(lane.to_owned(),slot);
    Some(slot)
}

fn pitch(value: &Value) -> Option<f64> {
    match value {
        Value::Str(note) => rustel_core::util::note_to_midi(note, 3).ok(),
        _ => value.as_f64(),
    }
}

fn transpose(value: &mut Value, semitones: i32) -> Option<f64> {
    let controls = controls(value)?;
    let note = controls.get("note").and_then(pitch);
    let frequency = controls
        .get("freq")
        .and_then(Value::as_f64)
        .filter(|f| *f > 0.0 && f.is_finite());
    let original = frequency.map(|f| 69.0 + 12.0 * (f / 440.0).log2()).or(note);
    if semitones != 0 && original.is_some() {
        let mut controls = controls.into_owned();
        if let Some(note) = note {
            controls.insert("note".into(), Value::F64(note + f64::from(semitones)));
        }
        if let Some(freq) = frequency {
            controls.insert(
                "freq".into(),
                Value::F64(freq * 2.0_f64.powf(f64::from(semitones) / 12.0)),
            );
        }
        *value = Value::Object(controls);
    }
    original
}

impl Session {
    pub fn music_link_enabled(&self) -> bool {
        self.music.link.is_some()
    }
    #[cfg(feature = "device-audio")]
    pub fn publish_music_audio(&mut self, device: &rustel_audio::LiveScalarDevice) {
        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
        { self.music.snapshot.lane_audio = device.lane_audio_mapping().map(|mapping| music_link::LaneAudioStream {
            mapping: mapping.to_owned(), lanes: self.music.lane_slots.clone(),
        }); }
        let Some(link) = self.music.link.as_ref() else {
            return;
        };
        // Arming copies only into the existing allocation-free callback tap.
        device.set_analysis_enabled(true);
        let state = self.music.audio.get_or_insert_with(MusicAudio::new);
        if Instant::now() < state.next {
            return;
        }
        state.next = Instant::now() + Duration::from_millis(33);
        let Some(snapshot) = device.copy_analysis_window(&mut state.samples) else {
            return;
        };
        let identity = (snapshot.stream_id, snapshot.epoch, snapshot.end_frame);
        if state.last == Some(identity) {
            return;
        }
        state.last = Some(identity);
        let analysis = state.analyzer.analyze(&state.samples[..]);
        let mut sides = [(0.0, 0.0); crate::ui_analysis::UI_SIDES_SAMPLES];
        let stereo = if device.copy_analysis_sides(&mut sides) {
            sides.into_iter().map(|(l, r)| [l, r]).collect()
        } else {
            Vec::new()
        };
        state.sequence = state.sequence.saturating_add(1);
        link.publish_audio(music_link::AudioFrame {
            version: 1,
            session: self.music.snapshot.session.clone(),
            generation: device.generation(),
            sequence: state.sequence,
            stream_id: snapshot.stream_id,
            epoch: snapshot.epoch,
            end_frame: snapshot.end_frame,
            sample_rate: snapshot.sample_rate,
            device_time: snapshot.end_frame as f64 / f64::from(snapshot.sample_rate.max(1)),
            scope: analysis.scope.to_vec(),
            spectrum: analysis.spectrum.to_vec(),
            sides: stereo,
        });
    }
    pub(super) fn music_events(
        &mut self,
        now: f64,
        mut events: Vec<rustel_scheduler::Event>,
    ) -> Vec<rustel_scheduler::Event> {
        if self.music.link.is_none() {
            return events;
        }
        let generation = self.generation();
        let cycle = self.cycle_at_time(now);
        let cps = self.cps();
        let mut lanes = self.lanes.music_lanes();
        let source = self.active_source().unwrap_or_default().to_owned();
        let state = &mut self.music;
        if state.snapshot.source != source {
            state.snapshot.bindings = rustel_transpiler::setup_definitions(&source)
                .top_level_declarations.into_iter().filter(|d| d.kind == "const")
                .map(|d| music_link::Binding {
                    color: music_link::named_color(&d.name), name: d.name, from: d.from, to: d.to,
                    definition_from: d.definition_from, definition_to: d.definition_to,
                }).collect();
            state.snapshot.source = source;
        }
        if state.snapshot.generation != generation || cycle < state.snapshot.cycle {
            state.snapshot.events.clear();
            state.snapshot.rests.clear();
        }
        if state.snapshot.generation == generation {
            for lane in &mut lanes {
                if let Some(old) = state.snapshot.lanes.iter().find(|old| old.id == lane.id) { lane.parts = old.parts.clone(); }
            }
        }
        state.snapshot.generation = generation;
        state.snapshot.cycle = cycle;
        state.snapshot.cps = cps;
        state.snapshot.lanes = lanes;
        let exporting = cfg!(all(any(windows, target_os = "macos", target_os = "linux"), feature = "device-audio"))
            && std::env::var("RUSTEL_LANE_AUDIO").is_ok_and(|s| s == "1");
        if exporting {
            for lane in &state.snapshot.lanes {
                reserve_lane_slot(&mut state.lane_slots, &lane.id);
            }
        }
        state
            .offsets
            .retain(|id, _| state.snapshot.lanes.iter().any(|lane| &lane.id == id));
        let link = state.link.as_ref().expect("checked link");
        for command in link.commands() {
            // Other sessions share the topic and must not answer this request.
            if command.session != state.snapshot.session { continue; }
            let result = command.validate(&state.snapshot);
            if result.is_ok() {
                match command.solo {
                    Some(true) => state.snapshot.solo_lane = Some(command.lane.clone()),
                    Some(false) => {
                        if state.snapshot.solo_lane.as_deref() == Some(&command.lane) { state.snapshot.solo_lane = None; }
                    }
                    None => { state.offsets.insert(command.lane.clone(), command.semitones); }
                }
            }
            link.respond(Response {
                request: command.request,
                accepted: result.is_ok(),
                message: result.err().unwrap_or("applies to newly scheduled events").into(),
            });
        }
        if state
            .snapshot
            .solo_lane
            .as_ref()
            .is_some_and(|id| !state.snapshot.lanes.iter().any(|l| &l.id == id))
        {
            state.snapshot.solo_lane = None;
        }
        for lane in &mut state.snapshot.lanes {
            lane.transpose = state.offsets.get(&lane.id).copied().unwrap_or(0);
        }
        state
            .snapshot
            .events
            .retain(|event| event.cycle + event.duration >= cycle - 1.0);
        for event in &mut events {
            if exporting {
                let lane = event.telemetry.as_ref().and_then(|o| o.lane.as_deref());
                let slot = lane.and_then(|lane| state.lane_slots.get(lane)).copied().unwrap_or(15);
                if let Some(value) = controls(&event.value) {
                    let mut value = value.into_owned();
                    value.insert("orbit".into(), Value::F64(f64::from(slot)));
                    event.value = Value::Object(value);
                }
            }
            let Some(observation) = &event.telemetry else {
                continue;
            };
            let Some(lane) = observation.lane.as_deref() else {
                continue;
            };
            let offset = state.offsets.get(lane).copied().unwrap_or(0);
            if let Some(part) = observation.part.as_deref().filter(|name| !name.is_empty()) {
                if let Some(parent) = state.snapshot.lanes.iter_mut().find(|l| l.id == lane) {
                    if parent.parts.len() < 64 && !parent.parts.iter().any(|p| p.name == part) {
                        parent.parts.push(music_link::NamedPart { name: part.to_owned(), color: music_link::named_color(part) });
                        parent.parts.sort_by_key(|p| state.snapshot.bindings.iter().position(|b| b.name == p.name).unwrap_or(usize::MAX));
                    }
                }
            }
            let original = transpose(&mut event.value, offset);
            let Some(controls) = controls(&event.value) else {
                continue;
            };
            let mut captured = Event {
                block_label: String::new(),
                rest: false,
                part_color: music_link::named_color(observation.part.as_deref().unwrap_or_default()),
                part: observation.part.as_deref().unwrap_or_default().to_owned(),
                context: observation.context.clone(),
                controls: ["gain", "pan", "cutoff", "resonance"]
                    .into_iter()
                    .filter_map(|key| {
                        controls
                            .get(key)
                            .and_then(Value::as_f64)
                            .filter(|v| v.is_finite())
                            .map(|v| (key.to_owned(), v))
                    })
                    .collect(),
                id: observation.onset_id,
                lane: lane.to_owned(),
                cycle: observation.whole_begin.to_f64(),
                duration: observation.duration.to_f64(),
                original_pitch: original,
                pitch: original.map(|p| p + f64::from(offset)),
                sound: match controls.get("s") {
                    Some(Value::Str(sound)) => sound.clone(),
                    _ => String::new(),
                },
                gain: controls.get("gain").and_then(Value::as_f64).unwrap_or(0.8),
            };
            captured.block_label = captured.describe(&state.snapshot.source);
            state.snapshot.events.push(captured);
        }
        audition(&mut events, state.snapshot.solo_lane.as_deref());
        if state.snapshot.events.len() > 2048 {
            let discard = state.snapshot.events.len() - 2048;
            state.snapshot.events.drain(..discard);
        }
        if state.last_publish.elapsed() >= Duration::from_millis(50) {
            state.snapshot.rests = state.snapshot.inferred_rests();
            link.publish(state.snapshot.clone());
            state.last_publish = Instant::now();
        }
        events
    }
}

#[cfg(feature = "device-audio")]
struct MusicAudio {
    analyzer: crate::ui_analysis::UiAudioAnalyzer,
    samples: Box<[f32; rustel_audio::LIVE_ANALYSIS_WINDOW_SAMPLES]>,
    next: Instant,
    sequence: u64,
    last: Option<(u64, u64, u64)>,
}
#[cfg(feature = "device-audio")]
impl MusicAudio {
    fn new() -> Self {
        Self {
            analyzer: crate::ui_analysis::UiAudioAnalyzer::new(),
            samples: Box::new([0.0; rustel_audio::LIVE_ANALYSIS_WINDOW_SAMPLES]),
            next: Instant::now(),
            sequence: 0,
            last: None,
        }
    }
}

fn audition(events: &mut Vec<rustel_scheduler::Event>, solo: Option<&str>) {
    if let Some(lane) = solo {
        events
            .retain(|event| event.telemetry.as_ref().and_then(|t| t.lane.as_deref()) == Some(lane));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn exported_slots_survive_lane_removal_and_refuse_to_recycle_tails() {
        let mut slots=std::collections::BTreeMap::new();
        assert_eq!(super::reserve_lane_slot(&mut slots,"#0"),Some(0));
        assert_eq!(super::reserve_lane_slot(&mut slots,"#1"),Some(1));
        // A later score has only #1 and #2. The absent #0 keeps its tail slot.
        assert_eq!(super::reserve_lane_slot(&mut slots,"#1"),Some(1));
        assert_eq!(super::reserve_lane_slot(&mut slots,"#2"),Some(2));
        for i in 3..15 { assert_eq!(super::reserve_lane_slot(&mut slots,&format!("#{i}")),Some(i)); }
        assert_eq!(super::reserve_lane_slot(&mut slots,"#15"),None);
        assert_eq!(super::reserve_lane_slot(&mut slots,"#0"),Some(0));
    }
    use super::*;
    #[test]
    fn solo_filters_audio_events_and_restores_all_on_release() {
        let event = |id: u64, lane: &str| {
            let hap = rustel_core::Hap::new(
                Some(rustel_core::TimeSpan::new(Fraction::ZERO, Fraction::ONE)),
                rustel_core::TimeSpan::new(Fraction::ZERO, Fraction::ONE),
                Value::F64(60.0),
            )
            .with_lane_context(std::sync::Arc::from(lane));
            rustel_scheduler::Event {
                telemetry: rustel_scheduler::ScheduleTraceEvent::from_hap(&hap, 1, id, 0.0)
                    .map(Box::new),
                onset_id: id,
                generation: 1,
                whole_begin: Fraction::ZERO,
                whole_end: Fraction::ONE,
                duration: Fraction::ONE,
                target_time: 0.0,
                cps: None,
                value: hap.value,
                live_controls: [0; 4],
                ui_visuals: 0,
                log_line: None,
            }
        };
        let original = vec![event(1, "#0"), event(2, "#1"), event(3, "#2")];
        let mut selected = original.clone();
        audition(&mut selected, Some("#1"));
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].onset_id, 2);
        let mut moved = original.clone();
        audition(&mut moved, Some("#2"));
        assert_eq!(moved.len(), 1);
        assert_eq!(moved[0].onset_id, 3);
        let mut restored = original;
        audition(&mut restored, None);
        assert_eq!(restored.len(), 3);
    }
    #[test]
    fn lane_sources_keep_complete_multiline_chains() {
        std::thread::Builder::new().stack_size(crate::QUERY_WORKER_STACK_BYTES).spawn(|| {
            let mut session = Session::new().expect("session");
            session.evaluate("$: note(\"c3 e3\").s(\"sine\")\n  .every(4, x => x.rev())\n$: note(\"g2\").slow(2)").expect("score");
            let lanes = session.lanes.music_lanes();
            assert_eq!(lanes.len(), 2);
            assert!(lanes[0].source.starts_with("$: note(\"c3 e3\")"));
            assert!(lanes[0].source.contains(".every(4, x => x.rev())"));
            assert!(lanes[1].source.contains(".slow(2)"));
            assert_ne!(lanes[0].id, lanes[1].id);
        }).expect("worker").join().expect("source test");
    }
    #[test]
    fn observation_is_read_only_and_preserves_user_metadata() {
        let mut value = Value::object([
            ("note".into(), Value::Str("c3".into())),
            ("__rustel_context".into(), Value::Str("score-owned".into())),
            (
                "custom".into(),
                Value::List(vec![Value::F64(1.0), Value::Bool(true)]),
            ),
        ]);
        let before = value.clone();
        assert!(transpose(&mut value, 0).is_some());
        assert_eq!(value, before);
        transpose(&mut value, 7);
        let controls = controls(&value).expect("controls");
        assert_eq!(
            controls.get("__rustel_context").and_then(Value::as_str),
            Some("score-owned")
        );
    }

    #[test]
    fn transpose_preserves_other_controls_and_unpitched_samples() {
        let mut value = Value::object([
            ("note".into(), Value::Str("c3".into())),
            ("gain".into(), Value::F64(0.3)),
            ("s".into(), Value::Str("sawtooth".into())),
        ]);
        let original = transpose(&mut value, 7).expect("pitch");
        let Value::Object(controls) = &value else {
            panic!("controls");
        };
        assert_eq!(
            controls.get("note").and_then(Value::as_f64),
            Some(original + 7.0)
        );
        assert_eq!(controls.get("gain").and_then(Value::as_f64), Some(0.3));
        let mut drum = Value::object([("s".into(), Value::Str("bd".into()))]);
        let unchanged = drum.clone();
        assert!(transpose(&mut drum, 7).is_none());
        assert_eq!(drum, unchanged);
        let mut explicit = Value::object([
            ("note".into(), Value::Str("c3".into())),
            ("freq".into(), Value::F64(440.0)),
        ]);
        assert_eq!(transpose(&mut explicit, 12), Some(69.0));
        let Value::Object(controls) = explicit else {
            panic!("controls");
        };
        assert_eq!(controls.get("freq").and_then(Value::as_f64), Some(880.0));
    }
}
