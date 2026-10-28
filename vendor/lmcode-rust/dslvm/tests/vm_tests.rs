//! Mirrors lmcode-cpp tests/dslvm_test.cpp (bytecode v5) plus robustness: every truncation length and bit flips of a valid
//! file must produce an `Err`, never a panic.
use dslvm::*;

const SCHEMA: &[u8] = br#"{"dsl.schema":1,"name":"t"}"#;
const T_INT: &str = "method Trace.int(int) -> void";
const T_FLOAT: &str = "method Trace.float(float) -> void";
const T_FLOAT_BITS: &str = "method Trace.float_bits(float) -> void";
const T_STR: &str = "method Trace.str(string) -> void";
const THING_ID: &str = "method Thing.id() -> int";
const TWICE: &str = "fn twice(int) -> int";
const BOOM: &str = "fn boom() -> void";

#[derive(Default)]
struct Asm { consts: Vec<Vec<u8>>, imports: Vec<u32>, contexts: Vec<(u32, u32)>, locals: Vec<(u32, u8)>, max_stack: u16, code: Vec<u8>, lines: Vec<(u32, u32, u16)> }
impl Asm {
    fn k(&mut self, s: &str) -> u32 { self.k_bytes(s.as_bytes()) }
    fn k_bytes(&mut self, s: &[u8]) -> u32 { if let Some(i) = self.consts.iter().position(|c| c == s) { return i as u32; } self.consts.push(s.to_vec()); (self.consts.len() - 1) as u32 }
    fn imp(&mut self, canonical: &str) -> u16 { let c = self.k(canonical); if let Some(i) = self.imports.iter().position(|x| *x == c) { return i as u16; } self.imports.push(c); (self.imports.len() - 1) as u16 }
    fn ctx_index(&mut self, name: &str, type_name: &str) -> u16 { let e = (self.k(name), self.k(type_name)); if let Some(i) = self.contexts.iter().position(|x| *x == e) { return i as u16; } self.contexts.push(e); (self.contexts.len() - 1) as u16 }
    fn local(mut self, type_name: &str, hidden: u8) -> Self { let k = self.k(type_name); self.locals.push((k, hidden)); self }
    fn line(mut self, pc: u32, line: u32, col: u16) -> Self { self.lines.push((pc, line, col)); self }
    fn u8(mut self, v: u8) -> Self { self.code.push(v); self }
    fn u16(mut self, v: u16) -> Self { self.code.extend_from_slice(&v.to_be_bytes()); self }
    fn u32(mut self, v: u32) -> Self { self.code.extend_from_slice(&v.to_be_bytes()); self }
    fn i32(mut self, v: i32) -> Self { self.code.extend_from_slice(&v.to_be_bytes()); self }
    fn i64(mut self, v: i64) -> Self { self.code.extend_from_slice(&v.to_be_bytes()); self }
    fn f64(mut self, v: f64) -> Self { self.code.extend_from_slice(&v.to_be_bytes()); self }
    fn push_int(self, v: i64) -> Self { self.u8(0x01).i64(v) }
    fn push_float(self, v: f64) -> Self { self.u8(0x12).f64(v) }
    fn push_str(self, s: &str) -> Self { self.push_bytes(s.as_bytes()) }
    fn push_bytes(mut self, s: &[u8]) -> Self { let k = self.k_bytes(s); self.u8(0x02).u32(k) }
    fn push_bool(self, b: bool) -> Self { self.u8(0x03).u8(b as u8) }
    fn ctx(mut self, n: &str, t: &str) -> Self { let i = self.ctx_index(n, t); self.u8(0x06).u16(i) }
    fn func(mut self, canonical: &str, argc: u8) -> Self { let i = self.imp(canonical); self.u8(0x07).u16(i).u8(argc) }
    fn method(mut self, canonical: &str, argc: u8) -> Self { let i = self.imp(canonical); self.u8(0x08).u16(i).u8(argc) }
    fn while_begin(self) -> Self { self.u8(0x34) }
    fn while_test(self, rel: i32) -> Self { self.u8(0x35).i32(rel) }
    fn loop_back(self, rel: i32) -> Self { self.u8(0x36).i32(rel) }
    fn jump(self, rel: i32) -> Self { self.u8(0x0c).i32(rel) }
    fn halt(self) -> Self { self.u8(0x11) }
    fn file_v(&self, schema: &[u8], version: u16, magic: &[u8]) -> Vec<u8> {
        let mut out = magic.to_vec(); out.extend_from_slice(&version.to_be_bytes()); out.extend_from_slice(&sha256(schema));
        out.extend_from_slice(&(self.consts.len() as u32).to_be_bytes());
        for c in &self.consts { out.push(1); out.extend_from_slice(&(c.len() as u32).to_be_bytes()); out.extend_from_slice(c); }
        out.extend_from_slice(&(self.imports.len() as u16).to_be_bytes()); for i in &self.imports { out.extend_from_slice(&i.to_be_bytes()); }
        out.extend_from_slice(&(self.contexts.len() as u16).to_be_bytes()); for (n, t) in &self.contexts { out.extend_from_slice(&n.to_be_bytes()); out.extend_from_slice(&t.to_be_bytes()); }
        out.extend_from_slice(&(self.locals.len() as u16).to_be_bytes()); for (t, h) in &self.locals { out.extend_from_slice(&t.to_be_bytes()); out.push(*h); }
        out.extend_from_slice(&self.max_stack.to_be_bytes());
        out.extend_from_slice(&(self.code.len() as u32).to_be_bytes()); out.extend_from_slice(&self.code);
        out.extend_from_slice(&(self.lines.len() as u32).to_be_bytes()); for (pc, l, c) in &self.lines { out.extend_from_slice(&pc.to_be_bytes()); out.extend_from_slice(&l.to_be_bytes()); out.extend_from_slice(&c.to_be_bytes()); }
        out.extend_from_slice(&0u16.to_be_bytes()); // v6: no rule entries
        out
    }
    fn file(&self) -> Vec<u8> { self.file_v(SCHEMA, VERSION, b"DSLB") }
}
struct Thing { id: i64 }
struct Trace(Vec<String>);
fn registry() -> Registry {
    let mut reg = Registry::without_std();
    reg.context("t", "Trace").unwrap().context("root", "Thing").unwrap();
    reg.method("Trace", "int", &["int"], "void", |ctx, this, a| { let v = a[0].as_int()?; ctx.get_mut::<Trace>(this)?.0.push(format!("int {}", v)); Ok(Value::None) }).unwrap();
    reg.method("Trace", "float", &["float"], "void", |ctx, this, a| { let v = a[0].as_float()?; ctx.get_mut::<Trace>(this)?.0.push(format!("float {}", format_g15(v))); Ok(Value::None) }).unwrap();
    reg.method("Trace", "float_bits", &["float"], "void", |ctx, this, a| { let v = a[0].as_float()?; ctx.get_mut::<Trace>(this)?.0.push(format!("bits {:016x}", v.to_bits())); Ok(Value::None) }).unwrap();
    reg.method("Trace", "str", &["string"], "void", |ctx, this, a| { let v = a[0].as_str()?.to_string(); ctx.get_mut::<Trace>(this)?.0.push(format!("str {}", v)); Ok(Value::None) }).unwrap();
    reg.method("Thing", "id", &[], "int", |ctx, this, _| Ok(Value::Int(ctx.get::<Thing>(this)?.id))).unwrap();
    reg.function("twice", &["int"], "int", |_, a| Ok(Value::Int(a[0].as_int()?.wrapping_mul(2)))).unwrap();
    reg.function("boom", &[], "void", |_, _| Err(Error::host("boom from the host"))).unwrap();
    reg
}
fn run(bytes: &[u8]) -> Result<Vec<String>, Error> {
    let reg = registry(); let p = Program::load(bytes, SCHEMA, &reg)?;
    let mut ctx = Context::new(); let t = ctx.set_object("t", "Trace", Trace(Vec::new())); ctx.set_object("root", "Thing", Thing { id: 0 });
    p.run(&mut ctx, &reg)?; Ok(ctx.take::<Trace>(t)?.0)
}
fn load_err(bytes: &[u8]) -> String { Program::load(bytes, SCHEMA, &registry()).expect_err("load must fail").message }
fn run_err(bytes: &[u8]) -> String { run(bytes).expect_err("run must fail").message }
fn t() -> Asm { Asm::default().ctx("t", "Trace") }

