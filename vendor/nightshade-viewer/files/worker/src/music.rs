use nightshade::ecs::material::components::Material;
use nightshade::prelude::*;
use protocol::music_link_types::Snapshot;

pub const PALETTE: [[f32; 3]; 8] = [
    [0.12, 0.95, 0.82],
    [1.0, 0.35, 0.42],
    [0.64, 0.42, 1.0],
    [1.0, 0.73, 0.2],
    [0.2, 0.65, 1.0],
    [1.0, 0.35, 0.8],
    [0.62, 1.0, 0.28],
    [1.0, 0.54, 0.22],
];

#[derive(Default)]
struct LaneScene {
    blocks: Vec<Entity>,
    labels: Vec<(Entity, String)>,
    colors: Vec<[f32; 3]>,
    rail: Option<Entity>,
    lamp: Option<Entity>,
    halo: Option<Entity>,
    heading: Option<(Entity, String)>,
    tint: Option<[f32; 3]>,
}

pub struct MusicScene {
    snapshot: Option<Snapshot>,
    projection: Option<Snapshot>,
    expanded: Vec<String>,
    received: f64,
    lanes: Vec<LaneScene>,
    beats: Vec<Entity>,
    playhead: Option<Entity>,
    mode: u8,
    lane_filter: String,
    orbit: bool,
    ghosts: bool,
    framed_lanes: usize,
}
impl Default for MusicScene {
    fn default() -> Self {
        Self {
            snapshot: None,
            projection: None,
            expanded: Vec::new(),
            received: 0.0,
            lanes: Vec::new(),
            beats: Vec::new(),
            playhead: None,
            mode: 0,
            lane_filter: String::new(),
            orbit: false,
            ghosts: false,
            framed_lanes: 0,
        }
    }
}

