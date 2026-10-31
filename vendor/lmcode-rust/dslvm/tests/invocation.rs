use dslvm::stdlib::result::{put_result, set_result};
use dslvm::stdlib::{InvocationResults as Results, ResultContract as Contract};
use dslvm::{Context, Error, Value};
use std::io::{self, Write};
fn results(n: usize) -> Results {
    Results::from_command_with_manifest(&(0..n).map(|_| "same".into()).collect::<Vec<_>>(), None)
        .unwrap()
}
fn contract(t: &str) -> Contract {
    Contract::from_schema(
        format!("{{\"dsl.schema\":2,\"result\":{{\"type\":\"{t}\"}}}}").as_bytes(),
    )
    .unwrap()
}
fn framed(s: &str) -> String {
    format!("LMCODE_RETURN_BEGIN\n{s}\nLMCODE_RETURN_END\n")
}
#[test]
fn ordered_distinct_slots_and_active_reselection() {
    let mut r = results(2);
    let mut ctx = Context::new();
    r.select(1, &mut ctx, Contract::default()).unwrap();
    put_result(&mut ctx, b"body", &Value::Int(2)).unwrap();
    r.select(0, &mut ctx, Contract::default()).unwrap();
    set_result(&mut ctx, &Value::Float(0.1 + 0.2)).unwrap();
    r.select(1, &mut ctx, Contract::default()).unwrap();
    assert_eq!(ctx.result_sink.as_ref().unwrap().json(), "{\"body\":2}");
    put_result(&mut ctx, b"entry", &Value::Bool(true)).unwrap();
    r.complete(1).unwrap();
    r.complete(0).unwrap();
    let mut out = Vec::new();
    assert_eq!(r.write_frame(&mut out), 0);
    assert_eq!(String::from_utf8(out.clone()).unwrap(),framed("{\"ok\":true,\"result\":[0.30000000000000004,{\"body\":2,\"entry\":true}],\"parts\":2}"));
    assert_eq!(r.write_frame(&mut out), 4);
    assert_eq!(out.iter().filter(|&&b| b == b'\n').count(), 3);
    assert!(r.select(0, &mut ctx, Default::default()).is_err());
    assert!(r.fail(None, "app", Error::host("late")).is_err());
}
#[test]
fn seal_shared_with_context_and_atomic_contracts() {
    let mut r = results(1);
    let mut ctx = Context::new();
    r.select(0, &mut ctx, contract("IntMap")).unwrap();
    put_result(&mut ctx, b"x", &Value::Int(7)).unwrap();
    assert!(put_result(&mut ctx, b"x", &Value::Float(2.0))
        .unwrap_err()
        .message
        .contains("expected int; got float"));
    assert!(put_result(&mut ctx, b"n", &Value::None).is_err());
    assert!(set_result(&mut ctx, &Value::None).is_err());
    assert_eq!(ctx.result_sink.as_ref().unwrap().json(), "{\"x\":7}");
    assert!(r.select(0, &mut ctx, contract("Json")).is_err());
    r.complete(0).unwrap();
    assert!(set_result(&mut ctx, &Value::Int(1))
        .unwrap_err()
        .message
        .contains("sealed"));
    assert!(put_result(&mut ctx, b"x", &Value::Int(2)).is_err());
    assert!(ctx.result_sink.as_ref().unwrap().reset().is_err());
    assert!(r.select(0, &mut ctx, contract("IntMap")).is_err());
    assert!(r.complete(0).is_err());
}
#[test]
fn failure_keeps_first_association_and_blocks_new_work() {
    let mut r = results(3);
    let mut ctx = Context::new();
    r.select(0, &mut ctx, Default::default()).unwrap();
    let mut e = Error::host("bad");
    e.locate(4, 2);
    r.fail(Some(1), "run", e).unwrap();
    r.fail(Some(999), "bogus", Error::host("ignored")).unwrap();
    assert!(r.select(2, &mut ctx, Default::default()).is_err());
    r.select(0, &mut ctx, Default::default()).unwrap();
    r.complete(0).unwrap();
    let mut out = Vec::new();
    assert_eq!(r.write_frame(&mut out), 3);
    assert_eq!(String::from_utf8(out).unwrap(),framed("{\"ok\":false,\"phase\":\"run\",\"code\":\"RUN_HOST\",\"message\":\"dsl line 4:2: bad\",\"line\":4,\"column\":2,\"part\":1}"));
}
#[test]
fn missing_completion_and_completion_error() {
    let mut r = results(2);
    let mut ctx = Context::new();
    r.select(1, &mut ctx, contract("int")).unwrap();
    assert!(r.complete(1).is_err());
    let mut out = Vec::new();
    assert_eq!(r.write_frame(&mut out), 3);
    let s = String::from_utf8(out).unwrap();
    assert!(s.contains("expected int; got null"));
    assert!(s.contains("\"part\":1"));
    assert!(!s.contains("\"result\""));
    let mut r = results(2);
    r.select(0, &mut ctx, Default::default()).unwrap();
    r.complete(0).unwrap();
    let mut out = Vec::new();
    assert_eq!(r.write_frame(&mut out), 3);
    assert!(String::from_utf8(out)
        .unwrap()
        .contains("slot not completed"));
}
#[derive(Default)]
struct Writer {
    data: Vec<u8>,
    flushes: usize,
    fail_flush: bool,
    fail_write: bool,
}
impl Write for Writer {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.fail_write {
            if self.data.is_empty() {
                self.data.push(b[0]);
                return Ok(1);
            }
            return Err(io::Error::other("broken"));
        }
        self.data.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        if self.fail_flush {
            Err(io::Error::other("flush"))
        } else {
            Ok(())
        }
    }
}
#[test]
fn writer_flush_and_partial_failure_are_single_shot() {
    for (fail_write, fail_flush) in [(false, false), (false, true), (true, false)] {
        let mut r = results(1);
        let mut c = Context::new();
        r.select(0, &mut c, Default::default()).unwrap();
        r.complete(0).unwrap();
        let mut w = Writer {
            fail_write,
            fail_flush,
            ..Default::default()
        };
        assert_eq!(
            r.write_frame(&mut w),
            if fail_write || fail_flush { 4 } else { 0 }
        );
        assert_eq!(w.flushes, if fail_write { 0 } else { 1 });
        let before = w.data.clone();
        assert_eq!(r.write_frame(&mut w), 4);
        assert_eq!(w.data, before);
    }
}
#[test]
fn schema_metadata_and_json_source_shapes() {
    for raw in [
        "{}",
        "{\"dsl.schema\":1}",
        "{\"dsl.schema\":2,\"result\":{}}",
        "{\"dsl.schema\":2,\"result\":{\"type\":\"null\",\"nullable\":false}}",
        "{\"dsl.schema\":2,\"result\":{\"type\":\"void\"}}",
        "{\"dsl.schema\":2,\"result\":{\"type\":\"int\",\"x\":0}}",
        "{\"dsl.schema\":2,\"result\":{\"type\":\"int\",\"nullable\":0}}",
    ] {
        assert_eq!(
            Contract::from_schema(raw.as_bytes()).unwrap_err().code,
            "LOAD_BAD_SCHEMA"
        );
    }
    let mut r = results(1);
    let mut c = Context::new();
    r.select(0, &mut c, contract("int")).unwrap();
    assert!(set_result(&mut c, &Value::Float(2.0)).is_err());

    for t in [
        "null",
        "bool",
        "int",
        "float",
        "string",
        "Json",
        "StrList",
        "IntList",
        "FloatList",
        "StrMap",
        "IntMap",
        "FloatMap",
    ] {
        assert_eq!(contract(t).result_type(), t)
    }
}
#[test]
fn manifest_strictness_and_single_builder_distinction() {
    let a = std::env::current_dir()
        .unwrap()
        .join("A space 😀")
        .to_str()
        .unwrap()
        .to_owned();
    let b = std::env::current_dir()
        .unwrap()
        .join("B")
        .to_str()
        .unwrap()
        .to_owned();
    let raw=format!("{{\"version\":1,\"mode\":\"builder\",\"parts\":[{{\"index\":0,\"file\":{}}},{{\"index\":1,\"file\":{}}}]}}",dslvm::json_escape(&a),dslvm::json_escape(&b));
    let r = Results::from_metadata(raw.as_bytes(), &[b.clone(), a.clone()]).unwrap();
    assert_eq!(r.registrations()[0].index, 1);
    for files in [
        vec![a.clone()],
        vec![a.clone(), a.clone()],
        vec![a.clone(), "other".into()],
    ] {
        assert!(Results::from_metadata(raw.as_bytes(), &files).is_err())
    }
    for bad in [
        raw.replace("\"version\":1", "\"version\":2"),
        raw.replace("\"version\":1", "\"version\":1,\"version\":1"),
        raw.replace("\"builder\"", "\"single\""),
        raw.replace("\"index\":1", "\"index\":0"),
        raw.replace("\"parts\":", "\"other\":0,\"parts\":"),
        raw.replace(&dslvm::json_escape(&b), "\"relative\""),
        raw.replace(&dslvm::json_escape(&b), &dslvm::json_escape(&a)),
    ] {
        assert!(
            Results::from_metadata(bad.as_bytes(), &[a.clone(), b.clone()]).is_err(),
            "{bad}"
        );
    }
    let raw = format!(
        "{{\"version\":1,\"mode\":\"builder\",\"parts\":[{{\"index\":0,\"file\":{}}}]}}",
        dslvm::json_escape(&a)
    );
    let mut r = Results::from_metadata(raw.as_bytes(), &[a]).unwrap();
    let mut c = Context::new();
    r.select(0, &mut c, Default::default()).unwrap();
    r.complete(0).unwrap();
    let mut out = Vec::new();
    assert_eq!(r.write_frame(&mut out), 0);
    assert_eq!(
        String::from_utf8(out).unwrap(),
        framed("{\"ok\":true,\"result\":[null],\"parts\":1}")
    );
    assert!(Results::from_command_with_manifest(&[], None).is_err());
    assert!(
        Results::from_command_with_manifest(&["x".into()], Some(std::path::Path::new(""))).is_err()
    );
}