fn range_file(start: i64, end: i64, locals: usize, body: &[u8]) -> Vec<u8> {
    let mut a = t();
    for _ in 0..locals { a = a.local("int", 1); }
    a = a.push_int(start).push_int(end).u8(0x2f).u16((locals - 2) as u16).u16((locals - 1) as u16).i32(0);
    let body_start = a.code.len();
    a.code.extend_from_slice(body);
    let next = a.code.len();
    a.code.extend_from_slice(&[0x31, 0, (locals - 1) as u8, 0, 0, 0, 0]);
    let after_next = a.code.len();
    a.code.push(0x11);
    a.code[body_start - 4..body_start].copy_from_slice(&((after_next - body_start) as i32).to_be_bytes());
    a.code[next + 3..next + 7].copy_from_slice(&((body_start as i32) - (after_next as i32)).to_be_bytes());
    a.file()
}

#[test] fn arithmetic_concat_methods() { let a = t().push_int(1).push_int(2).u8(0x13).method(T_INT, 1).ctx("t", "Trace").push_str("a").push_str("b").u8(0x26).method(T_STR, 1).halt(); assert_eq!(run(&a.file()).unwrap(), ["int 3", "str ab"]); }
#[test] fn host_function_and_locals() { let a = Asm::default().local("int", 0).push_int(21).func(TWICE, 1).u8(0x05).u16(0).ctx("t", "Trace").u8(0x04).u16(0).method(T_INT, 1).halt(); assert_eq!(run(&a.file()).unwrap(), ["int 42"]); }
#[test] fn forward_jumps_select_branch() { let a = Asm::default().push_bool(true).u8(0x0D).i32(21).ctx("t", "Trace").push_int(1).method(T_INT, 1).u8(0x0C).i32(16).ctx("t", "Trace").push_int(2).method(T_INT, 1).halt(); assert_eq!(run(&a.file()).unwrap(), ["int 1"]); }
#[test] fn context_floats_conversions_equality() { let a = t().ctx("root", "Thing").method(THING_ID, 0).method(T_INT, 1).ctx("t", "Trace").push_float(2.5).u8(0x28).method(T_STR, 1).ctx("t", "Trace").push_int(7).push_int(2).u8(0x16).u8(0x25).push_float(0.5).u8(0x17).u8(0x28).method(T_STR, 1).ctx("t", "Trace").push_int(3).push_int(3).u8(0x09).u8(0x29).method(T_STR, 1).halt(); assert_eq!(run(&a.file()).unwrap(), ["int 0", "str 2.5", "str 3.5", "str true"]); }
#[test] fn math_opcode_covers_float_integer_and_edge_semantics() {
    let a = t()
        .push_float(std::f64::consts::FRAC_PI_2).u8(0x32).u8(0).method(T_FLOAT, 1)
        .ctx("t", "Trace").push_float(2.0).u8(0x32).u8(2).method(T_FLOAT, 1)
        .ctx("t", "Trace").push_float(-2.75).u8(0x32).u8(4).method(T_FLOAT, 1)
        .ctx("t", "Trace").push_float(2.0).push_float(3.0).u8(0x32).u8(5).method(T_FLOAT, 1)
        .ctx("t", "Trace").push_float(7.0).push_float(3.0).u8(0x32).u8(6).method(T_FLOAT, 1)
        .ctx("t", "Trace").push_float(7.0).push_float(3.0).u8(0x32).u8(7).method(T_FLOAT, 1)
        .ctx("t", "Trace").push_float(8.0).push_float(0.0).push_float(3.0).u8(0x32).u8(8).method(T_FLOAT, 1)
        .ctx("t", "Trace").push_float(2.0).push_float(10.0).push_float(0.25).u8(0x32).u8(9).method(T_FLOAT, 1)
        .ctx("t", "Trace").push_float(-1.0).u8(0x32).u8(10).method(T_FLOAT, 1)
        .ctx("t", "Trace").push_float(2.9).u8(0x32).u8(13).method(T_INT, 1)
        .ctx("t", "Trace").push_int(i64::MIN).u8(0x32).u8(14).method(T_INT, 1)
        .ctx("t", "Trace").push_int(3).push_int(8).u8(0x32).u8(15).method(T_INT, 1)
        .ctx("t", "Trace").push_int(3).push_int(8).u8(0x32).u8(16).method(T_INT, 1)
        .ctx("t", "Trace").push_int(8).push_int(0).push_int(3).u8(0x32).u8(17).method(T_INT, 1)
        .halt();
    assert_eq!(run(&a.file()).unwrap(), [
        "float 1", "float 1.4142135623731", "float -3", "float 8", "float 3", "float 7", "float 3", "float 4", "float 0",
        "int 2", "int -9223372036854775808", "int 3", "int 8", "int 3"
    ]);
}
#[test] fn math_opcode_rejects_unknown_function_at_load() {
    let a = t().push_float(1.0).u8(0x32).u8(99).halt();
    assert!(load_err(&a.file()).contains("MATH at offset 12 has unknown function 99"));
}
#[test] fn math_nan_and_signed_zero_contract() {
    let a = t()
        .push_float(f64::NAN).push_float(1.0).u8(0x32).u8(6).method(T_FLOAT, 1)
        .ctx("t", "Trace").push_float(-0.0).push_float(0.0).u8(0x32).u8(6).method(T_FLOAT, 1)
        .ctx("t", "Trace").push_float(0.0).push_float(-0.0).u8(0x32).u8(7).method(T_FLOAT, 1)
        .ctx("t", "Trace").push_float(f64::NAN).u8(0x32).u8(10).method(T_FLOAT, 1)
        .halt();
    assert_eq!(run(&a.file()).unwrap(), ["float nan", "float -0", "float 0", "float nan"]);
}
#[test] fn math_native_results_are_captured_without_decimal_formatting() {
    let a = t()
        .push_float(2.0).u8(0x32).u8(2).method(T_FLOAT_BITS, 1)
        .ctx("t", "Trace").push_float(2.0).push_float(0.5).u8(0x32).u8(5).method(T_FLOAT_BITS, 1)
        .ctx("t", "Trace").push_float(1.0).push_float(3.0).push_float(0.25).u8(0x32).u8(9).method(T_FLOAT_BITS, 1)
        .ctx("t", "Trace").push_float(-0.0).push_float(0.0).u8(0x32).u8(6).method(T_FLOAT_BITS, 1)
        .halt();
    let expected = [
        format!("bits {:016x}", 2.0_f64.sqrt().to_bits()),
        format!("bits {:016x}", 2.0_f64.powf(0.5).to_bits()),
        format!("bits {:016x}", (1.0_f64 + (3.0 - 1.0) * 0.25).to_bits()),
        format!("bits {:016x}", (-0.0_f64).to_bits()),
    ];
    assert_eq!(run(&a.file()).unwrap(), expected);
}
#[test] fn empty_program_loads() { let p = Program::load(&Asm::default().halt().file(), SCHEMA, &registry()).unwrap(); assert_eq!((p.constant_count(), p.code_size(), p.import_count(), p.locals().len(), p.lines().len()), (0, 1, 0, 0, 0)); }
#[test] fn int64_literals_and_wraparound() {
    let a = t().push_int(i64::MAX).method(T_INT, 1).ctx("t", "Trace").push_int(i64::MIN).method(T_INT, 1).ctx("t", "Trace").push_int(i64::MAX).push_int(1).u8(0x13).method(T_INT, 1).ctx("t", "Trace").push_int(i64::MIN).u8(0x1B).method(T_INT, 1).ctx("t", "Trace").push_int(i64::MIN).push_int(-1).u8(0x16).method(T_INT, 1).ctx("t", "Trace").push_int(i64::MIN).push_int(-1).u8(0x33).method(T_INT, 1).ctx("t", "Trace").push_int(i64::MAX).push_int(i64::MAX).u8(0x15).method(T_INT, 1).halt();
    assert_eq!(run(&a.file()).unwrap(), ["int 9223372036854775807", "int -9223372036854775808", "int -9223372036854775808", "int -9223372036854775808", "int -9223372036854775808", "int 0", "int 1"]);
}
#[test] fn mod_takes_the_dividends_sign() { let a = t().push_int(7).push_int(3).u8(0x33).method(T_INT, 1).ctx("t", "Trace").push_int(-7).push_int(3).u8(0x33).method(T_INT, 1).ctx("t", "Trace").push_int(7).push_int(-3).u8(0x33).method(T_INT, 1).ctx("t", "Trace").push_int(-7).push_int(2).u8(0x16).method(T_INT, 1).halt(); assert_eq!(run(&a.file()).unwrap(), ["int 1", "int -1", "int 1", "int -3"]); }
#[test] fn sections_are_read_back() {
    let mut a = Asm::default().local("int", 0).local("Thing", 1).line(0, 3, 5).line(9, 4, 1).push_int(1).u8(0x05).u16(0).halt(); a.max_stack = 7;
    let p = Program::load(&a.file(), SCHEMA, &registry()).unwrap();
    assert_eq!(p.max_stack(), 7); assert_eq!(p.locals(), &[Local { type_name: "int".into(), hidden: false }, Local { type_name: "Thing".into(), hidden: true }]);
    assert_eq!(p.lines().len(), 2); assert_eq!(p.line_of(0).unwrap().line, 3); assert_eq!(p.line_of(8).unwrap().line, 3); assert_eq!(p.line_of(9).unwrap().line, 4); assert_eq!(p.line_of(12).unwrap().column, 1);
    assert_eq!(Program::load(&Asm::default().halt().file(), SCHEMA, &registry()).unwrap().line_of(0), None);
}
#[test] fn bad_magic() { assert!(load_err(&Asm::default().halt().file_v(SCHEMA, VERSION, b"XXXX")).contains("bad magic")); }
#[test] fn version_4_refused() { assert!(load_err(&Asm::default().halt().file_v(SCHEMA, 4, b"DSLB")).contains("unsupported bytecode version 4 (this runtime reads version 6)")); }
#[test] fn while_false_exits_and_break_is_structural() {
    // begin 0, test ends at 8, break jump ends at 13, loop-back ends at 18, HALT at 18.
    let a = Asm::default().while_begin().push_bool(true).while_test(10).jump(5).loop_back(-18).halt();
    assert!(run(&a.file()).is_ok());
}
#[test] fn while_continue_forward_jump_is_structural() {
    // A false condition skips the body; the body still contains a continue jump to LOOP_BACK.
    let a = Asm::default().while_begin().push_bool(false).while_test(10).jump(0).loop_back(-18).halt();
    assert!(run(&a.file()).is_ok());
}
#[test] fn while_test_must_consume_only_the_condition() {
    // A caller operand remains below the condition, so the test depth is head+2 instead of head+1.
    let a = Asm::default().while_begin().push_int(1).push_bool(false).while_test(6).u8(0x10).loop_back(-23).halt();
    let error = load_err(&a.file());
    assert!(error.contains("WHILE_TEST must leave the loop-head stack depth"), "got {error}");
}
#[test] fn jump_may_not_skip_a_paired_while_test() {
    // begin 0, jump ends 6 and lands at the body after WHILE_TEST at 8..12.
    let a = Asm::default().while_begin().jump(7).push_bool(false).while_test(5).loop_back(-18).halt();
    let error = load_err(&a.file());
    assert!(error.contains("skips its WHILE_TEST"), "got {error}");
}
#[test] fn schema_mismatch() { assert!(load_err(&Asm::default().halt().file_v(b"other", VERSION, b"DSLB")).contains("schema mismatch")); }
#[test] fn truncated() { let f = t().halt().file(); assert!(load_err(&f[..f.len() - 3]).contains("bytecode is truncated")); }
#[test] fn trailing_bytes() { let mut f = Asm::default().halt().file(); f.push(0); assert!(load_err(&f).contains("trailing byte(s) after the entries")); }
#[test] fn unknown_constant_tag() { let mut f = Asm::default().push_str("x").halt().file(); f[4 + 2 + 32 + 4] = 7; assert!(load_err(&f).contains("constant 0 has unknown tag 7 (1 = string)")); }
#[test] fn constant_out_of_range() { assert!(load_err(&Asm::default().u8(0x02).u32(9).halt().file()).contains("constant index 9 out of range")); }
#[test] fn local_out_of_range() { assert!(load_err(&Asm::default().u8(0x04).u16(0).halt().file()).contains("local slot index 0 out of range")); }
#[test] fn context_index_out_of_range() { assert!(load_err(&Asm::default().u8(0x06).u16(0).halt().file()).contains("context index 0 out of range")); }
#[test] fn undeclared_context() { assert!(load_err(&Asm::default().ctx("nope", "Trace").halt().file()).contains("context variable 'nope' is not declared")); }
#[test] fn context_type_differs() { assert!(load_err(&Asm::default().ctx("t", "Thing").halt().file()).contains("context variable 't' is declared as Thing by the bytecode, the host provides Trace")); }
#[test] fn unbound_function_import() { assert!(load_err(&Asm::default().func("fn missing() -> int", 0).halt().file()).contains("import 'fn missing() -> int' is not provided by the host")); }
#[test] fn import_with_other_types_names_the_hosts() { assert!(load_err(&Asm::default().push_float(1.0).func("fn twice(float) -> int", 1).halt().file()).contains("import 'fn twice(float) -> int' is not provided by the host (the host declares fn twice(int) -> int)")); }
#[test] fn unbound_method_import() { assert!(load_err(&Asm::default().ctx("root", "Thing").method("method Thing.name() -> string", 0).halt().file()).contains("import 'method Thing.name() -> string' is not provided by the host")); }
#[test] fn method_import_with_other_types() { assert!(load_err(&Asm::default().ctx("root", "Thing").method("method Thing.id(int) -> int", 1).halt().file()).contains("(the host declares method Thing.id() -> int)")); }
#[test] fn call_arity_differs_from_import() { assert!(load_err(&Asm::default().push_int(1).push_int(2).func(TWICE, 2).halt().file()).contains("passes 2 argument(s), the import takes 1")); }
#[test] fn call_fn_on_method_import() { assert!(load_err(&Asm::default().ctx("root", "Thing").func(THING_ID, 0).halt().file()).contains("which is a method")); }
#[test] fn call_method_on_function_import() { assert!(load_err(&Asm::default().push_int(1).method(TWICE, 1).halt().file()).contains("which is a function")); }
#[test] fn import_index_out_of_range() { assert!(load_err(&Asm::default().u8(0x07).u16(4).u8(0).halt().file()).contains("import index 4 out of range")); }
#[test] fn jump_into_operand() { assert!(load_err(&Asm::default().u8(0x0C).i32(1).halt().file()).contains("not an instruction start")); }
#[test] fn backward_jump() { assert!(load_err(&Asm::default().u8(0x0C).i32(-5).halt().file()).contains("goes backwards")); }
#[test] fn missing_halt() { assert!(load_err(&Asm::default().push_int(1).file()).contains("does not end with HALT")); }
#[test] fn unknown_opcode() { assert!(load_err(&Asm::default().u8(0x7f).halt().file()).contains("unknown opcode 127")); }
#[test] fn bad_bool_operand() { assert!(load_err(&Asm::default().u8(0x03).u8(2).halt().file()).contains("PUSH_BOOL operand must be 0 or 1")); }
#[test] fn bad_hidden_flag() { assert!(load_err(&Asm::default().local("int", 2).halt().file()).contains("hidden flag must be 0 or 1")); }
#[test] fn unsorted_line_table() { assert!(load_err(&Asm::default().line(5, 1, 1).line(2, 2, 1).push_int(1).u8(0x10).halt().file()).contains("line table is not sorted")); }
#[test] fn line_entry_beyond_code() { assert!(load_err(&Asm::default().line(99, 1, 1).halt().file()).contains("beyond the code")); }
#[test]
fn byte_strings_load_concat_compare_and_reach_hosts_unchanged() {
    let mut reg = registry();
    reg.function("echo", &["string"], "string", |_, args| {
        assert_eq!(args[0].as_bytes()?, b"\xff\0\xc3\xa9");
        Ok(Value::string(args[0].as_bytes()?))
    }).unwrap();
    reg.function("capture", &["string"], "void", |ctx, args| {
        ctx.get_mut::<Vec<Vec<u8>>>(ctx.lookup("bytes").unwrap())?.push(args[0].as_bytes()?.to_vec());
        Ok(Value::None)
    }).unwrap();
    let a = Asm::default().push_bytes(b"\xff\0").push_str("é").u8(0x26)
        .func("fn echo(string) -> string", 1).func("fn capture(string) -> void", 1)
        .ctx("t", "Trace").push_bytes(b"\xff\0").push_bytes(b"\xff\0").u8(0x09).u8(0x29).method(T_STR, 1)
        .ctx("t", "Trace").push_bytes(b"\xff\0").push_bytes(b"\xfe\0").u8(0x0a).u8(0x29).method(T_STR, 1).halt();
    let program = Program::load(&a.file(), SCHEMA, &reg).unwrap();
    let mut ctx = Context::new();
    let bytes = ctx.set_object("bytes", "Bytes", Vec::<Vec<u8>>::new());
    let trace = ctx.set_object("t", "Trace", Trace(Vec::new()));
    program.run(&mut ctx, &reg).unwrap();
    assert_eq!(ctx.take::<Vec<Vec<u8>>>(bytes).unwrap(), vec![b"\xff\0\xc3\xa9".to_vec()]);
    assert_eq!(ctx.take::<Trace>(trace).unwrap().0, ["str true", "str true"]);
}

