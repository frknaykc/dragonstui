//! Decode parity against the real Python M67 transport hooks and validator.
//!
//! This is deliberately not another handwritten Rust schema. Every wire string
//! goes through serde_json::from_str::<ProtocolMessage>, as in process.rs.
//! Only adapter-to-host v1 messages are compared. Runtime-only incompatibilities
//! (other protocol versions, empty versions/capabilities, repeated capabilities,
//! manifest identity mismatch) are outside this decoding corpus, not waived
//! mismatches. Opaque payload semantics and correlation are not validated here.
#![cfg(unix)]

use std::{
    collections::HashSet,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

use dragonstui_adapter_host::ProtocolMessage;
use serde_json::Value;

#[derive(Debug)]
struct Case {
    name: String,
    wire: String,
}

fn add(cases: &mut Vec<Case>, name: impl Into<String>, wire: impl Into<String>) {
    cases.push(Case {
        name: name.into(),
        wire: wire.into(),
    });
}

fn extend(object: &str, fields: &str) -> String {
    format!("{},{fields}}}", object.strip_suffix('}').unwrap())
}

fn event(observation: &str) -> String {
    format!(
        r#"{{"type":"event","protocol":1,"stream":"opaque","kind":"custom","payload":null,"observation":{observation}}}"#
    )
}

fn info(fields: &str) -> String {
    extend(
        r#"{"type":"adapter_info","protocol":1,"id":"fixture","version":"opaque-version","capabilities":["test.echo"]}"#,
        fields,
    )
}

// Mutations operate on raw strings for duplicate cases: a Value roundtrip
// would erase the very evidence this test is intended to exercise.
fn object_cases(cases: &mut Vec<Case>, name: &str, raw: &str, wrap: impl Fn(&str) -> String) {
    add(cases, format!("{name}/baseline"), wrap(raw));
    let object: Value = serde_json::from_str(raw).unwrap();
    for (field, value) in object.as_object().unwrap() {
        let mut absent = object.clone();
        absent.as_object_mut().unwrap().remove(field);
        add(
            cases,
            format!("{name}/{field}/absent"),
            wrap(&absent.to_string()),
        );
        let mut nullable = object.clone();
        nullable[field] = Value::Null;
        add(
            cases,
            format!("{name}/{field}/null"),
            wrap(&nullable.to_string()),
        );
        add(
            cases,
            format!("{name}/{field}/duplicate"),
            wrap(&extend(raw, &format!("{field:?}:{value}"))),
        );
        add(
            cases,
            format!("{name}/{field}/duplicate-null"),
            wrap(&extend(&nullable.to_string(), &format!("{field:?}:null"))),
        );
    }
    add(
        cases,
        format!("{name}/additive-duplicates"),
        wrap(&extend(
            raw,
            r#""future":1,"future":{"type":false,"type":null}"#,
        )),
    );
}

fn corpus() -> Vec<Case> {
    let mut cases = Vec::new();
    for (name, raw) in [
        (
            "adapter-info",
            r#"{"type":"adapter_info","protocol":1,"id":"fixture","version":"opaque-version","capabilities":["test.echo"],"actions":[],"sessions":[]}"#,
        ),
        (
            "response",
            r#"{"type":"response","protocol":1,"id":"Request:1","payload":null}"#,
        ),
        (
            "error",
            r#"{"type":"error","protocol":1,"id":"Request:1","code":"","message":""}"#,
        ),
        (
            "event",
            r#"{"type":"event","protocol":1,"stream":"","kind":"","payload":null,"observation":null}"#,
        ),
        (
            "session-opened",
            r#"{"type":"session_opened","protocol":1,"id":"Request:1","session_id":"session-1"}"#,
        ),
        (
            "session-output",
            r#"{"type":"session_output","protocol":1,"session_id":"session-1","data":""}"#,
        ),
        (
            "session-exit",
            r#"{"type":"session_exit","protocol":1,"session_id":"session-1","exit_code":0}"#,
        ),
        ("shutdown-ack", r#"{"type":"shutdown_ack","protocol":1}"#),
    ] {
        object_cases(&mut cases, name, raw, str::to_owned);
    }
    object_cases(
        &mut cases,
        "action",
        r#"{"id":"test.action","operation":"test.echo","label":"","description":"","confirmation_required":false}"#,
        |raw| info(&format!(r#""actions":[{raw}]"#)),
    );
    object_cases(
        &mut cases,
        "session-declaration",
        r#"{"capability":"test.echo","label":"","description":""}"#,
        |raw| info(&format!(r#""sessions":[{raw}]"#)),
    );
    for (name, raw) in [
        (
            "log",
            r#"{"type":"log","text":"","severity":"info","timestamp_millis":0}"#,
        ),
        (
            "metric",
            r#"{"type":"metric","name":"","value":0,"unit":"","timestamp_millis":0}"#,
        ),
        (
            "status",
            r#"{"type":"status","entity":"","check":"","status":"ok","timestamp_millis":0}"#,
        ),
        (
            "observation-event",
            r#"{"type":"event","title":"","detail":"","timestamp_millis":0}"#,
        ),
        (
            "observation-error",
            r#"{"type":"error","message":"","signature":"","stack":[],"timestamp_millis":0}"#,
        ),
    ] {
        object_cases(&mut cases, name, raw, event);
    }

    for (index, value) in [
        "null",
        "true",
        "false",
        "0",
        "-0",
        "1.0",
        "[]",
        "{}",
        r#""opaque""#,
        r#"{"id":1,"id":2,"nested":{"type":null,"type":false}}"#,
        r#"[{"value":1,"value":2}]"#,
    ]
    .into_iter()
    .enumerate()
    {
        add(
            &mut cases,
            format!("payload/{index}"),
            format!(r#"{{"type":"response","protocol":1,"id":"r","payload":{value}}}"#),
        );
    }

    // Unit enums have externally-tagged object representations even though
    // Observation and ProtocolMessage use internally-tagged discriminators.
    for (field, variants) in [
        (
            "severity",
            &["trace", "debug", "info", "warning", "error", "critical"][..],
        ),
        ("status", &["ok", "warning", "error", "unknown"][..]),
    ] {
        for variant in variants {
            for (form, value) in [
                ("string", format!("{variant:?}")),
                ("unit-object", format!(r#"{{"{variant}":null}}"#)),
                ("nonunit-object", format!(r#"{{"{variant}":0}}"#)),
                (
                    "duplicate-object",
                    format!(r#"{{"{variant}":null,"{variant}":null}}"#),
                ),
            ] {
                let raw = if field == "severity" {
                    format!(r#"{{"type":"log","text":"","severity":{value}}}"#)
                } else {
                    format!(r#"{{"type":"status","entity":"","check":"","status":{value}}}"#)
                };
                add(&mut cases, format!("{field}/{variant}/{form}"), event(&raw));
            }
        }
    }
    for (index, value) in [
        "true",
        "false",
        "1.0",
        "-0",
        "-1",
        "0",
        "2147483647",
        "2147483648",
        "-2147483648",
        "-2147483649",
        "18446744073709551615",
        "18446744073709551616",
        "null",
        r#""1""#,
    ]
    .into_iter()
    .enumerate()
    {
        add(
            &mut cases,
            format!("exit-code/{index}"),
            format!(
                r#"{{"type":"session_exit","protocol":1,"session_id":"s","exit_code":{value}}}"#
            ),
        );
        add(
            &mut cases,
            format!("timestamp/{index}"),
            event(&format!(
                r#"{{"type":"log","text":"","timestamp_millis":{value}}}"#
            )),
        );
    }
    for (index, value) in ["true", "false", "1.0", "null", r#""1""#, "4294967296", "-1"]
        .into_iter()
        .enumerate()
    {
        add(
            &mut cases,
            format!("protocol-type/{index}"),
            format!(r#"{{"type":"shutdown_ack","protocol":{value}}}"#),
        );
    }
    for (index, value) in ["true", "false", "null", "0", "1", "1.0", r#""false""#]
        .into_iter()
        .enumerate()
    {
        add(
            &mut cases,
            format!("confirmation/{index}"),
            info(&format!(
                r#""actions":[{{"id":"a","operation":"test.echo","label":"","confirmation_required":{value}}}]"#
            )),
        );
    }

    for (index, token) in [
        String::new(),
        "a".into(),
        "0".into(),
        "a".repeat(64),
        "a".repeat(65),
        "a".repeat(128),
        "a".repeat(129),
        "A".into(),
        "_a".into(),
        "-a".into(),
        "a_b-c".into(),
        "a.b".into(),
        "a..b".into(),
        ".a".into(),
        "a.".into(),
        "A:1._-".into(),
        "a\n".into(),
        "é".into(),
        "🐉".into(),
        format!("{}.{}", "a".repeat(64), "b".repeat(64)),
        format!("{}.b", "a".repeat(65)),
    ]
    .into_iter()
    .enumerate()
    {
        let token = serde_json::to_string(&token).unwrap();
        for (kind, wire) in [
            (
                "adapter",
                format!(
                    r#"{{"type":"adapter_info","protocol":1,"id":{token},"version":"v","capabilities":["test.echo"]}}"#
                ),
            ),
            (
                "request",
                format!(r#"{{"type":"response","protocol":1,"id":{token},"payload":null}}"#),
            ),
            (
                "session",
                format!(
                    r#"{{"type":"session_output","protocol":1,"session_id":{token},"data":""}}"#
                ),
            ),
            (
                "capability",
                format!(
                    r#"{{"type":"adapter_info","protocol":1,"id":"a","version":"v","capabilities":[{token}]}}"#
                ),
            ),
            (
                "action",
                info(&format!(
                    r#""actions":[{{"id":{token},"operation":"test.echo","label":""}}]"#
                )),
            ),
        ] {
            add(&mut cases, format!("identifier/{kind}/{index}"), wire);
        }
    }

    // Raw lexical cases cannot be roundtripped through Value before decode.
    for (index, number) in [
        "0.0",
        "-0.0",
        "5e-324",
        "1e-324",
        "1e-9999",
        "0e9999",
        "1.7976931348623157e308",
        "1.7976931348623158e308",
        "1.7976931348623159e308",
        "1e308",
        "1e309",
        "-1e309",
        "NaN",
        "Infinity",
        "-Infinity",
        "18446744073709551615",
        "18446744073709551616",
        "-9223372036854775809",
        "1e+",
        "01",
        "+1",
        ".1",
    ]
    .into_iter()
    .enumerate()
    {
        add(
            &mut cases,
            format!("number/payload/{index}"),
            format!(r#"{{"type":"response","protocol":1,"id":"r","payload":{number}}}"#),
        );
        add(
            &mut cases,
            format!("number/metric/{index}"),
            event(&format!(
                r#"{{"type":"metric","name":"","value":{number}}}"#
            )),
        );
        add(
            &mut cases,
            format!("number/additive/{index}"),
            format!(r#"{{"type":"shutdown_ack","protocol":1,"future":{number}}}"#),
        );
    }
    for (index, text) in [
        r#""\ud83d\udc09""#,
        r#""\ud800""#,
        r#""\udc00""#,
        r#""\ud800x""#,
        r#""\udc00\ud800""#,
        r#""\u0000""#,
        r#""龍🐉""#,
    ]
    .into_iter()
    .enumerate()
    {
        add(
            &mut cases,
            format!("unicode/typed/{index}"),
            format!(r#"{{"type":"session_output","protocol":1,"session_id":"s","data":{text}}}"#),
        );
        add(
            &mut cases,
            format!("unicode/payload/{index}"),
            format!(r#"{{"type":"response","protocol":1,"id":"r","payload":{text}}}"#),
        );
        add(
            &mut cases,
            format!("unicode/key/{index}"),
            format!(r#"{{"type":"shutdown_ack","protocol":1,{text}:null}}"#),
        );
        add(
            &mut cases,
            format!("unicode/overwritten/{index}"),
            format!(
                r#"{{"type":"response","protocol":1,"id":"r","payload":{{"x":{text},"x":null}}}}"#
            ),
        );
    }
    for (name, raw) in [
        ("not-object", "[]"),
        ("null-envelope", "null"),
        ("unknown-tag", r#"{"type":"future","protocol":1}"#),
        (
            "object-tag",
            r#"{"type":{"shutdown_ack":null},"protocol":1}"#,
        ),
        (
            "trailing-json",
            r#"{"type":"shutdown_ack","protocol":1} {}"#,
        ),
        (
            "escaped-duplicate",
            r#"{"type":"shutdown_ack","protocol":1,"proto\u0063ol":1}"#,
        ),
        ("tag-last", r#"{"protocol":1,"type":"shutdown_ack"}"#),
    ] {
        add(&mut cases, name, raw);
    }
    cases
}

#[test]
fn actual_rust_serde_and_python_m67_accept_the_same_wire_corpus() {
    let cases = corpus();
    assert_eq!(
        cases
            .iter()
            .map(|case| &case.name)
            .collect::<HashSet<_>>()
            .len(),
        cases.len(),
        "case names must be unique"
    );
    assert!(cases.len() <= 4096);
    assert!(cases.iter().all(|case| case.wire.len() <= 16384));
    let tools = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools");
    let mut child = Command::new("python3")
        .arg("-B")
        .arg(tools.join("fixtures/conformance_wire_probe.py"))
        .arg(&tools)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("wire parity tests require python3 on Unix");
    let raw: Vec<_> = cases.iter().map(|case| &case.wire).collect();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&raw).unwrap())
        .expect("write finite wire corpus");
    let output = child
        .wait_with_output()
        .expect("wait for Python wire probe");
    assert!(
        output.status.success(),
        "Python wire probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let python: Vec<bool> =
        serde_json::from_slice(&output.stdout).expect("Python probe must return a boolean array");
    assert_eq!(
        python.len(),
        cases.len(),
        "Python must test every wire case"
    );
    let mut mismatches = Vec::new();
    let mut accepted = 0;
    let mut rejected = 0;
    for (case, python_accepts) in cases.iter().zip(python) {
        let rust = serde_json::from_str::<ProtocolMessage>(&case.wire);
        if rust.is_ok() {
            accepted += 1;
        } else {
            rejected += 1;
        }
        if rust.is_ok() != python_accepts {
            mismatches.push(format!(
                "{}: Rust={} Python={}\n  wire={}\n  serde={rust:?}",
                case.name,
                rust.is_ok(),
                python_accepts,
                case.wire
            ));
        }
    }
    assert!(
        accepted > 0 && rejected > 0,
        "corpus must exercise both outcomes"
    );
    eprintln!(
        "wire parity: {} cases; Rust accepted {accepted}, rejected {rejected}; {} mismatches",
        cases.len(),
        mismatches.len()
    );
    assert!(
        mismatches.is_empty(),
        "actual wire decoding parity mismatches:\n{}",
        mismatches.join("\n")
    );
}
