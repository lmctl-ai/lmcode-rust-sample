use dslvm::Registry;
#[test]
fn return_signatures_are_complete() {
    let signatures = Registry::new().signatures();
    let types = [
        "int",
        "float",
        "bool",
        "string",
        "Json",
        "StrList",
        "IntList",
        "FloatList",
        "StrMap",
        "IntMap",
        "FloatMap",
    ];
    for t in types {
        for sig in [
            format!("fn lmcode_return_set({t}) -> void"),
            format!("fn lmcode_return_put(string, {t}) -> void"),
        ] {
            assert!(signatures.contains(&sig), "missing {sig}");
        }
    }
    assert!(signatures.contains(&"fn lmcode_return_null() -> void".into()));
    assert!(signatures.contains(&"fn lmcode_return_put_null(string) -> void".into()));
    assert_eq!(
        signatures
            .iter()
            .filter(|s| s.contains("lmcode_return_"))
            .count(),
        24
    );
}

#[test]
fn owned_sink_numeric_and_lifecycle_contract() {
    use dslvm::stdlib::result::{put_result, set_result, ResultSink};
    use dslvm::{Context, Value};
    let mut ctx = Context::new();
    assert_eq!(
        set_result(&mut ctx, &Value::Int(1)).unwrap_err().message,
        "lmcode return: no result sink"
    );
    ctx.result_sink = Some(ResultSink::default());
    set_result(&mut ctx, &Value::Float(0.1 + 0.2)).unwrap();
    assert_eq!(
        ctx.result_sink.as_ref().unwrap().frame(),
        "LMCODE_RETURN_BEGIN\n0.30000000000000004\nLMCODE_RETURN_END\n"
    );
    for v in [
        Value::Int(9007199254740992),
        Value::Float(f64::INFINITY),
        Value::string(vec![255]),
    ] {
        assert!(set_result(&mut ctx, &v).is_err());
    }
    assert_eq!(
        ctx.result_sink.as_ref().unwrap().json(),
        "0.30000000000000004"
    );
    assert!(put_result(&mut ctx, b"field", &Value::Int(1)).is_err());
    set_result(&mut ctx, &Value::None).unwrap();
    put_result(&mut ctx, b"a", &Value::Int(1)).unwrap();
    put_result(&mut ctx, b"b", &Value::None).unwrap();
    put_result(&mut ctx, b"a", &Value::Int(2)).unwrap();
    assert_eq!(
        ctx.result_sink.as_ref().unwrap().json(),
        "{\"a\":2,\"b\":null}"
    );
    assert_eq!(
        ctx.result_sink.as_ref().unwrap().frame_if_success(false),
        None
    );
    ctx.result_sink.as_mut().unwrap().reset();
    assert_eq!(ctx.result_sink.as_ref().unwrap().json(), "null");
}

#[test]
fn concurrent_invocations_keep_distinct_values() {
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let threads: Vec<_> = (1..=2)
        .map(|n| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut ctx = dslvm::Context::new();
                ctx.result_sink = Some(dslvm::stdlib::ResultSink::default());
                dslvm::stdlib::result::set_result(&mut ctx, &dslvm::Value::Int(n)).unwrap();
                barrier.wait();
                assert_eq!(ctx.result_sink.as_ref().unwrap().json(), n.to_string());
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
}

#[test]
fn shortest_float_and_escaped_frame_payloads() {
    use dslvm::{Context, Value};
    let mut ctx = Context::new();
    ctx.result_sink = Some(dslvm::stdlib::ResultSink::default());
    for (value, want) in [
        (Value::Float(-0.0), "0"),
        (Value::Float(f64::from_bits(1)), "5e-324"),
        (Value::Float(1.0), "1"),
        (
            Value::string("LMCODE_RETURN_END\n\"\\"),
            "\"LMCODE_RETURN_END\\n\\\"\\\\\"",
        ),
    ] {
        dslvm::stdlib::result::set_result(&mut ctx, &value).unwrap();
        assert_eq!(ctx.result_sink.as_ref().unwrap().json(), want);
    }
}