#[test]
fn host_byte_string_helpers_validate_or_convert_lossily() {
    let value = Value::string(b"a\xff\0b");
    assert_eq!(value.as_bytes().unwrap(), b"a\xff\0b");
    assert_eq!(value.as_str().unwrap_err().code, "RUN_HOST");
    assert_eq!(value.to_string_lossy().unwrap(), "a\u{fffd}\0b");
    let text = Value::string("é\0");
    assert_eq!(text.as_str().unwrap(), "é\0");
    // Byte offsets can split a UTF-8 codepoint; no decoding or replacement occurs.
    assert_eq!(&text.as_string().unwrap()[1..], b"\xa9\0");
    assert_eq!(Value::string(&text.as_bytes().unwrap()[1..]).as_bytes().unwrap(), b"\xa9\0");
    assert!(Value::Int(1).as_bytes().is_err());
}

#[test] fn int_division_by_zero() { assert!(run_err(&Asm::default().push_int(1).push_int(0).u8(0x16).halt().file()).contains("int division by zero")); }
#[test] fn int_modulo_by_zero() { assert!(run_err(&Asm::default().push_int(1).push_int(0).u8(0x33).halt().file()).contains("int modulo by zero")); }
#[test] fn mixed_kind_equality() { assert!(run_err(&Asm::default().push_int(1).push_str("x").u8(0x09).halt().file()).contains("cannot compare int with string")); }
#[test] fn float_op_on_int() { assert!(run_err(&Asm::default().push_int(1).push_float(1.0).u8(0x17).halt().file()).contains("float operator applied to int and float")); }
#[test] fn ordinary_vm_type_error_has_run_error_code() {
    let error = run(&Asm::default().push_int(1).push_float(1.0).u8(0x17).halt().file()).unwrap_err();
    assert_eq!(error.code, "RUN_ERROR");
    assert_eq!(error.message, "float operator applied to int and float");
}
#[test] fn local_read_before_store() { assert!(run_err(&Asm::default().local("int", 0).u8(0x04).u16(0).halt().file()).contains("read before assignment")); }
#[test] fn non_bool_condition() { assert!(run_err(&Asm::default().push_int(1).u8(0x0D).i32(0).halt().file()).contains("condition is a int, not a bool")); }
#[test] fn stack_underflow() { assert!(run_err(&Asm::default().u8(0x10).halt().file()).contains("stack underflow")); }
#[test] fn host_error_propagates() { assert!(run_err(&Asm::default().func(BOOM, 0).halt().file()).contains("boom from the host")); }
#[test] fn host_callback_error_has_run_host_code() {
    let error = run(&Asm::default().func(BOOM, 0).halt().file()).unwrap_err();
    assert_eq!(error.code, "RUN_HOST");
    assert_eq!(error.message, "boom from the host");
}
#[test] fn context_object_missing_at_run() { let reg = registry(); let p = Program::load(&t().halt().file(), SCHEMA, &reg).unwrap(); let mut empty = Context::new(); assert!(p.run(&mut empty, &reg).unwrap_err().message.contains("context variable 't' was not provided by the host for this run")); }
#[test] fn method_on_wrong_object_type() { assert!(run_err(&t().method(THING_ID, 0).halt().file()).contains("method Thing.id called on a Trace")); }
#[test] fn no_stack_cap() { let mut a = Asm { max_stack: 1, ..Asm::default() }; for _ in 0..5000 { a = a.push_int(1); } for _ in 0..4999 { a = a.u8(0x13); } let a = a.ctx("t", "Trace").u8(0x0F).u8(0x10).halt(); assert!(run(&a.file()).is_ok()); }
#[test] fn no_string_cap() { let mut a = t().push_str(&"x".repeat(65536)); for _ in 0..9 { a = a.u8(0x0F).u8(0x26); } let got = run(&a.method(T_STR, 1).halt().file()).unwrap(); assert_eq!(got[0].len(), 4 + 65536 * 512); }
#[test] fn range_loop_executes_snapshot_and_empty() {
    // PUSH start/end, FOR_RANGE(counter=0,end=1,exit=44), body prints i, FOR_NEXT(end=1,back=-17), HALT.
    let a = t().local("int", 1).local("int", 1)
        .push_int(0).push_int(3).u8(0x2f).u16(0).u16(1).i32(17)
        .ctx("t", "Trace").u8(0x04).u16(0).method(T_INT, 1)
        .u8(0x31).u16(1).i32(-17).halt();
    assert_eq!(run(&a.file()).unwrap(), ["int 0", "int 1", "int 2"]);

    let empty = t().local("int", 1).local("int", 1)
        .push_int(4).push_int(4).u8(0x2f).u16(0).u16(1).i32(17)
        .ctx("t", "Trace").u8(0x04).u16(0).method(T_INT, 1)
        .u8(0x31).u16(1).i32(-17).halt();
    assert!(run(&empty.file()).unwrap().is_empty());
}
#[test] fn range_loop_rejects_bad_structure() {
    let base = t().local("int", 1).local("int", 1)
        .push_int(0).push_int(1).u8(0x2f).u16(0).u16(1).i32(17)
        .ctx("t", "Trace").u8(0x04).u16(0).method(T_INT, 1)
        .u8(0x31).u16(1).i32(-17).halt();
    let mut bad = base.file();
    // FOR_NEXT's encoded end slot no longer matches its FOR_RANGE.
    let next_end = bad.windows(2).rposition(|w| w == [0x00, 0x01]).unwrap();
    bad[next_end + 1] = 0;
    assert!(load_err(&bad).contains("closes the wrong range loop"));

    // The exit is required to be the instruction immediately following the paired FOR_NEXT.
    let wrong_exit = t().local("int", 1).local("int", 1)
        .push_int(0).push_int(1).u8(0x2f).u16(0).u16(1).i32(10)
        .ctx("t", "Trace").u8(0x04).u16(0).method(T_INT, 1)
        .u8(0x31).u16(1).i32(-17).halt();
    assert!(load_err(&wrong_exit.file()).contains("must exit immediately after its FOR_NEXT"));
}
#[test] fn range_loop_snapshot_and_i64_edges() {
    // The bound is copied into its hidden local at entry; body execution cannot alter the trip bound.
    let body = [0x04, 0, 0, 0x10];
    assert!(run(&range_file(0, 3, 2, &body)).unwrap().is_empty());
    assert!(run(&range_file(i64::MAX - 2, i64::MAX, 2, &[])).is_ok());
    assert!(run(&range_file(i64::MIN, i64::MIN + 2, 2, &[])).is_ok());
}
#[test] fn range_loop_million_iterations_without_per_iteration_trace() {
    // The body only loads/pops the read-only counter; one trace call after the loop observes the final value.
    let body = [0x04, 0, 0, 0x10];
    let bytes = range_file(0, 1_000_000, 2, &body);
    let reg = registry();
    let mut ctx = Context::new();
    let t = ctx.set_object("t", "Trace", Trace(Vec::new()));
    ctx.set_object("root", "Thing", Thing { id: 0 });
    let p = Program::load(&bytes, SCHEMA, &reg).unwrap();
    p.run(&mut ctx, &reg).unwrap();
    assert!(ctx.take::<Trace>(t).unwrap().0.is_empty());
}
#[test] fn every_truncation_is_refused() {
    let f = t().push_int(1).push_int(2).u8(0x13).method(T_INT, 1).push_bool(true).u8(0x0D).i32(0).halt().file();
    for n in 0..f.len() { assert!(Program::load(&f[..n], SCHEMA, &registry()).is_err(), "truncation at {} must fail", n); }
}
#[test] fn context_arena_take_and_type_checks() {
    let mut ctx = Context::new(); let t = ctx.insert("Thing", Thing { id: 7 });
    assert!(ctx.get::<Trace>(t).is_err()); assert_eq!(ctx.get::<Thing>(t).unwrap().id, 7); assert_eq!(ctx.take::<Thing>(t).unwrap().id, 7); assert!(ctx.take::<Thing>(t).is_err());
}
#[test] fn schema_summary_groups_methods_by_owner() {
    assert_eq!(registry().schema_summary(), "context root: Thing\ncontext t: Trace\nfn boom() -> void\nfn twice(int) -> int\nThing:\n  Thing.id() -> int\nTrace:\n  Trace.float(float) -> void\n  Trace.float_bits(float) -> void\n  Trace.int(int) -> void\n  Trace.str(string) -> void\n");
    assert_eq!(Registry::without_std().schema_summary(), "");
}
#[test] fn signatures_are_sorted_canonical_strings() { assert_eq!(registry().signatures(), ["context root: Thing", "context t: Trace", "fn boom() -> void", "fn twice(int) -> int", "method Thing.id() -> int", "method Trace.float(float) -> void", "method Trace.float_bits(float) -> void", "method Trace.int(int) -> void", "method Trace.str(string) -> void"]); }
#[test] fn duplicate_registration_is_refused() { let mut reg = Registry::without_std(); reg.function("f", &[], "void", |_, _| Ok(Value::None)).unwrap(); let e = reg.function("f", &[], "void", |_, _| Ok(Value::None)).err().unwrap(); assert_eq!(e.code, "LOAD_UNBOUND"); assert!(e.message.contains("registered twice")); }
#[test] fn overloads_are_keyed_by_parameter_types() {
    let mut reg = Registry::without_std();
    reg.function("f", &["int"], "int", |_, _| Ok(Value::Int(1))).unwrap();
    reg.function("f", &["float"], "float", |_, _| Ok(Value::Float(1.0))).unwrap();
    reg.function("f", &["int", "int"], "int", |_, _| Ok(Value::Int(2))).unwrap();
    assert_eq!(reg.signatures(), ["fn f(float) -> float", "fn f(int) -> int", "fn f(int, int) -> int"]);
    let e = reg.function("f", &["int"], "float", |_, _| Ok(Value::Float(1.0))).err().unwrap();
    assert_eq!(e.code, "LOAD_UNBOUND");
    assert!(e.message.contains("registered twice"));
}
#[test] fn canonical_overloads_dispatch_selected_function_and_method() {
    use std::cell::Cell;
    use std::rc::Rc;
    let picked = Rc::new(Cell::new(0));
    let mut reg = Registry::without_std();
    for (params, value) in [(&["int"][..], 1), (&["float"][..], 2), (&["int", "int"][..], 3)] {
        let p = Rc::clone(&picked);
        reg.function("pick", params, "void", move |_, _| { p.set(value); Ok(Value::None) }).unwrap();
    }
    reg.context("thing", "Thing").unwrap();
    for (params, value) in [(&["int"][..], 4), (&["float"][..], 5), (&["int", "int"][..], 6)] {
        let p = Rc::clone(&picked);
        reg.method("Thing", "pick", params, "void", move |_, _, _| { p.set(value); Ok(Value::None) }).unwrap();
    }
    let bytes = Asm::default().push_int(1).func("fn pick(int) -> void", 1)
        .push_float(1.5).func("fn pick(float) -> void", 1)
        .push_int(2).push_int(3).func("fn pick(int, int) -> void", 2)
        .ctx("thing", "Thing").push_int(1).method("method Thing.pick(int) -> void", 1)
        .ctx("thing", "Thing").push_float(1.5).method("method Thing.pick(float) -> void", 1)
        .ctx("thing", "Thing").push_int(2).push_int(3).method("method Thing.pick(int, int) -> void", 2).halt().file();
    let p = Program::load(&bytes, SCHEMA, &reg).unwrap();
    let mut ctx = Context::new(); ctx.set_object("thing", "Thing", Thing { id: 0 });
    p.run(&mut ctx, &reg).unwrap();
    assert_eq!(picked.get(), 6);
}
#[test] fn error_codes_classify_the_existing_errors() {
    assert_eq!(Program::load(&Asm::default().halt().file_v(SCHEMA, VERSION, b"XXXX"), SCHEMA, &registry()).unwrap_err().code, "LOAD_BAD_BYTECODE");
    assert_eq!(Program::load(&Asm::default().halt().file_v(b"other", VERSION, b"DSLB"), SCHEMA, &registry()).unwrap_err().code, "LOAD_SCHEMA_MISMATCH");
    assert_eq!(Program::load(&Asm::default().func("fn missing() -> void", 0).halt().file(), SCHEMA, &registry()).unwrap_err().code, "LOAD_UNBOUND");
    assert_eq!(Program::load(&Asm::default().ctx("t", "Thing").halt().file(), SCHEMA, &registry()).unwrap_err().code, "LOAD_UNBOUND");
    assert_eq!(run(&Asm::default().func(BOOM, 0).halt().file()).unwrap_err().code, "RUN_HOST");
    assert_eq!(run(&Asm::default().push_int(1).push_int(0).u8(0x16).halt().file()).unwrap_err().code, "RUN_ERROR");
}
#[test] fn result_line_exit_codes() { assert_eq!(report_error("load", &Error::new("LOAD_UNBOUND", "x")), 3); assert_eq!(report_error("run", &Error::host("no")), 3); assert_eq!(report_app_failure("nothing"), 4); assert_eq!(report_ok("42"), 0); assert_eq!(json_escape("a\"b\n"), "\"a\\\"b\\n\""); }
#[test] fn error_json_has_exact_optional_location_fields() {
    let mut full = Error::new("RUN_ERROR", "bad"); full.locate(4, 2);
    let mut line = Error::host("host"); line.locate(5, 0);
    let app = Error::new("APP_FAIL", "app");
    assert_eq!(error_json("run", &full), "{\"ok\": false, \"phase\": \"run\", \"code\": \"RUN_ERROR\", \"message\": \"dsl line 4:2: bad\", \"line\": 4, \"column\": 2}");
    assert_eq!(error_json("run", &line), "{\"ok\": false, \"phase\": \"run\", \"code\": \"RUN_HOST\", \"message\": \"dsl line 5: host\", \"line\": 5}");
    assert_eq!(error_json("app", &app), "{\"ok\": false, \"phase\": \"app\", \"code\": \"APP_FAIL\", \"message\": \"app\"}");
    full.locate(9, 9); assert_eq!((full.line, full.column), (Some(4), Some(2)));
}
#[test] fn runtime_errors_use_faulting_opcode_locations_and_empty_tables_stay_unlocated() {
    let error = run(&Asm::default().line(0, 1, 1).push_int(1).push_int(0).line(18, 9, 7).u8(0x16).line(19, 99, 1).halt().file()).unwrap_err();
    assert_eq!(error.code, "RUN_ERROR"); assert_eq!((error.line, error.column), (Some(9), Some(7))); assert!(error.message.starts_with("dsl line 9:7: int division by zero"));
    let error = run(&Asm::default().line(0, 4, 2).func(BOOM, 0).halt().file()).unwrap_err();
    assert_eq!(error.code, "RUN_HOST"); assert_eq!((error.line, error.column), (Some(4), Some(2)));
    let error = run(&Asm::default().push_int(1).push_int(0).u8(0x16).halt().file()).unwrap_err();
    assert_eq!((error.line, error.column), (None, None));
    let mut error = Error::new("RUN_ERROR", "x"); error.locate(6, 0); assert_eq!((error.line, error.column, error.message), (Some(6), None, "dsl line 6: x".into()));
    let mut empty = Error::new("RUN_ERROR", "x"); empty.locate(0, 0); assert_eq!((empty.line, empty.column, empty.message), (None, None, "x".into()));
}
