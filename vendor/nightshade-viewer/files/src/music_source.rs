//! Source offsets stay in UTF-8 bytes, as published by Rustel.
use crate::state::ViewerState;
use leptos::prelude::*;
use wasm_bindgen::{closure::Closure, JsCast};

fn tokens(source: &str) -> Vec<(usize, usize, &'static str)> {
    let mut result = Vec::new();
    let mut at = 0;
    while at < source.len() {
        let start = at;
        let c = source[at..].chars().next().unwrap();
        at += c.len_utf8();
        let class = if source[start..].starts_with("//") {
            while at < source.len() && source.as_bytes()[at] != b'\n' { at += 1; }
            "comment"
        } else if source[start..].starts_with("/*") {
            at = source[start + 2..].find("*/").map_or(source.len(), |i| start + i + 4);
            "comment"
        } else if matches!(c, '\'' | '"' | '`') {
            let mut escaped = false;
            for next in source[at..].chars() {
                at += next.len_utf8();
                if escaped { escaped = false; }
                else if next == '\\' { escaped = true; }
                else if next == c { break; }
            }
            "string"
        } else if c.is_ascii_digit() {
            while at < source.len() && (source.as_bytes()[at].is_ascii_digit() || source.as_bytes()[at] == b'.') { at += 1; }
            "number"
        } else if c.is_alphabetic() || c == '_' || c == '$' {
            for next in source[at..].chars() {
                if !(next.is_alphanumeric() || next == '_' || next == '$') { break; }
                at += next.len_utf8();
            }
            if matches!(&source[start..at], "const" | "let" | "var" | "return" | "true" | "false" | "null") { "keyword" }
            else if source[at..].trim_start().starts_with('(') { "function" }
            else { "plain" }
        } else { "plain" };
        result.push((start, at, class));
    }
    result
}

// Lane labels are normalized by the bridge ("$: " + body). Map the displayed
// characters back to the original line without assuming identical whitespace.
fn source_offsets(document: &str, source: &str, line: usize) -> Vec<Option<usize>> {
    let start = document.split_inclusive('\n').take(line.saturating_sub(1)).map(str::len).sum::<usize>();
    let mut original = document[start.min(document.len())..].char_indices().peekable();
    source.chars().map(|c| {
        if c.is_whitespace() { return None; }
        while original.peek().is_some_and(|(_, c)| c.is_whitespace()) { original.next(); }
        match original.next() {
            Some((offset, actual)) if actual == c => Some(start + offset),
            _ => { original = document[document.len()..].char_indices().peekable(); None }
        }
    }).collect()
}

#[component]
pub fn MusicSource(state: ViewerState, source: String, lane: String, line: usize,
                   color: &'static str, expanded: RwSignal<bool>) -> impl IntoView {
    let now = RwSignal::new(js_sys::Date::now());
    let callback = Closure::<dyn FnMut()>::new(move || {
        if expanded.get_untracked() { now.set(js_sys::Date::now()); }
    });
    if let Some(window) = web_sys::window() {
        if let Ok(timer) = window.set_interval_with_callback_and_timeout_and_arguments_0(callback.as_ref().unchecked_ref(), 33) {
            let _callback = StoredValue::new_local(callback);
            on_cleanup(move || {
                if let Some(window) = web_sys::window() { window.clear_interval_with_handle(timer); }
            });
        }
    }
    let displayed = source.clone();
    let offsets = Memo::new(move |_| {
        let document = state.music.with(|s| s.as_ref().map(|s| s.source.clone()).unwrap_or_default());
        source_offsets(&document, &displayed, line)
    });
    let active = Memo::new(move |_| {
        if !expanded.get() { return Vec::new(); }
        let time = now.get();
        let age = ((time - state.music_received.get()) / 1000.0).max(0.0);
        if age >= 3.0 { return Vec::new(); }
        state.music.with(|snapshot| {
            let Some(snapshot) = snapshot else { return Vec::new(); };
            let cycle = snapshot.cycle + age * snapshot.cps;
            snapshot.events.iter().filter(|e| e.lane == lane && cycle >= e.cycle && cycle < e.cycle + e.duration)
                .flat_map(|e| e.context.iter().copied()).collect::<Vec<_>>()
        })
    });
    // Mount text once. Playback updates only paint styles, never the text tree,
    // so expanded details, selection, wrapping and scroll anchoring stay stable.
    let mut parts = Vec::new();
    let mut index = 0;
    for (a, b, class) in tokens(&source) {
        let identifier = source[a..b].to_owned();
        let tint = Memo::new(move |_| {
            if class != "plain" && class != "function" { return String::new(); }
            state.music.with(|snapshot| snapshot.as_ref().and_then(|s|
                s.bindings.iter().find(|b| b.name == identifier).map(|b| {
                    let [r,g,b] = b.color;
                    format!("color:#{r:02x}{g:02x}{b:02x};")
                })).unwrap_or_default())
        });
        for c in source[a..b].chars() {
            let character = index;
            index += 1;
            parts.push(view! {
                <span class=format!("music-token-{class}") style=move || {
                    let offset = offsets.with(|o| o.get(character).copied().flatten());
                    if offset.is_some_and(|offset| active.with(|ranges| ranges.iter().any(|&(a,b)| a <= offset && offset < b))) {
                        format!("{}background:{color}55;box-shadow:inset 0 -2px {color}", tint.get())
                    } else { tint.get() }
                }>{c.to_string()}</span>
            });
        }
    }
    view! { <pre class="music-source">{parts.collect_view()}</pre> }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_normalized_lane_to_its_original_line() {
        let document = "// é\n$:note(\"c3\")\n  $:  note(\"é4\")\n    .slow(2)";
        let source = "$: note(\"é4\")\n    .slow(2)";
        let offsets = source_offsets(document, source, 3);
        for (c, offset) in source.chars().zip(offsets) {
            if let Some(offset) = offset {
                assert_eq!(document[offset..].chars().next(), Some(c));
                assert!(offset >= document.find("  $:").unwrap());
            } else { assert!(c.is_whitespace()); }
        }
    }
    #[test]
    fn preserves_unicode_escaped_strings_and_comments() {
        let source = "$: note(\"c3 \\\"é\\\"\").gain(0.5) // café\n/* λ */ const x = true";
        let spans = tokens(source);
        assert_eq!(spans.iter().map(|&(a,b,_)| &source[a..b]).collect::<String>(), source);
        assert!(spans.iter().any(|&(a,b,c)| c == "string" && &source[a..b] == "\"c3 \\\"é\\\"\""));
        assert!(spans.iter().any(|&(a,b,c)| c == "function" && &source[a..b] == "gain"));
        assert_eq!(spans.iter().filter(|s| s.2 == "comment").count(), 2);
    }
}
