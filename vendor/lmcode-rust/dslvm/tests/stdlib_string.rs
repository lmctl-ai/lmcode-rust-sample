//! The string catalog: behaviour of the functions and parity of the registry with the shipped schema fragment and the compiler.
use dslvm::{sha256, stdlib, Context, Program, Registry, Value, VERSION};
use std::cell::RefCell;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn registry() -> Registry {
    let mut r = Registry::without_std();
    stdlib::register_string(&mut r).unwrap();
    r
}

#[test]
fn str_len_counts_bytes() {
    assert_eq!(stdlib::str_len(""), 0);
    assert_eq!(stdlib::str_len("abc"), 3);
    assert_eq!(stdlib::str_len("a\0b"), 3);
    assert_eq!(stdlib::str_len("é"), 2);
    assert_eq!(stdlib::str_len("日本"), 6);
}

#[test]
fn byte_edge_cases_match_cpp_catalog() {
    assert_eq!(stdlib::str_substr(b"a\0b", 1, i64::MAX), b"\0b");
    assert_eq!(stdlib::str_substr(b"abc", i64::MIN, i64::MAX), b"abc");
    assert_eq!(stdlib::str_trim(b" \0\t "), b"\0");
    assert_eq!(stdlib::str_lower(b"AbC-\xc3\x89"), b"abc-\xc3\x89");
    assert_eq!(stdlib::str_upper(b"aBc-\xc3\xa9"), b"ABC-\xc3\xa9");
    assert_eq!(stdlib::str_replace(b"a\0a", b"\0", b"X").unwrap(), b"aXa");
    assert!(stdlib::str_match(b"a\0b", b"a?b").unwrap());
    assert!(stdlib::str_match(b"?*[]\\", b"\\?\\*\\[\\]\\\\").unwrap());
    assert!(stdlib::str_is_int("-9223372036854775808"));
    assert!(!stdlib::str_is_int("9223372036854775808"));
    assert!(stdlib::str_is_float("4.9406564584124654e-324"));
    assert!(!stdlib::str_is_float("1e99999"));
    assert_eq!(stdlib::str_substr(b"abc", 9, 3), b"");
    assert_eq!(stdlib::str_substr(b"abc", 1, -1), b"");
    assert_eq!(stdlib::str_substr(b"abc", i64::MAX, i64::MAX), b"");
    assert_eq!(stdlib::str_substr(b"abc", 0, i64::MIN), b"");
    assert_eq!(
        stdlib::str_lower((128u8..=255).collect::<Vec<_>>()),
        (128u8..=255).collect::<Vec<_>>()
    );
    assert_eq!(
        stdlib::str_upper((128u8..=255).collect::<Vec<_>>()),
        (128u8..=255).collect::<Vec<_>>()
    );
    assert_eq!(stdlib::str_find(b"abcabc", b"bc"), 1);
    assert_eq!(stdlib::str_find(b"abc", b"z"), -1);
    assert_eq!(
        stdlib::str_replace(b"a-b-a", b"a", b"xy").unwrap(),
        b"xy-b-xy"
    );
    assert_eq!(stdlib::str_replace(b"abc", b"", b"x").unwrap(), b"abc");
    assert_eq!(stdlib::str_trim(b" \tvalue\r\n"), b"value");
    assert_eq!(stdlib::str_trim(b"\xc2\xa0x\xc2\xa0"), b"\xc2\xa0x\xc2\xa0");
    assert_eq!(stdlib::str_repeat(b"xy", 3).unwrap(), b"xyxyxy");
    assert_eq!(stdlib::str_repeat(b"x", 0).unwrap(), b"");
    assert_eq!(stdlib::str_repeat(b"", i64::MAX).unwrap(), b"");
    assert!(stdlib::str_repeat(b"ab", i64::MAX).is_err());
    for n in [i64::MIN, -1] {
        assert!(stdlib::str_repeat(b"", n).is_err());
        assert!(stdlib::str_repeat(b"x", n).is_err());
    }
    assert_eq!(stdlib::str_to_int("-9223372036854775808"), i64::MIN);
    assert_eq!(stdlib::str_to_int("+12"), 12);
    assert!(!stdlib::str_is_int("9223372036854775808"));
    for token in [
        "+-1", "++1", "--1", "-+1", "+", "-", " 1", "1 ", "0x10", "1,5",
    ] {
        assert!(!stdlib::str_is_int(token));
        assert_eq!(stdlib::str_to_int(token), 0);
        assert!(!stdlib::str_is_float(token));
        assert_eq!(stdlib::str_to_float(token), 0.0);
    }
    assert!(!stdlib::str_is_int(b"1\0"));
    assert!(!stdlib::str_is_float(b"1\0"));
    assert!(
        stdlib::str_is_float(".5") && stdlib::str_is_float("1.") && stdlib::str_is_float("+1e-2")
    );
    assert_eq!(stdlib::str_to_float("-1.25e2"), -125.0);
    assert_eq!(stdlib::str_to_float("+2.5"), 2.5);
    for token in [
        ".",
        "1e",
        "1e+",
        "1e-",
        "1..0",
        "0X1P2",
        "-inf",
        "+NaN",
        "nan(payload)",
        "\t1",
        "1\n",
    ] {
        assert!(!stdlib::str_is_float(token));
        assert_eq!(stdlib::str_to_float(token), 0.0);
    }
    assert_eq!(
        stdlib::str_to_float("4.9406564584124654e-324"),
        f64::from_bits(1)
    );
    assert!(!stdlib::str_is_float("2.4703282292062327e-324"));
    assert!(stdlib::str_is_float("2.4703282292062328e-324"));
    assert!(stdlib::str_to_float("-0").is_sign_negative());
    assert!(stdlib::str_is_float("0e99999") && stdlib::str_is_float("0e-99999"));
    assert!(stdlib::str_match(b"a/b.txt", b"a/*.txt").unwrap());
    assert!(stdlib::str_match(b"a", b"[!b]").unwrap());
    assert!(!stdlib::str_match(b"x", b"[z-a]").unwrap());
    assert!(stdlib::str_match(b"]", br"[\]]").unwrap());
    assert!(stdlib::str_match(b"-", b"[-a]").unwrap());
    assert!(stdlib::str_match(b"-", b"[a-]").unwrap());
    assert!(!stdlib::str_match(b"z", b"[z-a]").unwrap());
    assert!(stdlib::str_match(b"[]", b"[]").unwrap());
    assert!(stdlib::str_match(b"[!", b"[!").unwrap());
    assert!(stdlib::str_match(b"", b"***").unwrap());
    assert!(!stdlib::str_match(b"", b"?").unwrap());
    assert!(!stdlib::str_match(b"x", b"").unwrap());
    assert!(stdlib::str_match([0x80], [b'[', 0x80, b']']).unwrap());
}

