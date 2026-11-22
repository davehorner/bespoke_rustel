//! Flat views stay on the page thread so a stopped GPU worker cannot stop them.
use crate::state::ViewerState;
use leptos::{html, prelude::*};
use std::f64::consts::TAU;
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{CanvasRenderingContext2d as Context, HtmlCanvasElement};

const COLORS: [&str; 8] = [
    "#1ff2d1", "#ff596b", "#a36bff", "#ffba33", "#33a6ff", "#ff59cc", "#9eff47", "#ff8a38",
];

#[component]
pub fn MusicCanvas(state: ViewerState) -> impl IntoView {
    let canvas = NodeRef::<html::Canvas>::new();
    let pan = RwSignal::new((0.0_f64, 0.0_f64));
    let zoom = RwSignal::new(1.0_f64);
    let drag = RwSignal::new(None::<(f64, f64)>);
    let gain = RwSignal::new(4.0_f64);
    Effect::new(move |_| {
        let Some(canvas) = canvas.get() else {
            return;
        };
        let Ok(Some(context)) = canvas.get_context("2d") else {
            return;
        };
        let Ok(context) = context.dyn_into::<Context>() else {
            return;
        };
        let canvas: HtmlCanvasElement = (*canvas).clone().unchecked_into();
        let mut bars = [0.0_f64; 64];
        let mut peaks = [0.0_f64; 64];
        let mut last_tick = js_sys::Date::now();
        let callback = Closure::<dyn FnMut()>::new(move || {
            let mode = state.music_mode.get_untracked();
            let now = js_sys::Date::now();
            let suspended = now - last_tick > 2000.0;
            last_tick = now;
            if suspended {
                state.render_heartbeat.set(now);
            }
            if mode == 0 {
                // A silent worker stop previously left a permanently frozen stage.
                if now - state.render_heartbeat.get_untracked()
                    > if state.ready.get_untracked() {
                        10000.0
                    } else {
                        30000.0
                    }
                {
                    state.render_error.set("3D renderer stopped responding. Switched to piano roll; reload to restart 3D.".into());
                    state.music_mode.set(1);
                }
                return;
            }
            // Keep the watchdog armed only while the worker is actually rendering.
            state.render_heartbeat.set(now);
            let rect = canvas.get_bounding_client_rect();
            let w = rect.width().max(1.0);
            let h = rect.height().max(1.0);
            let dpr = web_sys::window()
                .map(|w| w.device_pixel_ratio().clamp(1.0, 2.0))
                .unwrap_or(1.0);
            let pw = (w * dpr) as u32;
            let ph = (h * dpr) as u32;
            if canvas.width() != pw || canvas.height() != ph {
                canvas.set_width(pw);
                canvas.set_height(ph);
            }
            let _ = context.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
            color(&context, "#080e1b");
            context.fill_rect(0.0, 0.0, w, h);
            let top = 170.0;
            let bottom = (h - h * 0.22 - 80.0).max(top + 40.0);
            context.set_font("12px monospace");
            if (5..=7).contains(&mode) {
                let audio = state.music_audio.get_untracked();
                let valid = audio.as_ref().is_some_and(|a| {
                    now - state.music_audio_received.get_untracked() < 1000.0
                        && a.session == state.music_session.get_untracked()
                        && state
                            .music
                            .get_untracked()
                            .is_some_and(|s| s.generation == a.generation)
                });
                if valid {
                    draw_audio(
                        &context,
                        audio.as_ref().unwrap(),
                        mode,
                        gain.get_untracked(),
                        w,
                        top,
                        bottom,
                        &mut bars,
                        &mut peaks,
                    );
                } else {
                    label(
                        &context,
                        "Waiting for fresh master audio",
                        32.0,
                        top + 30.0,
                        "#91a8bd",
                    );
                }
                label(
                    &context,
                    "MASTER TAP / pre gain and limiter / display gain is visual only",
                    32.0,
                    bottom + 25.0,
                    "#91a8bd",
                );
                return;
            }
            let Some(snapshot) = state.music.get_untracked() else {
                label(&context, "Waiting for Rustel", 32.0, top + 20.0, "#91a8bd");
                return;
            };
            let age = ((now - state.music_received.get_untracked()) / 1000.0).max(0.0);
            if age >= 3.0 {
                label(
                    &context,
                    "Pattern stream is stale",
                    32.0,
                    top + 20.0,
                    "#91a8bd",
                );
                return;
            }
            let lane = state.music_lane.get_untracked();
            let snapshot = snapshot.visual_lanes(&state.music_expanded.get_untracked(), &lane);
            let cycle = snapshot.cycle + age * snapshot.cps;
            draw_events(
                &context,
                &snapshot,
                mode,
                cycle,
                w,
                top,
                bottom,
                pan.get_untracked(),
                zoom.get_untracked(),
                state.music_ghosts.get_untracked(),
            );
        });
        if let Some(window) = web_sys::window() {
            if let Ok(timer) = window.set_interval_with_callback_and_timeout_and_arguments_0(
                callback.as_ref().unchecked_ref(),
                33,
            ) {
                let _callback = StoredValue::new_local(callback);
                on_cleanup(move || {
                    if let Some(window) = web_sys::window() {
                        window.clear_interval_with_handle(timer);
                    }
                });
            }
        }
    });
    view! {
        <canvas node_ref=canvas aria-label="Music visualization"
            style=move || format!("position:fixed;inset:0;width:100vw;height:100vh;z-index:10;touch-action:none;display:{}", if state.music_mode.get() == 0 { "none" } else { "block" })
            on:pointerdown=move |e| { drag.set(Some((e.client_x() as f64, e.client_y() as f64))); if let Some(c) = canvas.get() { let _ = c.set_pointer_capture(e.pointer_id()); } }
            on:pointerup=move |_| drag.set(None)
            on:pointercancel=move |_| drag.set(None)
            on:pointermove=move |e| { if let Some((x,y)) = drag.get_untracked() { pan.update(|p| { p.0 += e.client_x() as f64-x; p.1 += e.client_y() as f64-y; }); drag.set(Some((e.client_x() as f64,e.client_y() as f64))); } }
            on:wheel=move |e| { e.prevent_default(); zoom.update(|z| *z = (*z * (-e.delta_y()*0.001).exp()).clamp(0.25,8.0)); }
            on:dblclick=move |_| { pan.set((0.0,0.0)); zoom.set(1.0); }
        ></canvas>
        <Show when=move || (5..=7).contains(&state.music_mode.get())>
            <label style="position:fixed;z-index:55;right:28px;top:94px;color:#91a8bd;font:11px monospace">
                "Display gain "
                <input aria-label="Display gain" type="range" min="0.25" max="32" step="0.25" prop:value=move || gain.get().to_string()
                    on:input=move |e| gain.set(event_target_value(&e).parse().unwrap_or(4.0)) />
            </label>
        </Show>
    }
}
fn color(c: &Context, value: &str) {
    c.set_fill_style_str(value);
    c.set_stroke_style_str(value);
}
fn event_color(event: &music_link::Event, fallback: &str) -> String {
    if event.part.is_empty() { return fallback.to_owned(); }
    let [r,g,b] = event.part_color;
    format!("#{r:02x}{g:02x}{b:02x}")
}
fn label(c: &Context, text: &str, x: f64, y: f64, value: &str) {
    color(c, value);
    let _ = c.fill_text(text, x, y);
}
fn line(c: &Context, a: (f64, f64), b: (f64, f64)) {
    c.begin_path();
    c.move_to(a.0, a.1);
    c.line_to(b.0, b.1);
    c.stroke();
}
fn dot(c: &Context, x: f64, y: f64, r: f64) {
    c.begin_path();
    let _ = c.arc(x, y, r, 0.0, TAU);
    c.fill();
}
fn draw_events(
    c: &Context,
    s: &music_link::Snapshot,
    mode: u8,
    cycle: f64,
    w: f64,
    top: f64,
    bottom: f64,
    pan: (f64, f64),
    zoom: f64,
    ghosts: bool,
) {
    c.save();
    c.begin_path();
    c.rect(20.0, top, w - 40.0, bottom - top);
    c.clip();
    let span = 2.0 / zoom;
    let x_at = |at: f64| 90.0 + (at - cycle + span * 0.75) / span * (w - 120.0) + pan.0;
    if mode == 3 || mode == 4 {
        draw_radial(c, s, mode, cycle, w, top, bottom, pan, zoom, ghosts);
        c.restore();
        label(
            c,
            if mode == 3 {
                "Time spiral / curved trails show duration / thin outlines show original pitch"
            } else {
                "Pitch wheel / angle is pitch class / radius separates octaves"
            },
            32.0,
            bottom + 24.0,
            "#91a8bd",
        );
        return;
    }
    if mode == 8 {
        let keys = ["gain", "pan", "cutoff", "resonance"];
        for (i, key) in keys.iter().enumerate() {
            let y = top + (bottom - top) * i as f64 / 4.0;
            let height = (bottom - top) / 4.0;
            let transform = |v: f64| if i == 2 { v.max(1.0).log10() } else { v };
            let mut low = 0.0_f64;
            let mut high = 1.0_f64;
            for event in &s.events {
                if let Some(v) = event.controls.get(*key).filter(|v| v.is_finite()) {
                    low = low.min(transform(*v));
                    high = high.max(transform(*v));
                }
            }
            label(
                c,
                &format!("{}{}", key, if i == 2 { " log Hz" } else { "" }),
                24.0,
                y + 16.0,
                "#91a8bd",
            );
            for (lane_index, lane) in s.lanes.iter().take(320).enumerate() {
                let mut events: Vec<_> = s
                    .events
                    .iter()
                    .filter(|e| e.lane == lane.id && e.controls.contains_key(*key))
                    .collect();
                events.sort_by(|a, b| a.cycle.total_cmp(&b.cycle));
                color(c, COLORS[lane_index % 8]);
                let mut previous = None;
                for event in events.iter().take(1024) {
                    let value = transform(event.controls[*key]);
                    if !value.is_finite() {
                        continue;
                    }
                    let x = x_at(event.cycle);
                    let ey = y + height - 8.0 - (value - low) / (high - low) * (height - 22.0);
                    if let Some((px, py)) = previous {
                        line(c, (px, py), (x, py));
                        line(c, (x, py), (x, ey));
                    }
                    dot(c, x, ey, 2.0);
                    previous = Some((x, ey));
                }
            }
        }
        c.restore();
        label(
            c,
            "Dots: scheduled onset values / cutoff uses log scale",
            32.0,
            bottom + 24.0,
            "#91a8bd",
        );
        return;
    }
    if mode == 1 || mode == 2 {
        color(c, "#1b2b40");
        for beat in -12..=12 {
            let x = x_at(((cycle * 4.0).floor() + beat as f64) / 4.0);
            line(c, (x, top), (x, bottom));
        }
        color(c, "#d7eaff");
        line(c, (x_at(cycle), top), (x_at(cycle), bottom));
    }
    for (lane_index, lane) in s.lanes.iter().take(320).enumerate() {
        let row_height = ((bottom-top) / s.lanes.len().max(1) as f64).max(64.0) * zoom;
        if mode == 1 {
            label(c, &lane.label, 24.0, top + lane_index as f64 * row_height + pan.1 + 12.0, "#b2c9df");
            for note in (24..=96).step_by(12) {
                let y = top + (lane_index+1) as f64 * row_height - (note as f64-24.0)/72.0 * (row_height-18.0).max(1.0) + pan.1;
                color(c, "#1b2b40"); line(c, (20.0,y), (w-20.0,y));
            }
            color(c, "#26394d");
            line(c, (20.0, top + lane_index as f64 * row_height + pan.1), (w-20.0, top + lane_index as f64 * row_height + pan.1));
        } else if mode == 2 {
            let column = ((w-90.0) / s.lanes.len().max(1) as f64).max(150.0);
            label(c, &lane.label, 45.0 + lane_index as f64 * column + pan.0, top+14.0, "#b2c9df");
        }
        for event in s
            .events
            .iter()
            .filter(|e| {
                e.lane == lane.id && e.cycle + e.duration >= cycle - 1.0 && e.cycle <= cycle + 2.0
            })
            .take(128)
        {
            let pitch = event.pitch.unwrap_or(48.0);
            if !pitch.is_finite() {
                continue;
            }
            let active = event.cycle <= cycle && event.cycle + event.duration > cycle;
            let tint = event_color(event, COLORS[lane_index % 8]);
            color(c, &tint);
            c.set_global_alpha(if active { 1.0 } else { 0.55 });
            if mode == 1 {
                let y = top + (lane_index + 1) as f64 * row_height - (pitch - 24.0) / 72.0 * (row_height-18.0).max(1.0) + pan.1;
                let length = (event.duration / span * (w - 120.0)).clamp(3.0, w);
                if ghosts {
                    if let Some(original) = event.original_pitch.filter(|p| p.is_finite()) {
                        c.set_global_alpha(0.25);
                        c.fill_rect(
                            x_at(event.cycle),
                            top + (lane_index + 1) as f64 * row_height - (original - 24.0) / 72.0 * (row_height-18.0).max(1.0) + pan.1 - 1.0,
                            length,
                            2.0,
                        );
                        c.set_global_alpha(if active { 1.0 } else { 0.55 });
                    }
                }
                let height = if event.part.is_empty() { 8.0 } else { 14.0 };
                c.fill_rect(x_at(event.cycle), y - height * 0.5, length, height);
                if !event.part.is_empty() {
                    c.save();
                    label(c, &event.part, x_at(event.cycle) + 2.0, y - 9.0, &tint);
                    c.restore();
                }
            } else if mode == 2 {
                let column = ((w - 90.0) / s.lanes.len().max(1) as f64).max(150.0);
                let x = 45.0 + lane_index as f64 * column + pan.0;
                let y = bottom - (event.cycle - cycle + 0.5) * 120.0 * zoom + pan.1;
                let sound = if event.sound.is_empty() {
                    "note"
                } else {
                    &event.sound
                };
                let _ = c.fill_text(&format!("{} {} {:.0}", event.part, sound, pitch), x, y);
            }
        }
    }
    c.set_global_alpha(1.0);
    c.restore();
}
fn draw_radial(
    c: &Context,
    s: &music_link::Snapshot,
    mode: u8,
    cycle: f64,
    w: f64,
    top: f64,
    bottom: f64,
    pan: (f64, f64),
    zoom: f64,
    ghosts: bool,
) {
    let mut low = 127.0_f64;
    let mut high = 0.0_f64;
    let visible = |e: &&music_link::Event| {
        e.cycle.is_finite()
            && e.duration.is_finite()
            && e.cycle + e.duration >= cycle - 1.5
            && e.cycle <= cycle + 0.5
            && s.lanes.iter().any(|l| l.id == e.lane)
    };
    for e in s.events.iter().filter(visible) {
        let p = e.pitch.unwrap_or(48.0);
        if !p.is_finite() {
            continue;
        }
        let original = if ghosts {
            e.original_pitch.filter(|p| p.is_finite()).unwrap_or(p)
        } else {
            p
        };
        low = low.min(p.min(original));
        high = high.max(p.max(original));
    }
    if low > high {
        low = 36.0;
        high = 84.0;
    }
    low = (low / 12.0).floor().clamp(-2.0, 10.0) * 12.0;
    high = ((high + 1.0) / 12.0).ceil() * 12.0;
    high = high.max(low + 24.0).clamp(low + 24.0, 144.0);
    let width = w - 76.0;
    let height = bottom - top - 26.0;
    let center = (52.0 + width * 0.5, top + 12.0 + height * 0.5);
    let radius = width.min(height) * 0.43;
    let pivot = (w * 0.5, top + (bottom - top) * 0.65);
    c.save();
    let _ = c.translate(pivot.0 + pan.0, pivot.1 + pan.1);
    let _ = c.scale(zoom, zoom);
    let _ = c.translate(-pivot.0, -pivot.1);
    let point = |time: f64, pitch: f64| {
        let p = if mode == 3 {
            crate::music_geometry::spiral(time - cycle, pitch, low, high, radius)
        } else {
            crate::music_geometry::wheel(pitch, low, high, radius)
        };
        (center.0 + p.0, center.1 + p.1)
    };
    c.set_line_width(1.0);
    color(c, "#374f6e");
    if mode == 3 {
        c.begin_path();
        for i in 0..=160 {
            let p = point(cycle - 1.5 + i as f64 / 80.0, low);
            if i == 0 {
                c.move_to(p.0, p.1);
            } else {
                c.line_to(p.0, p.1);
            }
        }
        c.stroke();
        let p = point(cycle, low);
        color(c, "#d2f0ff");
        dot(c, p.0, p.1, 5.0);
    } else {
        for ring in 0..4 {
            c.begin_path();
            let _ = c.arc(
                center.0,
                center.1,
                radius * (0.3 + ring as f64 * 0.22),
                0.0,
                TAU,
            );
            c.stroke();
        }
        for pitch in 0..12 {
            let a = pitch as f64 / 12.0 * TAU - TAU * 0.25;
            color(c, "#28415a");
            line(
                c,
                center,
                (center.0 + a.cos() * radius, center.1 + a.sin() * radius),
            );
            label(
                c,
                [
                    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
                ][pitch],
                center.0 + a.cos() * (radius + 15.0) - 5.0,
                center.1 + a.sin() * (radius + 15.0) + 3.0,
                "#91afc8",
            );
        }
    }
    for event in s.events.iter().filter(visible).take(1024) {
        let Some(lane) = s.lanes.iter().position(|l| l.id == event.lane) else {
            continue;
        };
        let sounding = event.cycle <= cycle && event.cycle + event.duration > cycle;
        let elapsed = (cycle - event.cycle) / s.cps.max(0.001);
        let hit = if elapsed >= 0.0 {
            (-elapsed * 7.0).exp()
        } else {
            0.0
        };
        for source in [true, false] {
            if source && !ghosts {
                continue;
            }
            let pitch = if source {
                event.original_pitch
            } else {
                event.pitch
            }
            .unwrap_or(48.0);
            if !pitch.is_finite() {
                continue;
            }
            let tint = event_color(event, COLORS[lane % 8]);
            color(c, &tint);
            c.set_global_alpha(if source {
                65.0 / 255.0
            } else if sounding {
                1.0
            } else {
                150.0 / 255.0
            });
            let p = point(event.cycle, pitch);
            if !source && !event.part.is_empty() {
                label(c, &event.part, p.0 + 6.0, p.1 - 6.0, &tint);
            }
            if mode == 3 {
                c.set_line_width(if source { 0.7 } else { 2.0 });
                c.begin_path();
                c.move_to(p.0, p.1);
                let duration = event.duration.clamp(0.01, 0.5);
                for i in 1..=12 {
                    let q = point(event.cycle + duration * i as f64 / 12.0, pitch);
                    c.line_to(q.0, q.1);
                }
                c.stroke();
            } else if sounding && !source {
                c.set_line_width(1.5);
                line(c, center, p);
            }
            let size = if source {
                if mode == 3 { 2.0 } else { 3.0 }
            } else if mode == 3 {
                3.0 + hit * 5.0
            } else {
                3.0 + hit * 7.0 + event.gain.clamp(0.0, 1.0) * 2.0
            };
            c.begin_path();
            let _ = c.arc(p.0, p.1, size, 0.0, TAU);
            if source {
                c.set_line_width(1.2);
                c.stroke();
            } else {
                c.fill();
            }
        }
    }
    c.restore();
}
fn draw_audio(
    c: &Context,
    a: &music_link::AudioFrame,
    mode: u8,
    gain: f64,
    w: f64,
    top: f64,
    bottom: f64,
    bars: &mut [f64; 64],
    peaks: &mut [f64; 64],
) {
    let h = bottom - top;
    let mid = (top + bottom) * 0.5;
    color(c, "#24364f");
    line(c, (32.0, mid), (w - 32.0, mid));
    c.save();
    c.begin_path();
    c.rect(32.0, top, w - 64.0, h);
    c.clip();
    if mode == 5 {
        let trigger = (1..a.scope.len() / 2)
            .find(|&i| a.scope[i - 1] <= 0.0 && a.scope[i] > 0.0)
            .unwrap_or(0);
        color(c, "#1ff2d1");
        c.set_line_width(2.0);
        c.begin_path();
        for (i, v) in a.scope.iter().skip(trigger).take(256).enumerate() {
            let x = 32.0 + i as f64 / 255.0 * (w - 64.0);
            let y = mid - *v as f64 * gain * h * 0.45;
            if i == 0 {
                c.move_to(x, y);
            } else {
                c.line_to(x, y);
            }
        }
        c.stroke();
    } else if mode == 6 {
        let bins = a.spectrum.len();
        let nyquist = a.sample_rate as f64 / 2.0;
        for i in 0..64 {
            let low = 20.0 * (nyquist / 20.0).powf(i as f64 / 64.0);
            let high = 20.0 * (nyquist / 20.0).powf((i + 1) as f64 / 64.0);
            let first = ((low / nyquist * bins as f64) as usize).min(bins - 1);
            let last = ((high / nyquist * bins as f64) as usize)
                .max(first + 1)
                .min(bins);
            let db = a.spectrum[first..last]
                .iter()
                .copied()
                .fold(-120.0_f32, f32::max) as f64;
            let target = ((db + gain.log10() * 20.0 + 90.0) / 90.0).clamp(0.0, 1.0);
            bars[i] += (target - bars[i]) * if target > bars[i] { 0.65 } else { 0.12 };
            peaks[i] = (peaks[i] - 0.005).max(bars[i]);
            let bw = (w - 64.0) / 64.0;
            color(c, COLORS[i / 8]);
            c.fill_rect(
                32.0 + i as f64 * bw,
                bottom - bars[i] * h,
                bw - 2.0,
                bars[i] * h,
            );
            color(c, "#d7eaff");
            c.fill_rect(32.0 + i as f64 * bw, bottom - peaks[i] * h, bw - 2.0, 2.0);
        }
    } else {
        let radius = h.min(w - 64.0) * 0.45;
        let cx = w * 0.5;
        color(c, "#24364f");
        line(c, (cx - radius, mid - radius), (cx + radius, mid + radius));
        line(c, (cx - radius, mid + radius), (cx + radius, mid - radius));
        color(c, "#1ff2d1");
        c.set_global_alpha(0.45);
        let mut cross = 0.0;
        let mut l2 = 0.0;
        let mut r2 = 0.0;
        for [l, r] in &a.sides {
            let l = *l as f64;
            let r = *r as f64;
            dot(
                c,
                cx + (l - r) * 0.5 * gain * radius,
                mid - (l + r) * 0.5 * gain * radius,
                1.1,
            );
            cross += l * r;
            l2 += l * l;
            r2 += r * r;
        }
        c.set_global_alpha(1.0);
        label(
            c,
            &format!(
                "Stereo correlation {:.2}",
                cross / (l2 * r2).sqrt().max(1e-12)
            ),
            40.0,
            top + 18.0,
            "#91a8bd",
        );
    }
    c.restore();
    label(
        c,
        &format!("{} Hz / frame {}", a.sample_rate, a.end_frame),
        32.0,
        top - 12.0,
        "#91a8bd",
    );
}