fn pose(world: &mut World, entity: Entity, position: Vec3, scale: Vec3) {
    let hidden = scale.x == 0.0 || scale.y == 0.0 || scale.z == 0.0;
    let position = if hidden {
        Vec3::repeat(-10000.0)
    } else {
        position
    };
    let scale = if hidden { Vec3::repeat(0.0001) } else { scale };
    if let Some(transform) = world.core.get_local_transform_mut(entity) {
        transform.translation = position;
        transform.scale = scale;
    }
    mark_local_transform_dirty(world, entity);
}
fn material(world: &mut World, entity: Entity, name: String, color: [f32; 3], glow: f32) {
    spawn_material(
        world,
        entity,
        name,
        Material {
            base_color: [color[0], color[1], color[2], 1.0],
            emissive_factor: color,
            emissive_strength: glow,
            metallic: 0.35,
            roughness: 0.3,
            ..Default::default()
        },
    );
}
impl MusicScene {
    pub fn active(&self) -> bool {
        self.snapshot.is_some()
    }
    pub fn paused(&self) -> bool {
        self.active() && self.mode != 0
    }
    pub fn configure(&mut self, orbit: bool, ghosts: bool, mode: u8, lane: String, expanded: Vec<String>) {
        self.lane_filter = lane;
        self.expanded = expanded;
        if let Some(snapshot) = &self.snapshot { self.projection = Some(snapshot.visual_lanes(&self.expanded, &self.lane_filter)); }
        self.mode = mode.min(8);
        self.orbit = orbit;
        self.ghosts = ghosts;
    }
    pub fn receive(&mut self, snapshot: Snapshot, world: &mut World) {
        if snapshot.version != 1
            || snapshot.events.len() > 2048
            || snapshot.lanes.len() > 64
            || !snapshot.cycle.is_finite()
            || !snapshot.cps.is_finite()
            || snapshot.cps <= 0.0
        {
            return;
        }
        if self.playhead.is_none() {
            world.resources.render_settings.atmosphere = Atmosphere::None;
            world.resources.render_settings.clear_color = [0.008, 0.012, 0.025, 1.0];
            world.resources.render_settings.bloom_enabled = true;
            world.resources.render_settings.bloom_intensity = 0.22;
            world.resources.debug_draw.show_grid = false;
            let entity = spawn_cube_at(world, Vec3::zeros());
            material(world, entity, "MusicPlayhead".into(), [0.7, 0.85, 1.0], 2.0);
            self.playhead = Some(entity);
            // Persistent beat-grid geometry; only transforms move each frame.
            for beat in 0..17 {
                let entity = spawn_cube_at(world, Vec3::zeros());
                material(
                    world,
                    entity,
                    format!("MusicBeat{beat}"),
                    [0.06, 0.12, 0.2],
                    0.35,
                );
                self.beats.push(entity);
            }
        }
        self.received = js_sys::Date::now();
        let mut snapshot = snapshot;
        snapshot.events.retain(|e| {
            e.cycle.is_finite()
                && e.duration.is_finite()
                && e.duration >= 0.0
                && e.gain.is_finite()
                && e.pitch
                    .is_none_or(|p| p.is_finite() && (-256.0..=256.0).contains(&p))
                && e.original_pitch
                    .is_none_or(|p| p.is_finite() && (-256.0..=256.0).contains(&p))
        });
        self.projection = Some(snapshot.visual_lanes(&self.expanded, &self.lane_filter));
        self.snapshot = Some(snapshot);
    }
    pub fn update(&mut self, world: &mut World) {
        let Some(snapshot) = &self.projection else {
            return;
        };
        let age = ((js_sys::Date::now() - self.received) / 1000.0).max(0.0);
        let cycle = snapshot.cycle + age.min(3.0) * snapshot.cps;
        let visible: Vec<_> = snapshot
            .lanes
            .iter()
            .take(320)
            .collect();
        let count = visible.len();
        if self.framed_lanes != count {
            self.framed_lanes = count;
            if let Some(camera) = world.resources.active_camera {
                if let Some(orbit) = world.core.get_pan_orbit_camera_mut(camera) {
                    orbit.target_focus = Vec3::new(-1.0, 1.0, count.saturating_sub(1) as f32 * 0.85);
                    orbit.target_radius = (12.0 + count as f32 * 1.05).max(15.0);
                }
            }
        }
        let depth = count.max(1) as f32 * 1.7;
        let beat_pulse = (1.0 - ((cycle * 4.0).fract() as f32 * 5.0)).max(0.0);
        if self.orbit {
            let delta = world.resources.window.timing.delta_time;
            if let Some(camera) = world.resources.active_camera {
                if let Some(orbit) = world.core.get_pan_orbit_camera_mut(camera) {
                    orbit.target_yaw += delta * 0.055;
                }
            }
        }
        while self.lanes.len() < count {
            let i = self.lanes.len();
            let color = PALETTE[i % PALETTE.len()];
            let rail = spawn_cube_at(world, Vec3::zeros());
            material(
                world,
                rail,
                format!("MusicRail{i}"),
                color.map(|v| v * 0.18),
                0.3,
            );
            let lamp = spawn_sphere_at(world, Vec3::zeros());
            material(world, lamp, format!("MusicLamp{i}"), color, 2.5);
            let halo = spawn_torus_at(world, Vec3::zeros());
            material(world, halo, format!("MusicHalo{i}"), color, 1.4);
            let heading = spawn_3d_billboard_text_with_properties(world, "", Vec3::zeros(), TextProperties { font_size: 16.0, ..Default::default() });
            self.lanes.push(LaneScene {
                heading: Some((heading, String::new())),
                rail: Some(rail),
                lamp: Some(lamp),
                halo: Some(halo),
                ..Default::default()
            });
        }
        let mut total_used = 0;
        let mut total_allocated: usize = self.lanes.iter().map(|l| l.blocks.len()).sum();
        for (index, lane) in self.lanes.iter_mut().enumerate() {
            let z = index as f32 * 1.7;
            if let Some(row) = visible.get(index) {
                let tint = if row.id.starts_with('~') { protocol::music_link_types::named_color(&row.source).map(|v| f32::from(v)/255.0) } else { PALETTE[index % 8] };
                if lane.tint != Some(tint) {
                    for (entity, brightness, glow) in [(lane.rail, 0.18, 0.3), (lane.lamp, 1.0, 2.5), (lane.halo, 1.0, 1.4)] {
                        if let Some(entity) = entity {
                            let color = tint.map(|v| v * brightness);
                            nightshade::ecs::world::commands::set_material_with_textures(world, entity, Material {
                                base_color: [color[0], color[1], color[2], 1.0], emissive_factor: color,
                                emissive_strength: glow, metallic: 0.35, roughness: 0.3, ..Default::default()
                            });
                        }
                    }
                    if let Some((entity, _)) = &lane.heading {
                        if let Some(text) = world.core.get_text_mut(*entity) { text.set_color(Vec4::new(tint[0], tint[1], tint[2], 1.0)); }
                    }
                    lane.tint = Some(tint);
                }
            }
            let mut used = 0;
            let mut pulse = 0.0_f32;
            if index < count && age < 3.0 {
                let id = &visible[index].id;
                for event in snapshot.events.iter().filter(|e| {
                    &e.lane == id && e.cycle + e.duration > cycle - 1.5 && e.cycle < cycle + 2.0
                }) {
                    let elapsed = ((cycle - event.cycle) / snapshot.cps.max(0.001)) as f32;
                    let hit = if elapsed >= 0.0 {
                        (-elapsed * 7.0).exp()
                    } else {
                        0.0
                    };
                    pulse = pulse.max(hit);
                    for original in [true, false] {
                        if original && !self.ghosts {
                            continue;
                        }
                        if used >= 128 || total_used >= 1024 {
                            break;
                        }
                        if lane.blocks.len() <= used {
                            if total_allocated >= 1024 {
                                break;
                            }
                            total_allocated += 1;
                            let entity = spawn_cube_at(world, Vec3::zeros());
                            // Fixed per-lane pools keep colors stable as events retire.
                            material(
                                world,
                                entity,
                                format!("MusicBlock{index}_{used}"),
                                PALETTE[index % 8],
                                1.1,
                            );
                            lane.blocks.push(entity);
                            lane.colors.push(PALETTE[index % 8]);
                            let label = spawn_3d_billboard_text_with_properties(world, "", Vec3::zeros(), TextProperties {
                                font_size: 16.0, outline_width: 0.12,
                                alignment: TextAlignment::Center,
                                ..Default::default()
                            });
                            lane.labels.push((label, String::new()));
                        }
                        let entity = lane.blocks[used];
                        let tint = if event.part.is_empty() { PALETTE[index % 8] } else { event.part_color.map(|v| f32::from(v) / 255.0) };
                        if lane.colors[used] != tint {
                            // Mutate this pool slot's material; never allocate per event/frame.
                            nightshade::ecs::world::commands::set_material_with_textures(world, entity, Material {
                                base_color: [tint[0], tint[1], tint[2], 1.0],
                                emissive_factor: tint, emissive_strength: 1.1,
                                metallic: 0.35, roughness: 0.3, ..Default::default()
                            });
                            lane.colors[used] = tint;
                        }
                        let pitch = if original {
                            event.original_pitch
                        } else {
                            event.pitch
                        }
                        .unwrap_or(48.0);
                        let length = event.duration.clamp(0.012, 3.0) as f32 * 5.0;
                        let y = (pitch as f32 - 24.0) * 0.065 + 0.3;
                        let lift = if original { 0.0 } else { hit * 0.35 };
                        let thickness = if original { 0.035 } else { 0.12 + hit * 0.2 };
                        pose(
                            world,
                            entity,
                            Vec3::new(
                                (event.cycle - cycle) as f32 * 5.0 + length * 0.5,
                                y + lift,
                                z + if original { -0.25 } else { 0.15 },
                            ),
                            Vec3::new(
                                length,
                                thickness,
                                if original { 0.055 } else { 0.22 + hit * 0.15 },
                            ),
                        );
                        let (label, name) = &mut lane.labels[used];
                        let display = if original { String::new() } else { event.part.chars().take(64).collect::<String>() };
                        if *name != display {
                            if let Some(text) = world.core.get_text(*label) {
                                world.resources.text.cache.set_text(text.text_index, &display);
                            }
                            if let Some(text) = world.core.get_text_mut(*label) { text.dirty = true; }
                            *name = display;
                        }
                        pose(world, *label, Vec3::new((event.cycle-cycle) as f32 * 5.0 + length * 0.5, y + lift + 0.35, z + 0.15),
                            if name.is_empty() { Vec3::zeros() } else { Vec3::repeat(0.012) });
                        used += 1;
                        total_used += 1;
                    }
                }
            }
            for entity in &lane.blocks[used..] {
                pose(world, *entity, Vec3::zeros(), Vec3::zeros());
            }
            for (entity, _) in &lane.labels[used..] { pose(world, *entity, Vec3::zeros(), Vec3::zeros()); }
            if let Some((entity, text)) = &mut lane.heading {
                let label = visible.get(index).map_or("", |l| l.label.as_str());
                if text != label {
                    if let Some(t) = world.core.get_text(*entity) { world.resources.text.cache.set_text(t.text_index, label); }
                    if let Some(t) = world.core.get_text_mut(*entity) { t.dirty = true; }
                    *text = label.to_owned();
                }
                pose(world, *entity, Vec3::new(-7.5, 0.25, z), if index < count { Vec3::repeat(0.014) } else { Vec3::zeros() });
            }
            let visible = index < count;
            if let Some(rail) = lane.rail {
                pose(
                    world,
                    rail,
                    Vec3::new(-1.5, -0.08, z),
                    if visible {
                        Vec3::new(15.0, 0.045, 0.75)
                    } else {
                        Vec3::zeros()
                    },
                );
            }
            if let Some(lamp) = lane.lamp {
                pose(
                    world,
                    lamp,
                    Vec3::new(0.0, 0.15, z),
                    if visible {
                        Vec3::repeat(0.16 + pulse * 0.28)
                    } else {
                        Vec3::zeros()
                    },
                );
            }
            if let Some(halo) = lane.halo {
                pose(
                    world,
                    halo,
                    Vec3::new(0.0, 0.02, z),
                    if visible {
                        Vec3::new(0.45 + pulse * 0.65, 0.025, 0.45 + pulse * 0.65)
                    } else {
                        Vec3::zeros()
                    },
                );
            }
        }
        for (i, entity) in self.beats.iter().enumerate() {
            let beat = (cycle * 4.0).floor() - 8.0 + i as f64;
            pose(
                world,
                *entity,
                Vec3::new(
                    (beat / 4.0 - cycle) as f32 * 5.0,
                    -0.12,
                    (depth - 1.7) * 0.5,
                ),
                Vec3::new(
                    if (beat as i64).rem_euclid(4) == 0 {
                        0.035
                    } else {
                        0.012
                    },
                    0.025,
                    depth,
                ),
            );
        }
        if let Some(entity) = self.playhead {
            // A low luminous bar leaves the note field unobscured.
            pose(
                world,
                entity,
                Vec3::new(0.0, -0.04, (depth - 1.7) * 0.5),
                Vec3::new(0.035 + beat_pulse * 0.025, 0.08, depth),
            );
        }
    }
}