#[test]
fn prefixes_suffixes_and_search_are_byte_exact() {
    assert!(stdlib::str_starts_with(b"alpha", b"al"));
    assert!(stdlib::str_starts_with(b"", b""));
    assert!(!stdlib::str_starts_with(b"a", b"ab"));
    assert!(stdlib::str_ends_with(b"alpha", b"ha"));
    assert!(stdlib::str_ends_with(b"", b""));
    assert!(!stdlib::str_ends_with(b"a", b"ba"));
    assert!(stdlib::str_contains(b"abc", b""));
    assert!(stdlib::str_contains(b"abc", b"b"));
    assert!(!stdlib::str_contains(b"abc", b"z"));
    assert_eq!(stdlib::str_find(b"abc", b""), 0);
    assert_eq!(stdlib::str_find(b"a\0b\0", b"\0b"), 1);
    assert_eq!(stdlib::str_find(b"\x80\xff", b"\xff"), 1);
    assert_eq!(stdlib::str_substr("é", 0, 1), b"\xc3");
    assert_eq!(stdlib::str_substr("é", 1, 1), b"\xa9");
}

#[test]
fn numeric_limits_and_rounding_match_cpp() {
    assert!(stdlib::str_is_int("9223372036854775807"));
    assert!(stdlib::str_is_int("-9223372036854775808"));
    assert!(!stdlib::str_is_int("-9223372036854775809"));
    assert!(!stdlib::str_is_int("9".repeat(10_000)));
    assert!(!stdlib::str_is_float("9".repeat(10_000)));
    assert!(!stdlib::str_is_float("1e99999"));
    assert!(!stdlib::str_is_float("1e-99999"));
    assert!(stdlib::str_is_float("4.9406564584124654e-324"));
    assert_eq!(
        stdlib::str_to_float("4.9406564584124654e-324"),
        f64::from_bits(1)
    );
    assert!(!stdlib::str_is_float("2.4703282292062327e-324"));
    assert!(stdlib::str_is_float("2.4703282292062328e-324"));
    assert_eq!(
        stdlib::str_to_float("2.2250738585072014e-308"),
        f64::MIN_POSITIVE
    );
    assert_eq!(stdlib::str_to_float("1.7976931348623157e308"), f64::MAX);
    assert!(!stdlib::str_is_float("1.7976931348623159e308"));
    assert_eq!(
        stdlib::str_to_float("1.00000000000000011102230246251565404236316680908203125"),
        1.0
    );
    assert_eq!(
        stdlib::str_to_float("1.00000000000000011102230246251565404236316680908203126"),
        f64::from_bits(0x3ff0000000000001)
    );
    assert!(stdlib::str_to_float("-0.0e-99999").is_sign_negative());
}

