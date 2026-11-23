use crate::{
    bridge::{Bridge, send},
    state::ViewerState,
};
use leptos::prelude::*;
use protocol::ClientMessage;
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{BinaryType, MessageEvent, WebSocket};

thread_local! {
    static REQUESTS: std::cell::RefCell<std::collections::HashSet<String>> = std::cell::RefCell::new(std::collections::HashSet::new());
    static CONNECTION: std::cell::RefCell<Option<MusicSocket>> = const { std::cell::RefCell::new(None) };
}
struct MusicSocket {
    socket: WebSocket,
    _open: Closure<dyn FnMut()>,
    _message: Closure<dyn FnMut(MessageEvent)>,
    _close: Closure<dyn FnMut()>,
}
impl Drop for MusicSocket {
    fn drop(&mut self) {
        self.socket.set_onopen(None);
        self.socket.set_onmessage(None);
        self.socket.set_onclose(None);
        let _ = self.socket.close();
    }
}

pub fn solo(state: ViewerState, lane: String, enabled: bool) {
    let Some(snapshot) = state.music.get_untracked() else { return; };
    if lane.is_empty() || snapshot.session != state.music_session.get_untracked() { return; }
    let request = format!("viewer-{}-{}", js_sys::Date::now(), js_sys::Math::random());
    let command = serde_json::json!({"request":request,"session":snapshot.session,"generation":snapshot.generation,"lane":lane,"solo":enabled});
    let sent = CONNECTION.with(|slot| {
        slot.borrow().as_ref().is_some_and(|connection| {
            hearsay::encode_publish_text(music_link::COMMAND_TOPIC, &command.to_string(), false)
                .is_ok_and(|frame| connection.socket.send_with_u8_array(&frame).is_ok())
        })
    });
    if sent {
        REQUESTS.with(|requests| { let mut requests=requests.borrow_mut(); if requests.len() >= 64 { requests.clear(); } requests.insert(request); });
        state.music_control_status.set("Applying solo…".into());
    } else { state.music_control_status.set("Solo command could not reach Rustel".into()); }
}

pub fn transport(state: ViewerState, action: music_link::TransportCommand) {
    let Some(snapshot)=state.music.get_untracked().filter(|s|s.transport.is_some()) else{return};
    let request=format!("transport-{}-{}",js_sys::Date::now(),js_sys::Math::random());
    let command=serde_json::json!({"request":request,"session":snapshot.session,"generation":snapshot.generation,"lane":"","transport":action});
    let sent=CONNECTION.with(|slot|slot.borrow().as_ref().is_some_and(|c|
        hearsay::encode_publish_text(music_link::COMMAND_TOPIC,&command.to_string(),false).is_ok_and(|frame|c.socket.send_with_u8_array(&frame).is_ok())));
    if sent {
        REQUESTS.with(|r|{let mut r=r.borrow_mut();if r.len()>=64{r.clear();}r.insert(request);});
        state.music_control_status.set("Applying transport…".into());
    } else {state.music_control_status.set("Transport could not reach Rustel".into());}
}

fn select_demo(state:ViewerState,name:String) {
    let Some(snapshot)=state.music.get_untracked() else{return};
    if !snapshot.demos.contains(&name){return;}
    let request=format!("demo-{}-{}",js_sys::Date::now(),js_sys::Math::random());
    let command=serde_json::json!({"request":request,"session":snapshot.session,"generation":snapshot.generation,"lane":"","demo":name});
    let sent=CONNECTION.with(|slot|slot.borrow().as_ref().is_some_and(|c|
        hearsay::encode_publish_text(music_link::COMMAND_TOPIC,&command.to_string(),false).is_ok_and(|frame|c.socket.send_with_u8_array(&frame).is_ok())));
    if sent {REQUESTS.with(|r|{r.borrow_mut().insert(request);});state.music_control_status.set("Loading demo…".into());}
    else {state.music_control_status.set("Demo command could not reach Rustel".into());}
}

