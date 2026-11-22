//! The bridge must not put observation metadata in score control objects.
#![cfg(feature = "music-link")]

#[test]
fn bridge_keeps_score_controls_clean() {
    const CHILD: &str = "RUSTEL_TELEMETRY_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let result = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "bridge_keeps_score_controls_clean",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("RUSTEL_MUSIC_LINK", "1")
            .output()
            .expect("isolated bridge-enabled test");
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    std::thread::Builder::new().stack_size(rustel_runtime::QUERY_WORKER_STACK_BYTES).spawn(|| {
        let source = "// Unicode prefix: 🎵\n$: note(\"c3 e3\").s(\"sine\").pan(sine.range(-0.5, 0.5))\n$: stack(note(\"g3\"), note(\"b3\")).s(\"triangle\")\nall(x => x.fast(2))";
        let mut session = rustel_runtime::Session::new().expect("session");
        assert!(session.music_link_enabled());
        session.set_schedule_trace_enabled(true);
        session.evaluate(source).expect("score");
        let events = session.schedule_at(0.0).expect("scheduled");
        assert!(!events.is_empty());
        for event in &events {
            let rustel_runtime::ValueJson::Raw(value) = &event.value else { panic!("controls"); };
            let object = value.as_object().expect("control object");
            assert!(!object.contains_key("__rustel_lane"));
            assert!(!object.contains_key("__rustel_context"));
        }
        let traces = session.take_schedule_trace_events();
        assert!(!traces.is_empty(), "bridge consumed editor observations");
        let lanes: std::collections::HashSet<_> = traces.iter().filter_map(|t|t.lane.as_deref()).collect();
        assert_eq!(lanes.len(), 2);
        let tokens: Vec<_> = traces.iter().flat_map(|t|t.context.iter()).filter_map(|&(a,b)|source.get(a..b)).collect();
        assert!(tokens.contains(&"c3"));
        assert!(tokens.contains(&"g3"));
    }).expect("worker").join().expect("test worker");
}