#[test]
fn glob_classes_escapes_and_work_limit_match_cpp() {
    assert!(stdlib::str_match(b"?*[]\\", b"\\?\\*\\[\\]\\\\").unwrap());
    assert!(stdlib::str_match(b"]", b"[\\]]").unwrap());
    assert!(stdlib::str_match(b"-", b"[a\\-c]").unwrap());
    assert!(!stdlib::str_match(b"b", b"[a\\-c]").unwrap());
    assert!(stdlib::str_match(b"\\", b"[\\\\]").unwrap());
    assert!(stdlib::str_match(b"!", b"[\\!]").unwrap());
    assert!(stdlib::str_match(b"]", b"[]]").unwrap());
    assert!(stdlib::str_match(b"a", b"[!]]").unwrap());
    assert!(!stdlib::str_match(b"]", b"[!]]").unwrap());
    assert!(stdlib::str_match(b"b", b"[a-\\c]").unwrap());
    assert!(stdlib::str_match(b"", b"***").unwrap());
    assert!(!stdlib::str_match(b"x", b"").unwrap());
    let adversarial = vec![b'a'; 12_000];
    assert!(!stdlib::str_match(&adversarial, b"*aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaab").unwrap());
    for _ in 0..2 {
        let malformed = vec![b'['; 20_000];
        let error = stdlib::str_match(&malformed, &malformed).unwrap_err();
        assert_eq!(error.code, "RUN_HOST");
        assert_eq!(
            error.message,
            "str_match: pattern exceeds the 2^27 work limit"
        );
    }
}

#[test]
fn size_caps_are_checked_before_output_allocation() {
    let mb = vec![b'x'; 1024 * 1024];
    assert!(stdlib::str_replace(vec![b'x'; 65], b"x", &mb).is_err());
    assert_eq!(
        stdlib::str_replace(vec![b'x'; 64], b"x", &mb)
            .unwrap()
            .len(),
        64 * 1024 * 1024
    );
    let repeat_error = stdlib::str_repeat(b"ab", i64::MAX).unwrap_err();
    assert_eq!(repeat_error.code, "RUN_HOST");
    assert_eq!(
        repeat_error.message,
        "str_repeat: count must be nonnegative and result at most 67108864 bytes"
    );
    let no_match_error = stdlib::str_replace(vec![b'x'; 65 * 1024 * 1024], b"z", b"x").unwrap_err();
    assert_eq!(no_match_error.code, "RUN_HOST");
    assert_eq!(
        no_match_error.message,
        "str_replace: result must be at most 67108864 bytes"
    );
    let noop_error = stdlib::str_replace(vec![b'x'; 65 * 1024 * 1024], b"", b"x").unwrap_err();
    assert_eq!(noop_error.code, "RUN_HOST");
    assert_eq!(
        noop_error.message,
        "str_replace: result must be at most 67108864 bytes"
    );
}