fn skip_cycles(state: ViewerState, delta:f64) {
    if let Some(s)=state.music.get_untracked() {
        let age=(js_sys::Date::now()-state.music_received.get_untracked())/1000.0;
        transport(state,music_link::TransportCommand::Seek {cycle:(s.display_cycle(age)+delta).clamp(0.0,1_000_000.0)});
    }
}
fn solo_target(state: ViewerState) -> String {
    let selected=state.music_source_focus.with(|focus|focus.as_ref().map(|f|f.0.clone()).unwrap_or_default());
    let filter=state.music_lane.get();
    let session=state.music_session.get();
    state.music.with(|s| {
        let Some(s)=s.as_ref().filter(|s|s.session==session) else {return String::new()};
        [selected,filter].into_iter().find(|id|!id.is_empty()&&s.lanes.iter().any(|l|l.id==*id)).unwrap_or_default()
    })
}
fn active_solo(state: ViewerState) -> Option<String> {
    let session=state.music_session.get();
    state.music.with(|s|s.as_ref().filter(|s|s.session==session).and_then(|s|s.solo_lane.clone()))
}
fn selected_is_solo(state: ViewerState) -> bool {
    active_solo(state).is_some()
}

pub fn enabled() -> bool {
    web_sys::window()
        .and_then(|w| w.location().search().ok())
        .is_some_and(|s| s.trim_start_matches('?').split('&').any(|p| p == "music=1"))
}

pub fn toggle_parts(state: ViewerState, lane: &str) {
    if !state.music.with_untracked(|s| s.as_ref().is_some_and(|s| s.lanes.iter().any(|l| l.id == lane && l.parts.iter().any(|p|!p.helper)))) { return; }
    state.music_expanded.update(|ids| {
        if ids.iter().any(|id| id == lane) { ids.retain(|id| id != lane); }
        else { ids.push(lane.to_owned()); }
    });
}

pub fn reveal_source(state: ViewerState, lane: &str, part: &str) {
    let target = state.music.with_untracked(|s| {
        let snapshot=s.as_ref()?;
        let parent=snapshot.lanes.iter().find(|l|l.id==lane)?;
        if !part.is_empty() { return Some(part.to_owned()); }
        let root=parent.display_name().chars().take_while(|c|c.is_alphanumeric()||*c=='_'||*c=='$').collect::<String>();
        Some(if snapshot.bindings.iter().any(|b|b.name==root) {root} else {String::new()})
    });
    let Some(target)=target else{return};
    focus_source(state, lane, target);
}

pub fn select_lane_source(state: ViewerState, lane: &str) {
    if state.music.with_untracked(|s| s.as_ref().is_some_and(|s| s.lanes.iter().any(|l| l.id == lane))) {
        if !state.music_lane.get_untracked().is_empty() { state.music_lane.set(lane.to_owned()); }
        focus_source(state, lane, String::new());
    }
}

fn focus_source(state: ViewerState, lane: &str, target: String) {
    let follow=active_solo(state).is_some_and(|active| active!=lane);
    let next = state.music_source_focus.with_untracked(|focus| focus.as_ref().map_or(1, |f| f.2.wrapping_add(1)));
    state.music_source_focus.set(Some((lane.to_owned(), target, next)));
    if follow {solo(state,lane.to_owned(),true);}
}

