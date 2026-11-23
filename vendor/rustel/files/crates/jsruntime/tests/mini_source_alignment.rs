use rustel_fraction::Fraction;
use rustel_jsruntime::{JsRuntime, Slot};
use rustel_transpiler::{TranspileOptions, transpile};

fn assert_tokens(source: &str, expected: &[&str]) {
    let runtime = JsRuntime::new().unwrap();
    runtime.install_semantic_bindings().unwrap();
    runtime
        .evaluate_score(source, &TranspileOptions::default())
        .unwrap();
    let haps = runtime
        .query(Slot::Active, 0, Fraction::ZERO, Fraction::ONE)
        .unwrap();
    let actual = haps
        .iter()
        .map(|hap| {
            let (a, b) = hap.context.first().expect("sourced event");
            source
                .get(a..b)
                .expect("UTF-8 source boundaries")
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected, "{source}");
    let output = transpile(source, &TranspileOptions::default());
    let decorations = output
        .mini_locations
        .iter()
        .map(|&(a, b)| &source[a..b])
        .filter(|token| *token != "~")
        .collect::<Vec<_>>();
    assert_eq!(
        decorations, expected,
        "editor spans must match accepted hap spans"
    );
}

#[test]
fn continuations_keep_spaces_and_note_ranges_aligned() {
    assert_tokens(
        "note(\"a2  e3 \\\n    f2   c3 \\\n    g2 d3\")",
        &["a2", "e3", "f2", "c3", "g2", "d3"],
    );
    assert_tokens("note(\"a2 \\\r\n    f2   c3\")", &["a2", "f2", "c3"]);
    // Mapped mini calls inside callbacks must retain the map through the JS bridge.
    assert_tokens(
        "pure(1).layer(x => note(\"a2 \\\n    e3\"))",
        &["a2", "e3"],
    );
}

#[test]
fn escaped_whitespace_and_unicode_use_raw_source_ranges() {
    // Mini intentionally retains tabs/newlines at leaf ends for compatibility.
    assert_tokens(
        r#"note("c3\t  \u00653  \x673")"#,
        &[r"c3\t", r"\u00653", r"\x673"],
    );
    assert_tokens(
        "// é before the string\nnote(`c3\r\n  e3\r\n g3`)",
        &["c3\r\n", "e3\r\n", "g3"],
    );
    assert_tokens(r#"s("bd é \u00e9")"#, &["bd", "é", r"\u00e9"]);
}

#[test]
fn bass_recording_fixture_aligns_every_leaf_after_continuations() {
    let source = "note(\"[a2 ~ e3 a2 e3 ~ g2 e3] \\\n   [f2 ~ c3 f2 c3 ~ e2 c3] \\\n   [c3 ~ g3 c3 g2 ~ b2 g2] \\\n   [g2 ~ d3 g2 d3 ~ b2 d3]\")";
    assert_tokens(
        source,
        &[
            "a2", "e3", "a2", "e3", "g2", "e3", "f2", "c3", "f2", "c3", "e2", "c3", "c3", "g3",
            "c3", "g2", "b2", "g2", "g2", "d3", "g2", "d3", "b2", "d3",
        ],
    );
}
