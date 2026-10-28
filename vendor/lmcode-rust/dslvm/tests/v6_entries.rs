use dslvm::{sha256, Program, Registry};
const SCHEMA: &[u8] = b"v6 entry unit schema";
fn u16b(b: &mut Vec<u8>, n: u16) {
    b.extend(n.to_be_bytes());
}
fn u32b(b: &mut Vec<u8>, n: u32) {
    b.extend(n.to_be_bytes());
}
fn fixture(params: u16, ty: u32, hidden: u8, iface: &str, code: &[u8]) -> Vec<u8> {
    let mut b = b"DSLB\0\x06".to_vec();
    b.extend(sha256(SCHEMA));
    let strings = ["rule", "pair", iface, "int", "float"];
    u32b(&mut b, strings.len() as u32);
    for s in strings {
        b.push(1);
        u32b(&mut b, s.len() as u32);
        b.extend(s.as_bytes());
    }
    u16b(&mut b, 0);
    u16b(&mut b, 0); // imports, context
    u16b(&mut b, 0);
    u16b(&mut b, 0);
    u32b(&mut b, 1);
    b.push(0x11);
    u32b(&mut b, 0); // body
    u16b(&mut b, 1);
    u32b(&mut b, 0);
    u32b(&mut b, 1);
    u32b(&mut b, 2);
    u16b(&mut b, params);
    u16b(&mut b, 1);
    u32b(&mut b, ty);
    b.push(hidden);
    u16b(&mut b, 2);
    u32b(&mut b, code.len() as u32);
    b.extend(code);
    u32b(&mut b, 1);
    u32b(&mut b, 0);
    u32b(&mut b, 7);
    u16b(&mut b, 9);
    b
}
fn refusal(b: Vec<u8>, text: &str) {
    let e = Program::load(&b, SCHEMA, &Registry::without_std()).expect_err("must refuse");
    assert_eq!(e.code, "LOAD_BAD_BYTECODE");
    assert!(
        e.message.contains(text),
        "expected {text:?}, got {}",
        e.message
    );
}
#[test]
fn refuses_parameter_count() {
    refusal(
        fixture(0, 3, 0, "interface Step(int) -> int", &[4, 0, 0, 17]),
        "entry 'rule' declares 0 parameter(s); interface Step(int) -> int has 1",
    );
}
#[test]
fn refuses_parameter_type() {
    refusal(
        fixture(1, 4, 0, "interface Step(int) -> int", &[4, 0, 0, 17]),
        "entry 'rule' parameter 1 has type float; the interface expects int",
    );
}
#[test]
fn refuses_hidden_parameter() {
    refusal(
        fixture(1, 3, 1, "interface Step(int) -> int", &[4, 0, 0, 17]),
        "entry 'rule' parameter 1 is hidden",
    );
}
#[test]
fn refuses_value_halt_depth_without_loops() {
    refusal(
        fixture(1, 3, 0, "interface Step(int) -> int", &[17]),
        "entry 'rule': HALT leaves 0 values",
    );
}
#[test]
fn refuses_void_halt_depth_without_loops() {
    refusal(
        fixture(1, 3, 0, "interface Visit(int) -> void", &[4, 0, 0, 17]),
        "entry 'rule': HALT leaves 1 values",
    );
}
#[test]
fn refuses_entry_jump_out() {
    refusal(
        fixture(
            1,
            3,
            0,
            "interface Step(int) -> int",
            &[12, 0, 0, 0, 99, 4, 0, 0, 17],
        ),
        "entry 'rule': jump",
    );
}
#[test]
fn refuses_malformed_interface() {
    refusal(
        fixture(1, 3, 0, "not an interface", &[4, 0, 0, 17]),
        "entry 'rule': interface",
    );
}
#[test]
fn refuses_entry_local_index() {
    refusal(
        fixture(1, 3, 0, "interface Step(int) -> int", &[4, 0, 1, 17]),
        "entry 'rule': local slot index 1 out of range",
    );
}
#[test]
fn refuses_invalid_type_constant() {
    refusal(
        fixture(1, 99, 0, "interface Step(int) -> int", &[4, 0, 0, 17]),
        "entry 'rule': local type: constant index 99",
    );
}
#[test]
fn refuses_v5() {
    let mut b = b"DSLB\0\x05".to_vec();
    b.extend(sha256(SCHEMA));
    u32b(&mut b, 0);
    for _ in 0..4 {
        u16b(&mut b, 0);
    }
    u32b(&mut b, 1);
    b.push(17);
    u32b(&mut b, 0);
    refusal(
        b,
        "unsupported bytecode version 5 (this runtime reads version 6)",
    );
}
#[test]
fn entry_api_and_call_refusals() {
    use dslvm::{Context, Value};
    let reg = Registry::without_std();
    let p = Program::load(
        &fixture(1, 3, 0, "interface Step(int) -> int", &[4, 0, 0, 17]),
        SCHEMA,
        &reg,
    )
    .unwrap();
    let e = &p.entries()[0];
    assert_eq!(
        (&*e.name, &*e.annotation, &*e.iface, &*e.returns),
        ("rule", "pair", "interface Step(int) -> int", "int")
    );
    assert_eq!(e.params, vec!["int"]);
    let mut ctx = Context::new();
    assert_eq!(
        p.call(0, &mut ctx, &reg, vec![Value::Int(42)]).unwrap(),
        Value::Int(42)
    );
    assert_eq!(
        p.call(0, &mut ctx, &reg, vec![Value::Bool(true)]).unwrap(),
        Value::Bool(true),
        "host argument kinds and return value are untyped"
    );
    p.run(&mut ctx, &reg).unwrap();
    assert_eq!(
        p.call(0, &mut ctx, &reg, vec![Value::Int(7)]).unwrap(),
        Value::Int(7)
    );
    let error = p.call(0, &mut ctx, &reg, vec![]).unwrap_err();
    assert_eq!(error.code, "RUN_HOST");
    assert_eq!(
        error.message,
        "call entry 'rule' expects 1 argument(s); found 0"
    );
    assert_eq!(
        p.call(2, &mut ctx, &reg, vec![]).unwrap_err().code,
        "RUN_HOST"
    );
}
#[test]
fn entry_error_uses_own_line_table() {
    use dslvm::{Context, Value};
    let reg = Registry::without_std();
    let mut code = vec![4, 0, 0, 1];
    code.extend(0i64.to_be_bytes());
    code.extend([0x16, 17]);
    let p = Program::load(
        &fixture(1, 3, 0, "interface Step(int) -> int", &code),
        SCHEMA,
        &reg,
    )
    .unwrap();
    let error = p
        .call(0, &mut Context::new(), &reg, vec![Value::Int(3)])
        .unwrap_err();
    assert_eq!(error.code, "RUN_ERROR");
    assert_eq!((error.line, error.column), (Some(7), Some(9)));
}
#[test]
fn void_entry_returns_none() {
    use dslvm::{Context, Value};
    let reg = Registry::without_std();
    let p = Program::load(
        &fixture(1, 3, 0, "interface Visit(int) -> void", &[17]),
        SCHEMA,
        &reg,
    )
    .unwrap();
    assert_eq!(
        p.call(0, &mut Context::new(), &reg, vec![Value::Int(0)])
            .unwrap(),
        Value::None
    );
}