pub fn connect(bridge: Option<Bridge>, state: ViewerState) {
    let Ok(socket) = WebSocket::new("ws://127.0.0.1:9613") else {
        return;
    };
    socket.set_binary_type(BinaryType::Arraybuffer);
    let opened = socket.clone();
    let onopen = Closure::<dyn FnMut()>::new(move || {
        state
            .music_status
            .set("Connected · waiting for Rustel".into());
        for frame in [
            hearsay::encode_hello("nightshade-music"),
            hearsay::encode_subscribe(music_link::SNAPSHOT_TOPIC),
            hearsay::encode_subscribe(music_link::AUDIO_TOPIC),
            hearsay::encode_subscribe(music_link::RESPONSE_TOPIC),
        ] {
            if let Ok(frame) = frame {
                let _ = opened.send_with_u8_array(&frame);
            }
        }
    });
    socket.set_onopen(Some(onopen.as_ref().unchecked_ref()));

    let incoming_bridge = bridge.clone();
    let mut seen = std::collections::BTreeMap::<String, f64>::new();
    let mut expansion_session = String::new();
    let mut expansion_seen = std::collections::BTreeSet::new();
    let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let Ok(buffer) = event.data().dyn_into::<js_sys::ArrayBuffer>() else {
            return;
        };
        let Some(message) = hearsay::decode_message(hearsay::Bytes::from(
            js_sys::Uint8Array::new(&buffer).to_vec(),
        )) else {
            return;
        };
        let Some(text) = message.text() else {
            return;
        };
        if let Ok(response) = serde_json::from_str::<music_link::Response>(text) {
            if REQUESTS.with(|requests| requests.borrow_mut().remove(&response.request)) {
                state.music_control_status.set(if response.accepted { String::new() } else { response.message });
            }
            return;
        }
        if let Ok(audio) = serde_json::from_str::<music_link::AudioFrame>(text) {
            if audio.valid()
                && audio.session == state.music_session.get_untracked()
                && state
                    .music
                    .get_untracked()
                    .is_some_and(|s| s.generation == audio.generation)
                && state.music_audio.get_untracked().is_none_or(|old| {
                    old.session != audio.session
                        || old.generation != audio.generation
                        || audio.sequence > old.sequence
                })
            {
                state.music_audio.set(Some(audio));
                state.music_audio_received.set(js_sys::Date::now());
            }
            return;
        }
        let Ok(snapshot) = serde_json::from_str::<music_link::Snapshot>(text) else {
            return;
        };
        if snapshot.version != 1
            || snapshot.events.len() > 2048
            || snapshot.lanes.len() > 64
            || !snapshot.cycle.is_finite()
            || !snapshot.cps.is_finite()
            || snapshot.cps <= 0.0
        {
            return;
        }
        let now = js_sys::Date::now();
        seen.retain(|_, received| now - *received < 3000.0);
        if seen.len() < 32 || seen.contains_key(&snapshot.session) {
            seen.insert(snapshot.session.clone(), now);
        }
        let sessions: Vec<_> = seen.keys().cloned().collect();
        if state.music_sessions.get_untracked() != sessions {
            state.music_sessions.set(sessions);
        }
        let selected = state.music_session.get_untracked();
        if !seen.contains_key(&selected) {
            state.music_session.set(snapshot.session.clone());
            state.music_lane.set(String::new());
            state.music_source_focus.set(None);
            state.music_audio.set(None);
        }
        if snapshot.session != state.music_session.get_untracked() {
            return;
        }
        if expansion_session != snapshot.session {
            expansion_session = snapshot.session.clone();
            expansion_seen.clear();
            state.music_expanded.set(Vec::new());
        }
        // Parts may arrive after the initial snapshot. Open each new parent once,
        // without reopening a parent the user explicitly collapsed.
        let newly_expanded: Vec<_> = snapshot.lanes.iter()
            .filter(|l| l.parts.iter().any(|p| !p.helper) && expansion_seen.insert(l.id.clone()))
            .map(|l| l.id.clone()).collect();
        if !newly_expanded.is_empty() {
            state.music_expanded.update(|ids| {
                for id in newly_expanded {if !ids.contains(&id) {ids.push(id);}}
            });
        }
        state.music_status.set(format!(
            "{} · {:.2} cycles · {} lanes",
            snapshot.session,
            snapshot.cycle,
            snapshot.lanes.len()
        ));
        if let Some(bridge) = &incoming_bridge {
            send(bridge, &ClientMessage::MusicSnapshot(snapshot.clone()));
        }
        state.music_received.set(js_sys::Date::now());
        state.music.set(Some(snapshot));
    });
    socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

    let onclose = Closure::<dyn FnMut()>::new(move || {
        state
            .music_status
            .set("Music hub disconnected · reconnecting".into());
        state.music.set(None);
        state.music_audio.set(None);
        state.music_session.set(String::new());
        state.music_source_focus.set(None);
        state.music_sessions.set(Vec::new());
        let next_bridge = bridge.clone();
        let retry = Closure::once_into_js(move || connect(next_bridge, state));
        if let Some(window) = web_sys::window() {
            let _ = window
                .set_timeout_with_callback_and_timeout_and_arguments_0(retry.unchecked_ref(), 2000);
        }
    });
    socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    CONNECTION.with(|slot| {
        *slot.borrow_mut() = Some(MusicSocket {
            socket,
            _open: onopen,
            _message: onmessage,
            _close: onclose,
        })
    });
}

