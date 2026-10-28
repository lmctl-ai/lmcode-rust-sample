use dslvm::{Context, Program, Registry, Value};
use std::{cell::RefCell, rc::Rc};

const SCHEMA: &[u8] = include_bytes!("fixtures/rule-methods/schema.json");
const BYTECODE: &[u8] = include_bytes!("fixtures/rule-methods/program.dslb");

#[test]
fn annotated_sam_rule_runs_as_inlined_v6_code() {
    assert_eq!(&BYTECODE[..6], b"DSLB\0\x06");
    let captured = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&captured);
    let mut registry = Registry::without_std();
    registry
        .function("capture", &["int"], "void", move |_, args| {
            sink.borrow_mut().push(args[0].as_int()?);
            Ok(Value::None)
        })
        .unwrap();
    assert_eq!(registry.signatures(), vec!["fn capture(int) -> void"]);
    let program = Program::load(BYTECODE, SCHEMA, &registry).unwrap();
    program.run(&mut Context::new(), &registry).unwrap();
    assert_eq!(*captured.borrow(), vec![42]);
    let changed = [SCHEMA, b" "].concat();
    let error = match Program::load(BYTECODE, &changed, &registry) {
        Err(error) => error,
        Ok(_) => panic!("schema drift must fail"),
    };
    assert_eq!(error.code, "LOAD_SCHEMA_MISMATCH");
}

#[test]
fn schema_tools_preserve_interfaces_without_runtime_signatures() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let output = std::process::Command::new("python3")
        .arg("-B")
        .arg("-c")
        .arg(
            r#"
import json, runpy
from pathlib import Path
m = runpy.run_path('scripts/merge-std.py')
host = json.loads(Path('dslvm/tests/fixtures/rule-methods/schema.json').read_bytes())
std = json.loads(Path('std/lmcode-std.schema.json').read_bytes())
merged = m['merge_schema'](host, std)
assert merged['interfaces'] == host['interfaces']
assert m['remove_standard'](merged, std)['interfaces'] == host['interfaces']
assert m['signature_strings'](host) == {'fn capture(int) -> void'}
"#,
        )
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