fn upper_chunk(schema: &[u8], input: &[u8]) -> Vec<u8> {
    fn u16(out: &mut Vec<u8>, value: u16) {
        out.extend_from_slice(&value.to_be_bytes());
    }
    fn u32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_be_bytes());
    }
    let mut out = b"DSLB".to_vec();
    u16(&mut out, VERSION);
    out.extend_from_slice(&sha256(schema));
    let constants = [
        input,
        b"fn str_upper(string) -> string",
        b"fn capture(string) -> void",
    ];
    u32(&mut out, constants.len() as u32);
    for constant in constants {
        out.push(1);
        u32(&mut out, constant.len() as u32);
        out.extend_from_slice(constant);
    }
    u16(&mut out, 2);
    u32(&mut out, 1);
    u32(&mut out, 2);
    u16(&mut out, 0);
    u16(&mut out, 0);
    u16(&mut out, 1);
    let code = [0x02, 0, 0, 0, 0, 0x07, 0, 0, 1, 0x07, 0, 1, 1, 0x11];
    u32(&mut out, code.len() as u32);
    out.extend_from_slice(&code);
    u32(&mut out, 0);
    u16(&mut out, 0); // v6: no rule entries
    out
}

#[test]
fn vm_dispatch_preserves_nul_bytes_through_upper_and_capture() {
    let schema = br#"{"dsl.schema":2,"name":"string_callbacks","types":{},"context":{},"functions":{"str_upper":[{"params":["string"],"returns":"string"}],"capture":[{"params":["string"],"returns":"void"}]}}"#;
    let captured = Rc::new(RefCell::new(Vec::<Vec<u8>>::new()));
    let mut registry = Registry::without_std();
    stdlib::register_string(&mut registry).unwrap();
    let sink = Rc::clone(&captured);
    registry
        .function("capture", &["string"], "void", move |_, args| {
            sink.borrow_mut().push(args[0].as_bytes()?.to_vec());
            Ok(Value::None)
        })
        .unwrap();
    let first_bytes = upper_chunk(schema, b"a\0b\xff");
    let second_bytes = upper_chunk(schema, b"second");
    let first = Program::load(&first_bytes, schema, &registry).unwrap();
    let second = Program::load(&second_bytes, schema, &registry).unwrap();
    let mut context = Context::new();
    first.run(&mut context, &registry).unwrap();
    second.run(&mut context, &registry).unwrap();
    first.run(&mut context, &registry).unwrap();
    assert_eq!(
        &*captured.borrow(),
        &[
            b"A\0B\xff".to_vec(),
            b"SECOND".to_vec(),
            b"A\0B\xff".to_vec()
        ]
    );
}

#[test]
fn registry_signatures_match_fragment() {
    let signatures = registry().signatures();
    assert_eq!(signatures.len(), 16);
    assert!(signatures
        .iter()
        .any(|s| s == "fn str_match(string, string) -> bool"));
    let fragment =
        std::fs::read_to_string(root().join("stdlib/string.schema-fragment.json")).unwrap();
    let fixture =
        std::fs::read_to_string(root().join("dslvm/tests/fixtures/string.schema.json")).unwrap();
    // The complete functions object is shared verbatim; ignore indentation and line endings.
    // Compiler parity below checks every declared parameter/return type against the registry.
    let compact = |text: &str| {
        text.chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect::<String>()
    };
    let fragment = compact(&fragment);
    let fixture = compact(&fixture);
    assert!(fixture.contains(
        fragment
            .strip_prefix('{')
            .unwrap()
            .strip_suffix('}')
            .unwrap()
    ));
}

#[test]
fn compiler_signatures_match_registry_when_lmscript_is_available() {
    // Older compilers inject v1 defaults. Check this compatibility fragment's
    // subset here; stdlib_catalogs checks exact full canonical v2 parity.
    let schema = root().join("dslvm/tests/fixtures/string.schema.json");
    let Ok(out) = Command::new(std::env::var("LMSCRIPT_BIN").unwrap_or_else(|_| "lmscript".into()))
        .arg("--lmcode-signatures")
        .arg(&schema)
        .output()
    else {
        eprintln!("lmscript not found: compiler parity skipped");
        return;
    };
    assert!(
        out.status.success(),
        "lmscript --lmcode-signatures failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let compiler: Vec<String> = String::from_utf8(out.stdout)
        .expect("compiler signatures must be UTF-8")
        .lines()
        .map(str::to_string)
        .collect();
    let std_sigs = Registry::new().signatures();
    for sig in registry().signatures() { assert!(compiler.contains(&sig), "compiler missing {sig}"); }
    for sig in registry().signatures() {
        assert!(std_sigs.contains(&sig), "string fragment declares {} outside the std catalog", sig);
    }
}
