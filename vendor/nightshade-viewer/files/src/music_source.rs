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

// Keep lexical words together: one reactive span per character made a full
// arrangement fan out thousands of effects on every telemetry message.
fn chunks(source: &str) -> Vec<(usize, usize, &'static str)> {
    let mut result = Vec::new();
    for (a,b,class) in tokens(source) {
        if class != "string" { result.push((a,b,class)); continue; }
        let mut start=a;
        let mut category=None;
        for (offset,c) in source[a..b].char_indices() {
            let kind=if c.is_alphanumeric() || matches!(c,'_'|'#'|'.') {0} else if c.is_whitespace() {1} else {2};
            if category.is_some_and(|old| old!=kind) { result.push((start,a+offset,class)); start=a+offset; }
            category=Some(kind);
        }
        result.push((start,b,class));
    }
    result
}

#[component]
pub fn MusicSource(state: ViewerState, source: String, lane: String, line: usize,
                   color: &'static str, expanded: RwSignal<bool>) -> impl IntoView {
    let root=NodeRef::<leptos::html::Pre>::new();
    let outline=NodeRef::<leptos::html::Div>::new();
    let focus_lane=StoredValue::new(lane.clone());
    let geometry=RwSignal::new(0_u64);
    let focus_range=Memo::new(move |_| {
        let focus=state.music_source_focus.get()?;
        if !focus_lane.with_value(|id| id==&focus.0) { return None; }
        state.music.with(|s| {
            let s=s.as_ref()?;
            if focus.1.is_empty() { s.lanes.iter().find(|l|l.id==focus.0)?.source_range(&s.source) }
            else { s.bindings.iter().find(|b|b.name==focus.1).map(|b| {
                if b.definition_to>b.definition_from {(b.definition_from,b.definition_to)} else {(b.from,b.to)}
            }) }
        })
    });
    let displayed=source.clone();
    let offsets=Memo::new(move |_| state.music.with(|s| {
        source_offsets(s.as_ref().map_or("",|s|s.source.as_str()),&displayed,line)
    }));
    let mut parts=Vec::new();
    let mut nodes=Vec::new();
    let mut character=0;
    for (a,b,class) in chunks(&source) {
        let first=character;
        character+=source[a..b].chars().count();
        let last=character;
        let node=NodeRef::<leptos::html::Span>::new();
        let name=source[a..b].to_owned();
        let tint=state.music.with_untracked(|s|s.as_ref().and_then(|s|s.bindings.iter().find(|v|v.name==name))
            .map(|v| {let [r,g,b]=v.color; format!("color:#{r:02x}{g:02x}{b:02x};")})).unwrap_or_default();
        let text=source[a..b].to_owned();
        let clicked=StoredValue::new(name);
        nodes.push((node,first,last,tint.clone(),source[a..b].chars().last().map_or(1,char::len_utf8)));
        parts.push(view! {
            <span node_ref=node class=format!("music-token-{class}") style=tint on:click=move |event| {
                let at=offsets.with_untracked(|o|o[first..last].iter().flatten().next().copied());
                let target=state.music.with_untracked(|s| {
                    let s=s.as_ref()?;
                    if let Some(lane)=at.and_then(|at|s.lanes.iter().find(|l|l.source_range(&s.source).is_some_and(|(a,b)|a<=at&&at<b))) {
                        return Some((lane.id.clone(),String::new(),true));
                    }
                    let binding=s.bindings.iter().find(|b| {
                        ((class=="plain"||class=="function")&&clicked.with_value(|n|n==&b.name))
                        ||at.is_some_and(|at|b.definition_from<=at&&at<b.definition_to)
                    })?;
                    let preferred=focus_lane.get_value();
                    let parent=s.lanes.iter().filter(|l|l.parts.iter().any(|p|p.name==binding.name))
                        .min_by_key(|l|l.id!=preferred).or_else(||s.lanes.iter().find(|l|l.id==preferred))?;
                    Some((parent.id.clone(),binding.name.clone(),false))
                });
                if let Some((lane,part,expression))=target {
                    event.stop_propagation();
                    if expression {crate::music::select_lane_source(state,&lane);} else {crate::music::reveal_source(state,&lane,&part);}
                }
            }>{text}</span>
        });
    }
    let nodes=std::rc::Rc::new(nodes);
    let bounds=move |first:usize,last:usize,tail:usize|offsets.with_untracked(|o| {
        let mut values=o[first..last].iter().flatten();
        let from=*values.next()?;
        let to=values.last().copied().unwrap_or(from)+tail;
        Some((from,to))
    });
    // One paint pass per pane, with DOM writes only when the active state changes.
    let now=RwSignal::new(js_sys::Date::now());
    let paints=std::rc::Rc::new(std::cell::RefCell::new(vec![false;nodes.len()]));
    let paint_nodes=nodes.clone();
    Effect::new(move |_| {
        let time=now.get();
        if !expanded.get() {return;}
        offsets.get();
        let age=((time-state.music_received.get())/1000.0).max(0.0);
        let ranges=state.music.with(|s| {
            let Some(s)=s.as_ref().filter(|_|age<3.0) else{return Vec::new()};
            let cycle=s.display_cycle(age);
            s.events.iter().filter(|e|e.lane==lane&&e.cycle<=cycle&&cycle<e.cycle+e.duration)
                .flat_map(|e|e.context.iter().copied()).collect::<Vec<_>>()
        });
        let mut old=paints.borrow_mut();
        for (i,(node,first,last,tint,tail)) in paint_nodes.iter().enumerate() {
            let active=bounds(*first,*last,*tail).is_some_and(|(a,b)|ranges.iter().any(|&(x,y)|a<y&&x<b));
            if active==old[i] {continue;}
            if let Some(node)=node.get() {
                let style=if active {format!("{tint}background:{color}55;box-shadow:inset 0 -2px {color}")} else {tint.clone()};
                let _=node.set_attribute("style",&style);
                old[i]=active;
            }
        }
    });
    let scroll_nodes=nodes.clone();
    Effect::new(move |_| {
        let Some(focus)=state.music_source_focus.get() else{return};
        if !focus_lane.with_value(|id|id==&focus.0) {return;}
        let Some((from,to))=focus_range.get_untracked() else{return};
        let Some(pre)=root.get() else{return};
        for (node,first,last,_,tail) in scroll_nodes.iter() {
            if bounds(*first,*last,*tail).is_some_and(|(a,b)|a<to&&from<b) {
                if let Some(node)=node.get() {
                    let delta=node.get_bounding_client_rect().top()-pre.get_bounding_client_rect().top();
                    pre.set_scroll_top(pre.scroll_top()+delta as i32-pre.client_height()/2);
                    geometry.update(|v|*v=v.wrapping_add(1));
                }
                break;
            }
        }
    });
    Effect::new(move |_| {
        geometry.get();
        let range=focus_range.get();
        let Some(border)=outline.get() else{return};
        let Some(pre)=root.get() else{return};
        let mut left=f64::INFINITY;let mut top=f64::INFINITY;
        let mut right=f64::NEG_INFINITY;let mut bottom=f64::NEG_INFINITY;
        if let Some((from,to))=range {
            for (node,first,last,_,tail) in nodes.iter() {
                if !bounds(*first,*last,*tail).is_some_and(|(a,b)|a<to&&from<b) {continue;}
                if let Some(node)=node.get() {
                    let rects=node.get_client_rects();
                    for i in 0..rects.length() {if let Some(r)=rects.item(i) {
                        left=left.min(r.left());top=top.min(r.top());right=right.max(r.right());bottom=bottom.max(r.bottom());
                    }}
                }
            }
        }
        let style=if left.is_finite() {
            let pre=pre.get_bounding_client_rect();
            format!("position:absolute;pointer-events:none;border:1.5px solid #ffd75a;border-radius:2px;background:transparent;left:{}px;top:{}px;width:{}px;height:{}px",left-pre.left()-2.0,top-pre.top()-2.0,right-left+4.0,bottom-top+4.0)
        } else {"display:none".into()};
        let _=border.set_attribute("style",&style);
    });
    let size=std::rc::Rc::new(std::cell::Cell::new((0,0)));
    let callback=Closure::<dyn FnMut()>::new(move || {
        if expanded.get_untracked() {
            now.set(js_sys::Date::now());
            if let Some(pre)=root.get_untracked() {
                let next=(pre.client_width(),pre.client_height());
                if next!=size.replace(next) {geometry.update(|v|*v=v.wrapping_add(1));}
            }
        }
    });
    if let Some(window)=web_sys::window() {
        if let Ok(timer)=window.set_interval_with_callback_and_timeout_and_arguments_0(callback.as_ref().unchecked_ref(),50) {
            let _callback=StoredValue::new_local(callback);
            on_cleanup(move ||{if let Some(w)=web_sys::window(){w.clear_interval_with_handle(timer);}});
        }
    }
    view! {<div style="position:relative;overflow:hidden;margin-top:12px">
        <pre node_ref=root class="music-source" style="max-height:180px;overflow:auto;margin:0" on:scroll=move |_|geometry.update(|v|*v=v.wrapping_add(1))>{parts.collect_view()}</pre>
        <div node_ref=outline data-source-selection aria-hidden="true" style="display:none"></div>
    </div>}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_chunks_preserve_text_without_a_span_for_every_character() {
        let source="const harmony = chord(\"<Am9 Fmaj7 Cmaj7 G6>\").dict(\"ireal\")\n".repeat(64);
        let spans=chunks(&source);
        assert_eq!(spans.iter().map(|&(a,b,_)|&source[a..b]).collect::<String>(),source);
        assert!(spans.len()<source.chars().count()/2,"{} spans for {} bytes",spans.len(),source.len());
    }
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