#[test]
fn typed_slots_nullable_and_known_collection_distinctions() {
    use dslvm::stdlib::collections::{IntList, StrList};
    let mut r = results(2);
    let mut c = Context::new();
    r.select(0, &mut c, contract("FloatList")).unwrap();
    let strings = c
        .insert_collection("StrList", StrList { items: vec![] })
        .unwrap();
    assert!(set_result(&mut c, &Value::object("StrList", strings))
        .unwrap_err()
        .message
        .contains("expected FloatList; got StrList"));
    let ints = c
        .insert_collection("IntList", IntList { items: vec![1, 2] })
        .unwrap();
    set_result(&mut c, &Value::object("IntList", ints)).unwrap();
    r.complete(0).unwrap();
    let nullable =
        Contract::from_schema(br#"{"dsl.schema":2,"result":{"type":"IntMap","nullable":true}}"#)
            .unwrap();
    r.select(1, &mut c, nullable).unwrap();
    set_result(&mut c, &Value::None).unwrap();
    put_result(&mut c, b"x", &Value::Int(1)).unwrap();
    assert!(put_result(&mut c, b"~/x", &Value::None)
        .unwrap_err()
        .message
        .contains("/~0~1x: expected int; got null"));
    r.complete(1).unwrap();
    let mut out = Vec::new();
    assert_eq!(r.write_frame(&mut out), 0);
    assert_eq!(
        String::from_utf8(out).unwrap(),
        framed("{\"ok\":true,\"result\":[[1,2],{\"x\":1}],\"parts\":2}")
    );
}

#[test]
fn canonical_float_layout_and_unlocated_failure_validation() {
    for (n, want) in [
        (0.00001, "1e-05"),
        (1000000.0, "1e+06"),
        (1.25e-10, "1.25e-10"),
        (0.0001, "0.0001"),
        (-0.0, "0"),
    ] {
        let mut c = Context::new();
        let mut r = results(1);
        r.select(0, &mut c, Default::default()).unwrap();
        set_result(&mut c, &Value::Float(n)).unwrap();
        r.complete(0).unwrap();
        let mut out = Vec::new();
        assert_eq!(r.write_frame(&mut out), 0);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            framed(&format!("{{\"ok\":true,\"result\":{want}}}"))
        );
    }
    let mut r = results(1);
    let mut e = Error::host("bad");
    e.column = Some(2);
    assert!(r.fail(None, "run", e).is_err());
    assert!(r.fail(None, "unknown", Error::host("bad")).is_err());
    r.fail(Some(0), "app", Error::new("APP_FAIL", "global"))
        .unwrap();
    let mut out = Vec::new();
    assert_eq!(r.write_frame(&mut out), 4);
    assert_eq!(
        String::from_utf8(out).unwrap(),
        framed("{\"ok\":false,\"phase\":\"app\",\"code\":\"APP_FAIL\",\"message\":\"global\"}")
    );
}
