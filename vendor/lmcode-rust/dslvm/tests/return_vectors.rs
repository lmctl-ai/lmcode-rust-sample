//! Integration controls for the canonical invocation-result fixtures.
//! These deliberately exercise the same body/entry ordering as the host
//! protocol: one sink survives every selected entry call and a frame is
//! considered only after all calls succeed.
use dslvm::{Context, Error, Program, Registry, Value};
use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/return-vectors")
}

fn registry() -> Registry {
    let mut reg = Registry::new();
    reg.function("entry_fail", &[], "void", |_, _| {
        Err(Error::host("entry failure"))
    })
    .unwrap();
    reg.function("expect_result", &["string"], "void", |ctx, args| {
        let want = args[0].as_bytes()?;
        let got = ctx
            .result_sink
            .as_ref()
            .ok_or_else(|| Error::host("expect_result: no sink"))?
            .json();
        if got.as_bytes() != want {
            return Err(Error::host(format!(
                "return payload mismatch: expected {:?}, got {:?}",
                String::from_utf8_lossy(want),
                got
            )));
        }
        Ok(Value::None)
    })
    .unwrap();
    reg
}

fn fixture(name: &str, expect: &str, repeat: usize, should_fail: bool) {
    let dir = root();
    let program = Program::load(
        &fs::read(dir.join(format!("{name}.dslb"))).unwrap(),
        &fs::read(dir.join("schema.json")).unwrap(),
        &registry(),
    )
    .unwrap();
    let reg = registry();
    let mut ctx = Context::new();
    let mut results =
        dslvm::stdlib::InvocationResults::from_command_with_manifest(&[name.into()], None).unwrap();
    results.select(0, &mut ctx, Default::default()).unwrap();
    let body = program.run(&mut ctx, &reg);
    if should_fail {
        let error = if name == "entry-error" {
            body.unwrap_or_else(|e| panic!("{name}: body failed before entry: {e}"));
            program.call(0, &mut ctx, &reg, vec![]).unwrap_err()
        } else {
            body.unwrap_err()
        };
        assert_eq!(error.code, "RUN_HOST", "{name}: error code");
        let expected = match name {
            "entry-error" => "entry failure",
            "unsafe-json-integer" => "invalid JSON value at /amount: unsafe_integer",
            "scalar-field" => "field assignment requires object or null",
            "invalid-utf8" => "invalid JSON value at : utf8",
            _ => unreachable!(),
        };
        assert!(
            error.message.contains(expected),
            "{name}: unexpected error: {}",
            error.message
        );
        assert!(
            error.message.contains("dsl line "),
            "{name}: missing source location: {}",
            error.message
        );
        results.fail(Some(0), "run", error).unwrap();
        let mut out = Vec::new();
        assert_eq!(results.write_frame(&mut out), 3);
        assert_eq!(out, fs::read(dir.join(format!("{name}.expect"))).unwrap());
        return;
    }
    body.unwrap_or_else(|e| panic!("{name}: body failed: {e}"));
    for _ in 0..repeat {
        for entry in 0..program.entries().len() {
            let result = program.call(entry, &mut ctx, &reg, vec![]);
            let value = result.unwrap_or_else(|e| panic!("{name}: entry {entry} failed: {e}"));
            assert_eq!(value, Value::Int(7), "{name}: entry {entry} HALT result");
        }
    }
    let sink = ctx.result_sink.as_ref().unwrap();
    assert_eq!(sink.json(), expect, "{name}: payload");
    let expected_frame = fs::read_to_string(dir.join(format!("{name}.expect"))).unwrap();
    results.complete(0).unwrap();
    let mut out = Vec::new();
    assert_eq!(results.write_frame(&mut out), 0);
    assert_eq!(String::from_utf8(out).unwrap(), expected_frame);
}

#[test]
fn canonical_success_and_failure_vectors() {
    fixture("float-roundtrip", "0.30000000000000004", 1, false);
    fixture("entry-sink", r#"{"body":"body","entry":"entry"}"#, 1, false);
    fixture(
        "repeated-calls",
        r#"{"program":"first","entry":"called"}"#,
        2,
        false,
    );
    fixture("unset-null", "null", 1, false);
    fixture("all-overloads", "null", 1, false);
    fixture("entry-error", "", 1, true);
    fixture("unsafe-json-integer", "", 1, true);
    fixture("scalar-field", "", 1, true);
    fixture("invalid-utf8", "", 1, true);
}

#[test]
fn separate_invocations_have_separate_sinks() {
    let expected = "0.30000000000000004";
    let threads: Vec<_> = (0..2)
        .map(|_| std::thread::spawn(move || fixture("float-roundtrip", expected, 1, false)))
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
}

#[test]
fn canonical_fixture_hashes_match_lock() {
    let dir = root();
    let lock = fs::read_to_string(dir.join("vectors.lock")).unwrap();
    let mut count = 0;
    for line in lock.lines() {
        let (want, name) = line.split_once("  ").expect("sha256sum lock line");
        let actual = dslvm::hex(&dslvm::sha256(&fs::read(dir.join(name)).unwrap()));
        assert_eq!(actual, want, "fixture lock entry {name}");
        count += 1;
    }
    assert!(count >= 25, "canonical fixture lock is unexpectedly small");
}
