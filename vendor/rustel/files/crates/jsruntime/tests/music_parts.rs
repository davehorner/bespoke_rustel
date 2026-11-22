use rustel_fraction::Fraction;
use rustel_jsruntime::{JsRuntime, Slot};
use rustel_transpiler::TranspileOptions;

fn names(source: &str) -> Vec<String> {
    let runtime = JsRuntime::new().unwrap();
    runtime.install_semantic_bindings().unwrap();
    let output = rustel_transpiler::transpile(source, &TranspileOptions {
        emit_part_names: true, add_return: false, ..Default::default()
    });
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    // Evaluate the already instrumented score: the normal host enables this
    // transform through RUSTEL_MUSIC_LINK, without global env mutation in tests.
    runtime.evaluate_score(&output.output, &TranspileOptions::default()).unwrap_or_else(|e| panic!("{e}\n{}", output.output));
    runtime.query(Slot::Active, 0, Fraction::ZERO, Fraction::ONE).unwrap()
        .into_iter().map(|h| h.part_context().unwrap_or("").to_owned()).collect()
}

#[test]
fn arbitrary_names_survive_stack_and_arrange_without_changing_lanes() {
    let source = r#"
        const moon = note("c3");
        const cedar = note("e4");
        const piece = arrange([1, stack(moon, cedar)], [1, cedar]);
        $: piece
    "#;
    assert_eq!(names(source), ["moon", "cedar"]);
}

#[test]
fn derived_parts_override_shared_helpers_and_composites_fill_only_unnamed_events() {
    assert_eq!(names(r#"
        const harmony = note("c3");
        const rhythm = harmony.gain(0.4);
        const drums = stack(s("bd"), s("hh"));
        const arrangement = stack(rhythm, drums, s("rim"));
        $: arrangement
    "#), ["rhythm", "drums", "drums", "arrangement"]);
}

#[test]
fn scalar_function_and_nested_declarations_keep_their_semantics() {
    assert_eq!(names(r#"
        const bpm = 124;
        const make = () => { const inner = 3; return note(inner); };
        if (make.name.length !== 4 || bpm !== 124) throw Error("changed scalar/function");
        const aurora = make();
        $: aurora
    "#), ["aurora"]);
}