#[component]
pub fn MusicPanel(
    state: ViewerState,
    bridge: StoredValue<Option<Bridge>, LocalStorage>,
) -> impl IntoView {
    let orbit = RwSignal::new(false);
    let transport_ready=move ||state.music.with(|s|s.as_ref().is_some_and(|s|s.transport.is_some()));

    let configure = move || {
        bridge.with_value(|value| {
            if let Some(bridge) = value {
                send(
                    bridge,
                    &ClientMessage::MusicView {
                        rests: state.music_rests.get_untracked(),
                        selected: state.music_source_focus.get_untracked().map(|f| (f.0, f.1)),
                        expanded: state.music_expanded.get_untracked(),
                        orbit: orbit.get_untracked(),
                        ghosts: false,
                        mode: state.music_mode.get_untracked(),
                        lane: state.music_lane.get_untracked(),
                    },
                );
            }
        });
    };
    Effect::new(move |_| {
        state.music_mode.get();
        state.music_lane.get();
        state.music_expanded.get();
        state.music_rests.get();
        state.music_source_focus.get();
        configure();
    });
    view! {
        <Show when=enabled>
            <crate::music_canvas::MusicCanvas state />
            <style>"
                .music-hud{position:fixed;z-index:50;font-family:Inter,system-ui,sans-serif;color:#e4edf7}
                .music-top{top:24px;left:28px;right:28px;display:flex;justify-content:space-between;gap:20px;pointer-events:none}
                .music-eyebrow{font:10px monospace;letter-spacing:.28em;color:#5df0cf;margin-bottom:8px}
                .music-title{font-size:30px;letter-spacing:-1.3px;font-weight:300;line-height:1.1}
                .music-controls{flex-wrap:wrap;max-width:65%;display:flex;gap:7px;align-items:flex-start;pointer-events:auto}
                .music-controls button,.music-select{border:1px solid #ffffff20;border-radius:20px;background:#0b1421cf;color:#c7d5e7;padding:9px 14px;font:11px monospace;cursor:pointer;backdrop-filter:blur(14px)}
                .music-controls button:hover{border-color:#5df0cf;color:#5df0cf}
                .music-bottom{bottom:24px;left:28px;right:28px;display:flex;align-items:flex-end;justify-content:space-between;gap:24px;pointer-events:none}
                .music-lanes{display:flex;flex-wrap:wrap;gap:8px;max-width:760px;max-height:22vh;overflow:auto;pointer-events:auto}
                .music-lane{padding:9px 13px;border:1px solid #ffffff12;background:#0a1220e9;border-radius:9px;width:340px;max-width:80vw;backdrop-filter:blur(12px)}
                .music-lane-name{font-size:11px;font-weight:500;margin-bottom:5px}
                .music-lane summary{cursor:pointer;font:11px monospace;overflow-wrap:anywhere}
                .music-part-toggle{border:0;background:transparent;color:inherit;padding:0 7px 0 0;font:inherit;cursor:pointer}
                .music-source{white-space:pre-wrap;overflow-wrap:anywhere;font:11px/1.6 monospace;color:#d5e5f5;margin:12px 0 4px;user-select:text}
                .music-token-string{color:#a8d991}.music-token-number{color:#ffba73}.music-token-function{color:#70cfff}.music-token-keyword{color:#c59aff}.music-token-comment{color:#778b9f}
                .music-lane-meta{font:9px monospace;letter-spacing:.08em;color:#74859d}
                .music-clock{text-align:right;flex-shrink:0}.music-cycle{font:300 34px monospace;letter-spacing:-2px;color:#e8f3ff}
                .music-help{font:10px monospace;color:#667b96;letter-spacing:.03em;margin-top:9px}
                .music-select{max-width:220px;margin-top:12px;padding:5px 10px;font-size:9px;pointer-events:auto}
            "</style>
            <div class="music-hud music-top">
                <div>
                    <div class="music-eyebrow">"● LIVE PATTERN FIELD"</div>
                    <div class="music-title">"bespoke_rustel"</div>
                    <div class="music-help" role="status">{move || state.render_error.get()}</div>
                    <div class="music-help">{move || state.music_status.get()}</div>
                    <div class="music-help">{move || state.music_control_status.get()}</div>
                    <div class="music-help" title=move ||state.music.with(|s|s.as_ref().map(|s|s.session_file.clone()).unwrap_or_default())>
                        {move ||state.music.with(|s|s.as_ref().map(|s|s.session_file.rsplit(['/', '\\']).next().unwrap_or_default().to_owned()).unwrap_or_default())}
                    </div>
                    <div class="music-help">{move ||state.music.with(|s|s.as_ref().and_then(|s|s.file_error.clone()).unwrap_or_default())}</div>
                    <div class="music-help">{move || {
                        let status=state.music.with(|s|s.as_ref().and_then(|s|s.transport.as_ref()).map(|t|if t.paused {"PAUSED"} else {"PLAYING"}).unwrap_or("TRANSPORT UNAVAILABLE"));
                        format!("{status} · {:.0} FPS",state.fps.get())
                    }}</div>
                    <select class="music-select" prop:value=move || state.music_session.get()
                        on:change=move |event| { if let Some(lane)=active_solo(state) { solo(state,lane,false); } state.music_session.set(event_target_value(&event)); state.music_lane.set(String::new()); state.music_source_focus.set(None); state.music_audio.set(None); }>
                        {move || state.music_sessions.get().into_iter().map(|id| view! { <option value=id.clone()>{id.clone()}</option> }).collect_view()}
                    </select>
                </div>
                <div class="music-controls">
                    <select class="music-select" aria-label="Demo" disabled=move ||state.music.with(|s|s.as_ref().is_none_or(|s|s.demos.is_empty()))
                        prop:value=move ||state.music.with(|s|s.as_ref().map(|s|s.demo.clone()).unwrap_or_default())
                        on:change=move |e|select_demo(state,event_target_value(&e))>
                        <option value="">"Select demo…"</option>
                        <For each=move ||state.music.with(|s|s.as_ref().map(|s|s.demos.clone()).unwrap_or_default()) key=|name|name.clone()
                            children=move |name|view! {<option value=name.clone()>{name.clone()}</option>} />
                    </select>
                    <Show when=move ||!state.render_error.get().is_empty()>
                        <button on:click=move |_| {if let Some(w)=web_sys::window(){let _=w.location().reload();}}>"Restart 3D"</button>
                    </Show>
                    <button disabled=move ||!transport_ready() title="Rewind four cycles" on:click=move |_|skip_cycles(state,-4.0)>"⏪ −4"</button>
                    <button disabled=move ||!transport_ready() on:click=move |_|transport(state,music_link::TransportCommand::Play)>"▶ Play"</button>
                    <button disabled=move ||!transport_ready() on:click=move |_|transport(state,music_link::TransportCommand::Pause)>"Ⅱ Pause"</button>
                    <button disabled=move ||!transport_ready() on:click=move |_|transport(state,music_link::TransportCommand::Stop)>"■ Stop"</button>
                    <button disabled=move ||!transport_ready() title="Forward four cycles" on:click=move |_|skip_cycles(state,4.0)>"+4 ⏩"</button>
                    <label style="font:11px monospace;padding:9px" title="Show gaps between event gates in each part; no inferred markers at the edges of the available stream.">
                        <input type="checkbox" prop:checked=move || state.music_rests.get()
                            on:change=move |e| state.music_rests.set(event_target_checked(&e)) />
                        " rests (~)"
                    </label>
                    <select class="music-select" aria-label="Music view" prop:value=move || state.music_mode.get().to_string()
                        on:change=move |event| state.music_mode.set(event_target_value(&event).parse().unwrap_or(0))>
                        <option value="0">"Stage (3D)"</option><option value="1">"Piano roll"</option>
                        <option value="2">"Wordfall"</option><option value="3">"Spiral"</option>
                        <option value="4">"Pitch wheel"</option><option value="5">"Scope (master)"</option>
                        <option value="6">"Spectrum (master)"</option><option value="7">"Vectorscope (master)"</option>
                        <option value="8">"Control curves"</option>
                    </select>
                    <select class="music-select" aria-label="Lane filter" prop:value=move || state.music_lane.get()
                        on:change=move |e| {
                            let lane=event_target_value(&e);
                            state.music_lane.set(lane.clone());
                            if !lane.is_empty() { reveal_source(state, &lane, ""); }
                        }>
                        <option value="">"All lanes"</option>
                        <For each=move || state.music.get().map(|s| s.lanes).unwrap_or_default() key=|lane| lane.id.clone()
                            children=move |lane| view! { <option value=lane.id.clone()>{format!("line {} / {}",lane.line,lane.display_name())}</option> } />
                    </select>
                    <button on:click=move |_| { orbit.update(|v| *v = !*v); configure(); }>
                        {move || if orbit.get() { "ORBIT ON" } else { "ORBIT OFF" }}
                    </button>
                    <label class="music-select" style="margin-top:0;white-space:nowrap">
                        <input aria-label="Solo selected lane" type="checkbox" disabled=move || solo_target(state).is_empty() && !selected_is_solo(state)
                            prop:checked=move || selected_is_solo(state)
                            on:change=move |e| { let enabled=event_target_checked(&e); let lane=if enabled {solo_target(state)} else {active_solo(state).unwrap_or_else(||solo_target(state))}; solo(state,lane,enabled); } />
                        " Solo"
                    </label>
                </div>
            </div>
            <div class="music-hud music-bottom">
                <div>
                    <div class="music-lanes">
                        <For
                            each=move || state.music.get().map(|snapshot| snapshot.lanes.into_iter().enumerate().collect::<Vec<_>>()).unwrap_or_default()
                            key=|(_, lane)| (lane.id.clone(), lane.source.clone(), lane.line)
                            children=move |(i, lane)| {
                            let palette = ["#1ff2d1", "#ff596b", "#a36bff", "#ffba33", "#33a6ff", "#ff59cc", "#9eff47", "#ff8a38"];
                            let source = if lane.source.is_empty() { "Source unavailable: restart Rustel with the updated bridge.".to_string() } else { lane.source.clone() };
                            let preview = if lane.source.is_empty() { source.clone() } else { lane.display_name() };
                            let expanded = RwSignal::new(false);
                            let lane_id = lane.id.clone();
                            let original_source = StoredValue::new(source.clone());
                            let source_lane = StoredValue::new(lane.id.clone());
                            let focus_details = NodeRef::<leptos::html::Details>::new();
                            let focus_id = StoredValue::new(lane.id.clone());
                            Effect::new(move |_| {
                                if state.music_source_focus.with(|focus| focus.as_ref().is_some_and(|f| focus_id.with_value(|id| &f.0 == id))) {
                                    expanded.set(true);
                                    if let Some(details) = focus_details.get() {
                                        let _ = details.set_attribute("open", "");
                                        details.scroll_into_view_with_bool(false);
                                    }
                                }
                            });
                            let shown_source = Memo::new(move |_| {
                                let full = state.music_source_focus.with(|focus| focus.as_ref().is_some_and(|f| source_lane.with_value(|id| &f.0 == id) && !f.1.is_empty()));
                                if full {
                                    state.music.with(|s| (s.as_ref().map(|s| s.source.clone()).unwrap_or_default(), 1))
                                } else { (original_source.get_value(), lane.line) }
                            });
                            let metadata_id = lane.id.clone();
                            let expand_id = StoredValue::new(lane.id.clone());
                            let checked_id = StoredValue::new(lane.id.clone());
                            let part_count_id = lane.id.clone();
                            let part_count = Memo::new(move |_| state.music.with(|s| s.as_ref().and_then(|s| s.lanes.iter().find(|l| l.id == part_count_id)).map_or(0, |l| l.parts.iter().filter(|p|!p.helper).count())));
                            let line = lane.line;
                            view! { <div class="music-lane" style:border-top=format!("2px solid {}", palette[i % 8])>
                                <details node_ref=focus_details on:toggle=move |event| {
                                    use wasm_bindgen::JsCast;
                                    if let Some(element) = event.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
                                        let open=element.has_attribute("open");
                                        expanded.set(open);
                                        if open && !state.music_source_focus.with_untracked(|focus| focus.as_ref().is_some_and(|f| source_lane.with_value(|id| &f.0==id))) {
                                            source_lane.with_value(|id| reveal_source(state,id,""));
                                        }
                                    }
                                }>
                                    <summary class="music-lane-name" style:color=palette[i % 8]>
                                        <Show when=move || { part_count.get() != 0 }>
                                            <button type="button" class="music-part-toggle"
                                                aria-label=move || if state.music_expanded.with(|ids| checked_id.with_value(|id| ids.contains(id))) { "Collapse named parts" } else { "Expand named parts" }
                                                aria-expanded=move || state.music_expanded.with(|ids| checked_id.with_value(|id| ids.contains(id))).to_string()
                                                on:click=move |e| { e.prevent_default(); e.stop_propagation(); expand_id.with_value(|id| toggle_parts(state, id)); }>
                                                {move || if state.music_expanded.with(|ids| checked_id.with_value(|id| ids.contains(id))) { "−" } else { "+" }}
                                            </button>
                                        </Show>
                                        {preview}
                                    </summary>
                                    <For each=move || vec![shown_source.get()] key=|(source,line)| (source.clone(),*line)
                                        children=move |(source,line)| view! { <crate::music_source::MusicSource state source lane=lane_id.clone() line color=palette[i % 8] expanded /> } />
                                </details>
                                <div class="music-lane-meta">{move || { let transpose = state.music.with(|s| s.as_ref().and_then(|s| s.lanes.iter().find(|l| l.id == metadata_id)).map(|l| l.transpose).unwrap_or(0)); format!("LINE {}  /  {:+} ST", line, transpose) }}</div>
                            </div> }
                            }
                        />
                    </div>
                    <DefinitionGraphs state />
                    <div class="music-help">{move || if state.music_mode.get() == 0 { "DRAG TO ORBIT · SCROLL TO ZOOM" } else { "DRAG TO PAN · WHEEL TO ZOOM · DOUBLE CLICK TO RESET · AUDIO VIEWS USE THE MASTER TAP" }}</div>
                </div>
                <div class="music-clock">
                    <div class="music-eyebrow">{move || state.music.get().map(|s| format!("{:.0} BPM / {} LANES", s.tempo.as_ref().map_or(s.cps * 240.0,|t|t.bpm), s.lanes.len())).unwrap_or("WAITING FOR RUSTEL".into())}</div>
                    <div class="music-cycle">{move || state.music.get().map(|s| format!("{:07.2}", s.cycle)).unwrap_or("0000.00".into())}</div>
                    <div class="music-help">"CYCLES"</div>
                </div>
            </div>
        </Show>
    }
}

#[component]
fn DefinitionGraphs(state: ViewerState) -> impl IntoView {
    view! {
        <details style="pointer-events:auto;max-width:760px;background:#0a1220e9;border-radius:8px;padding:6px" open>
            <summary style="font:11px monospace;cursor:pointer">"Shared definitions · value histories / pitch activity"</summary>
            <div style="display:flex;flex-wrap:wrap;gap:6px;max-height:150px;overflow:auto">
                <For each=move ||state.music.with(|s|s.as_ref().map(|s|s.shared_definitions().iter().map(|d|d.name.clone()).collect::<Vec<_>>()).unwrap_or_default())
                    key=|name|name.clone() children=move |name| {
                        let click_name=name.clone();
                        view! {<button style="width:235px;padding:6px;text-align:left;background:transparent;border:1px solid #ffffff20;border-radius:5px;cursor:pointer"
                            title="Select definition in the source. Numeric history uses cycles; pitch counts cover accepted events in the available window."
                            on:click=move |_|state.music.with_untracked(|s| {if let Some(s)=s {if let Some(lane)=s.lanes.first(){reveal_source(state,&lane.id,&click_name);}}})>
                            {move ||state.music.with(|s| {
                                let Some(d)=s.as_ref().and_then(|s|s.definitions.iter().find(|d|d.name==name)) else{return ().into_any()};
                                let color=format!("rgb({},{},{})",d.color[0],d.color[1],d.color[2]);
                                let title=if let Some(v)=d.value {format!("{} = {}",d.name,v)} else {format!("{} · pitch / sound counts",d.name)};
                                let history=!d.history.is_empty();
                                let min=d.history.iter().map(|p|p[1]).fold(f64::INFINITY,f64::min);
                                let max=d.history.iter().map(|p|p[1]).fold(f64::NEG_INFINITY,f64::max);
                                let first=d.history.first().map_or(0.0,|p|p[0]);
                                let last=d.history.last().map_or(first,|p|p[0]);
                                let span=(max-min).max(max.abs()*0.1).max(1.0);
                                let low=(min+max-span)*0.5;
                                let points=d.history.iter().map(|p|format!("{},{}",4.0+212.0*(p[0]-first)/(last-first).max(0.25),38.0-30.0*(p[1]-low)/span)).collect::<Vec<_>>().join(" ");
                                let peak=d.histogram.values().copied().max().unwrap_or(1).max(1) as f64;
                                let width=216.0/d.histogram.len().max(1) as f64;
                                let bars=d.histogram.iter().enumerate().map(|(i,(label,count))| {
                                    let x=2.0+i as f64*width;let h=28.0*f64::from(*count)/peak;
                                    view! {<g><title>{format!("{label}: {count}")}</title><rect x=x y=38.0-h width=(width-1.0).max(1.0) height=h fill=color.clone()/><text x=x y="49" fill="#aebed0" font-size="8">{if width>=22.0 {label.clone()}else{String::new()}}</text></g>}
                                }).collect_view();
                                let footer=if history {format!("cycles {first:.1}–{last:.1} · {:.2}…{:.2}",min,max)} else if d.histogram.is_empty(){"No independently attributed events yet".into()}else{"Accepted event window · counts".into()};
                                view! {<div style=format!("color:{color};font:11px monospace")>{title}</div>
                                    <svg viewBox="0 0 220 52" style="width:100%;height:52px"><path d="M2 38H218" stroke="#ffffff30"/>
                                        {if history {view! {<polyline points=points fill="none" stroke=color stroke-width="2"/>}.into_any()}else{bars.into_any()}}
                                    </svg><div style="font:9px monospace;color:#8fa3bb">{footer}</div>}.into_any()
                            })}
                        </button>}
                    } />
            </div>
        </details>
    }
}
