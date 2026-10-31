use dslvm::stdlib::{InvocationResults, ResultContract};
use dslvm::{sha256, Context, Program, Registry, Value};
use std::cell::Cell;
use std::rc::Rc;

const SCHEMA: &[u8] = br#"{"dsl.schema":2,"name":"dry-callbacks"}"#;

#[derive(Default)]
struct Asm {
    constants: Vec<Vec<u8>>,
    imports: Vec<u32>,
    locals: Vec<u32>,
    code: Vec<u8>,
    lines: Vec<(u32, u32, u16)>,
}
impl Asm {
    fn constant(&mut self, value: &str) -> u32 {
        if let Some(index) = self
            .constants
            .iter()
            .position(|item| item == value.as_bytes())
        {
            return index as u32;
        }
        self.constants.push(value.as_bytes().to_vec());
        (self.constants.len() - 1) as u32
    }
    fn import(&mut self, signature: &str) -> u16 {
        let constant = self.constant(signature);
        if let Some(index) = self.imports.iter().position(|item| *item == constant) {
            return index as u16;
        }
        self.imports.push(constant);
        (self.imports.len() - 1) as u16
    }
    fn local(&mut self, type_name: &str) -> u16 {
        let type_constant = self.constant(type_name);
        self.locals.push(type_constant);
        (self.locals.len() - 1) as u16
    }
    fn op_u16(&mut self, op: u8, value: u16) {
        self.code.push(op);
        self.code.extend_from_slice(&value.to_be_bytes());
    }
    fn function(&mut self, signature: &str, argc: u8) {
        let index = self.import(signature);
        self.op_u16(0x07, index);
        self.code.push(argc);
    }
    fn method(&mut self, signature: &str, argc: u8) {
        let index = self.import(signature);
        self.op_u16(0x08, index);
        self.code.push(argc);
    }
    fn push_string(&mut self, value: &str) {
        let constant = self.constant(value);
        self.code.push(0x02);
        self.code.extend_from_slice(&constant.to_be_bytes());
    }
    fn encode(self) -> Vec<u8> {
        let mut bytes = b"DSLB".to_vec();
        bytes.extend_from_slice(&dslvm::VERSION.to_be_bytes());
        bytes.extend_from_slice(&sha256(SCHEMA));
        bytes.extend_from_slice(&(self.constants.len() as u32).to_be_bytes());
        for constant in self.constants {
            bytes.push(1);
            bytes.extend_from_slice(&(constant.len() as u32).to_be_bytes());
            bytes.extend_from_slice(&constant);
        }
        bytes.extend_from_slice(&(self.imports.len() as u16).to_be_bytes());
        for import in self.imports {
            bytes.extend_from_slice(&import.to_be_bytes());
        }
        bytes.extend_from_slice(&0_u16.to_be_bytes()); // context declarations
        bytes.extend_from_slice(&(self.locals.len() as u16).to_be_bytes());
        for local in self.locals {
            bytes.extend_from_slice(&local.to_be_bytes());
            bytes.push(0);
        }
        bytes.extend_from_slice(&2_u16.to_be_bytes()); // maximum stack depth
        bytes.extend_from_slice(&(self.code.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&self.code);
        bytes.extend_from_slice(&(self.lines.len() as u32).to_be_bytes());
        for (pc, line, column) in self.lines {
            bytes.extend_from_slice(&pc.to_be_bytes());
            bytes.extend_from_slice(&line.to_be_bytes());
            bytes.extend_from_slice(&column.to_be_bytes());
        }
        bytes.extend_from_slice(&0_u16.to_be_bytes()); // rule entries
        bytes
    }
    fn mutable_dummy_program(mut self) -> Vec<u8> {
        let l0 = self.local("IntList");
        let l1 = self.local("IntList");
        self.function("fn list_hole() -> IntList", 0);
        self.op_u16(0x05, l0);
        self.op_u16(0x04, l0);
        self.code.push(0x01);
        self.code.extend_from_slice(&9_i64.to_be_bytes());
        self.method("method IntList.add(int) -> IntList", 1);
        self.code.push(0x10); // discard the fluent add result
        self.function("fn list_hole() -> IntList", 0);
        self.op_u16(0x05, l1);
        self.op_u16(0x04, l1);
        self.method("method IntList.len() -> int", 0);
        self.function("fn lmcode_return_set(int) -> void", 1);
        self.code.push(0x11);
        self.encode()
    }
    fn file_policy_program(mut self, input: &str, sentinel: &str) -> Vec<u8> {
        self.push_string(sentinel);
        self.push_string("written");
        self.function("fn file_write(string, string) -> void", 2);
        self.push_string(input);
        self.function("fn file_read(string) -> string", 1);
        self.function("fn lmcode_return_set(string) -> void", 1);
        self.code.push(0x11);
        self.encode()
    }
    fn method_preview_program(mut self) -> Vec<u8> {
        self.function("fn make_thing() -> Thing", 0);
        self.method("method Thing.score() -> int", 0);
        self.function("fn lmcode_return_set(int) -> void", 1);
        self.code.push(0x11);
        self.encode()
    }
    fn dry_null_receiver_program(mut self) -> Vec<u8> {
        self.function("fn thing_hole() -> Thing", 0);
        let method_pc = self.code.len() as u32;
        self.method("method Thing.score() -> int", 0);
        self.function("fn lmcode_return_set(int) -> void", 1);
        self.code.push(0x11);
        self.lines.push((method_pc, 12, 5));
        self.encode()
    }
}

struct DryThing(i64);

#[test]
fn dry_skips_real_callbacks_and_keeps_mutable_dummies_fresh_per_slot() {
    let real_calls = Rc::new(Cell::new(0));
    let mut registry = Registry::new();
    let counter = Rc::clone(&real_calls);
    registry
        .function("list_hole", &[], "IntList", move |ctx, _| {
            counter.set(counter.get() + 1);
            Ok(Value::object(
                "IntList",
                ctx.insert_collection("IntList", dslvm::stdlib::collections::IntList::default())?,
            ))
        })
        .unwrap();
    let program =
        Program::load(&Asm::default().mutable_dummy_program(), SCHEMA, &registry).unwrap();
    let files = vec!["part-0".to_string(), "part-1".to_string()];

    let mut dry = InvocationResults::from_command_with_dry(&files, true).unwrap();
    let mut dry_context = Context::new();
    for index in 0..2 {
        dry.select(index, &mut dry_context, ResultContract::default())
            .unwrap();
        program.run(&mut dry_context, &registry).unwrap();
        dry.complete(index).unwrap();
    }
    let mut dry_frame = Vec::new();
    assert_eq!(dry.write_frame(&mut dry_frame), 0);
    assert_eq!(String::from_utf8(dry_frame).unwrap(),
        "LMCODE_RETURN_BEGIN\n{\"ok\":true,\"result\":[0,0],\"parts\":2,\"dry\":true}\nLMCODE_RETURN_END\n");
    assert_eq!(
        real_calls.get(),
        0,
        "dry default dispatched the real callback"
    );

    let mut real = InvocationResults::from_command_with_dry(&files, false).unwrap();
    let mut real_context = Context::new();
    for index in 0..2 {
        real.select(index, &mut real_context, ResultContract::default())
            .unwrap();
        program.run(&mut real_context, &registry).unwrap();
        real.complete(index).unwrap();
    }
    let mut real_frame = Vec::new();
    assert_eq!(real.write_frame(&mut real_frame), 0);
    assert_eq!(
        String::from_utf8(real_frame).unwrap(),
        "LMCODE_RETURN_BEGIN\n{\"ok\":true,\"result\":[0,0],\"parts\":2}\nLMCODE_RETURN_END\n"
    );
    assert_eq!(
        real_calls.get(),
        4,
        "real mode did not dispatch each actual callback"
    );
}

#[test]
fn dry_file_reads_opt_in_while_file_writes_are_skipped() {
    let root = std::env::temp_dir().join(format!("lmcode-dry-file-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let input = root.join("input");
    let sentinel = root.join("sentinel");
    std::fs::write(&input, b"fixture").unwrap();
    let schema = SCHEMA;
    let registry = Registry::new();
    let program = Program::load(
        &Asm::default().file_policy_program(input.to_str().unwrap(), sentinel.to_str().unwrap()),
        schema,
        &registry,
    )
    .unwrap();

    let mut dry = InvocationResults::from_command_with_dry(&["dry".into()], true).unwrap();
    let mut dry_context = Context::new();
    dry.select(0, &mut dry_context, ResultContract::default())
        .unwrap();
    program.run(&mut dry_context, &registry).unwrap();
    dry.complete(0).unwrap();
    let mut frame = Vec::new();
    assert_eq!(dry.write_frame(&mut frame), 0);
    assert_eq!(String::from_utf8(frame).unwrap(),
        "LMCODE_RETURN_BEGIN\n{\"ok\":true,\"result\":\"fixture\",\"dry\":true}\nLMCODE_RETURN_END\n");
    assert!(
        !sentinel.exists(),
        "dry file_write called its real callback"
    );

    let mut real = InvocationResults::from_command_with_dry(&["real".into()], false).unwrap();
    let mut real_context = Context::new();
    real.select(0, &mut real_context, ResultContract::default())
        .unwrap();
    program.run(&mut real_context, &registry).unwrap();
    real.complete(0).unwrap();
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"written");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn function_and_method_previews_run_once_and_real_mode_keeps_real_callbacks() {
    let real_function_calls = Rc::new(Cell::new(0));
    let preview_function_calls = Rc::new(Cell::new(0));
    let real_method_calls = Rc::new(Cell::new(0));
    let preview_method_calls = Rc::new(Cell::new(0));
    let mut registry = Registry::new();
    let real_fn_count = Rc::clone(&real_function_calls);
    let preview_fn_count = Rc::clone(&preview_function_calls);
    registry
        .function_with_dry(
            "make_thing",
            &[],
            "Thing",
            move |ctx, _| {
                real_fn_count.set(real_fn_count.get() + 1);
                Ok(Value::object("Thing", ctx.insert("Thing", DryThing(5))))
            },
            dslvm::dry(
                move |ctx: &mut Context, _: &dslvm::Args| -> Result<Value, dslvm::Error> {
                    preview_fn_count.set(preview_fn_count.get() + 1);
                    Ok(Value::object("Thing", ctx.insert("Thing", DryThing(7))))
                },
            ),
        )
        .unwrap();
    let real_method_count = Rc::clone(&real_method_calls);
    let preview_method_count = Rc::clone(&preview_method_calls);
    registry
        .method_with_dry(
            "Thing",
            "score",
            &[],
            "int",
            move |ctx, this, _| {
                real_method_count.set(real_method_count.get() + 1);
                Ok(Value::Int(ctx.get::<DryThing>(this)?.0))
            },
            dslvm::dry(
                move |ctx: &mut Context,
                      this: dslvm::ObjRef,
                      _: &dslvm::Args|
                      -> Result<Value, dslvm::Error> {
                    preview_method_count.set(preview_method_count.get() + 1);
                    Ok(Value::Int(ctx.get::<DryThing>(this)?.0 * 10))
                },
            ),
        )
        .unwrap();
    let program =
        Program::load(&Asm::default().method_preview_program(), SCHEMA, &registry).unwrap();

    let mut dry = InvocationResults::from_command_with_dry(&["dry".into()], true).unwrap();
    let mut dry_context = Context::new();
    dry.select(0, &mut dry_context, ResultContract::default())
        .unwrap();
    program.run(&mut dry_context, &registry).unwrap();
    dry.complete(0).unwrap();
    let mut dry_frame = Vec::new();
    assert_eq!(dry.write_frame(&mut dry_frame), 0);
    assert_eq!(
        String::from_utf8(dry_frame).unwrap(),
        "LMCODE_RETURN_BEGIN\n{\"ok\":true,\"result\":70,\"dry\":true}\nLMCODE_RETURN_END\n"
    );
    assert_eq!(
        (real_function_calls.get(), preview_function_calls.get()),
        (0, 1)
    );
    assert_eq!(
        (real_method_calls.get(), preview_method_calls.get()),
        (0, 1)
    );

    let mut real = InvocationResults::from_command_with_dry(&["real".into()], false).unwrap();
    let mut real_context = Context::new();
    real.select(0, &mut real_context, ResultContract::default())
        .unwrap();
    program.run(&mut real_context, &registry).unwrap();
    real.complete(0).unwrap();
    let mut real_frame = Vec::new();
    assert_eq!(real.write_frame(&mut real_frame), 0);
    assert_eq!(
        String::from_utf8(real_frame).unwrap(),
        "LMCODE_RETURN_BEGIN\n{\"ok\":true,\"result\":5}\nLMCODE_RETURN_END\n"
    );
    assert_eq!(
        (real_function_calls.get(), preview_function_calls.get()),
        (1, 1)
    );
    assert_eq!(
        (real_method_calls.get(), preview_method_calls.get()),
        (1, 1)
    );
}

#[test]
fn dry_null_receiver_is_a_located_host_failure_for_a_preview_callback() {
    let mut registry = Registry::new();
    registry
        .function("thing_hole", &[], "Thing", |ctx, _| {
            Ok(Value::object("Thing", ctx.insert("Thing", DryThing(3))))
        })
        .unwrap();
    registry
        .method_with_dry(
            "Thing",
            "score",
            &[],
            "int",
            |ctx, this, _| Ok(Value::Int(ctx.get::<DryThing>(this)?.0)),
            dslvm::dry(
                |_: &mut Context,
                 _: dslvm::ObjRef,
                 _: &dslvm::Args|
                 -> Result<Value, dslvm::Error> { Ok(Value::Int(1)) },
            ),
        )
        .unwrap();
    let program = Program::load(
        &Asm::default().dry_null_receiver_program(),
        SCHEMA,
        &registry,
    )
    .unwrap();
    let mut results = InvocationResults::from_command_with_dry(&["dry".into()], true).unwrap();
    let mut context = Context::new();
    results
        .select(0, &mut context, ResultContract::default())
        .unwrap();
    let error = program.run(&mut context, &registry).unwrap_err();
    assert_eq!(error.code, "RUN_HOST");
    assert_eq!(error.line, Some(12));
    assert_eq!(error.column, Some(5));
    assert!(error
        .message
        .contains("requires a concrete receiver after dry null"));

    let real_method_calls = Rc::new(Cell::new(0));
    let mut registry = Registry::new();
    registry
        .function("thing_hole", &[], "Thing", |ctx, _| {
            Ok(Value::object("Thing", ctx.insert("Thing", DryThing(3))))
        })
        .unwrap();
    let calls = Rc::clone(&real_method_calls);
    registry
        .method("Thing", "score", &[], "int", move |ctx, this, _| {
            calls.set(calls.get() + 1);
            Ok(Value::Int(ctx.get::<DryThing>(this)?.0))
        })
        .unwrap();
    let program = Program::load(
        &Asm::default().dry_null_receiver_program(),
        SCHEMA,
        &registry,
    )
    .unwrap();
    let mut results = InvocationResults::from_command_with_dry(&["dry".into()], true).unwrap();
    let mut context = Context::new();
    results
        .select(0, &mut context, ResultContract::default())
        .unwrap();
    program.run(&mut context, &registry).unwrap();
    results.complete(0).unwrap();
    let mut frame = Vec::new();
    assert_eq!(results.write_frame(&mut frame), 0);
    assert_eq!(
        String::from_utf8(frame).unwrap(),
        "LMCODE_RETURN_BEGIN\n{\"ok\":true,\"result\":0,\"dry\":true}\nLMCODE_RETURN_END\n"
    );
    assert_eq!(real_method_calls.get(), 0);
}
