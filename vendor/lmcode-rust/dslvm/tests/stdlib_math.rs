use dslvm::{stdlib, Registry};
use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn registry() -> Registry {
    let mut r = Registry::without_std();
    stdlib::register_math(&mut r).unwrap();
    r
}

#[test]
fn integer_edges_wrap_and_root_exactly() {
    assert_eq!(stdlib::math_abs(i64::MIN), i64::MIN);
    assert_eq!(stdlib::math_pow(-2, 63).unwrap(), i64::MIN);
    assert_eq!(stdlib::math_pow(0, 0).unwrap(), 1);
    assert_eq!(
        stdlib::math_pow(2, -1).unwrap_err().message,
        "math_pow: integer exponent must be nonnegative"
    );
    assert_eq!(stdlib::math_sqrt(0).unwrap(), 0);
    assert_eq!(stdlib::math_sqrt(1).unwrap(), 1);
    assert_eq!(stdlib::math_sqrt(i64::MAX).unwrap(), 3037000499);
    assert_eq!(stdlib::math_sqrt(9_223_372_030_926_249_000).unwrap(), 3_037_000_498);
    assert_eq!(
        stdlib::math_sqrt(-1).unwrap_err().message,
        "math_sqrt: integer argument must be nonnegative"
    );
    for n in [2, 3, 15, 16, 17, 9_223_372_030_926_249_001i64] {
        let r = stdlib::math_sqrt(n).unwrap();
        let rr = i128::from(r);
        let nn = i128::from(n);
        assert!(rr * rr <= nn && (i128::from(r + 1) * i128::from(r + 1) > nn));
    }
}

#[test]
fn float_edges_preserve_classification_and_ties() {
    assert!(stdlib::math_abs_float(-0.0).to_bits() == 0);
    assert!(stdlib::math_floor_float(-0.0).is_sign_negative());
    assert!(stdlib::math_ceil_float(-0.0).is_sign_negative());
    assert!(stdlib::math_round_float(-0.5) == -1.0);
    assert!(stdlib::math_round_float(0.5) == 1.0);
    assert!(stdlib::math_min_float(-0.0, 0.0).is_sign_negative());
    assert!(!stdlib::math_min_float(0.0, -0.0).is_sign_negative());
    assert!(stdlib::math_max_float(-0.0, 0.0).is_sign_negative());
    assert!(stdlib::math_min_float(f64::NAN, 1.0).is_nan());
    assert!(stdlib::math_max_float(1.0, f64::NAN).is_nan());
    assert_eq!(stdlib::math_pow_float(f64::NAN, 0.0), 1.0);
    assert_eq!(stdlib::math_pow_float(-1.0, f64::INFINITY), 1.0);
    assert!(stdlib::math_sqrt_float(-1.0).is_nan());
    assert!(stdlib::math_sqrt_float(-0.0).is_sign_negative());
}

#[test]
fn registry_has_all_overloads() {
    let signatures = registry().signatures();
    assert_eq!(signatures.len(), 16);
    assert!(signatures
        .iter()
        .any(|s| s == "fn math_pow(int, int) -> int"));
    assert!(signatures
        .iter()
        .any(|s| s == "fn math_sqrt(float) -> float"));
    let fragment =
        std::fs::read_to_string(root().join("stdlib/math.schema-fragment.json")).unwrap();
    let fixture =
        std::fs::read_to_string(root().join("dslvm/tests/fixtures/math.schema.json")).unwrap();
    let compact = |text: &str| {
        text.chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect::<String>()
    };
    let fragment = compact(&fragment);
    let fragment = fragment
        .strip_prefix('{')
        .unwrap()
        .strip_suffix('}')
        .unwrap();
    assert!(compact(&fixture).contains(fragment));
}

#[test]
fn compiler_signatures_match_registry_when_lmscript_is_available() {
    // Older compilers inject v1 defaults. Check this compatibility fragment's
    // subset here; stdlib_catalogs checks exact full canonical v2 parity.
    let schema = root().join("dslvm/tests/fixtures/math.schema.json");
    let Ok(out) = Command::new(std::env::var("LMSCRIPT_BIN").unwrap_or_else(|_| "lmscript".into()))
        .arg("--dsl-signatures")
        .arg(&schema)
        .output()
    else {
        return;
    };
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let compiler: Vec<String> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    let std_sigs = Registry::new().signatures();
    for sig in registry().signatures() { assert!(compiler.contains(&sig), "compiler missing {sig}"); }
    for sig in registry().signatures() {
        assert!(std_sigs.contains(&sig), "math fragment declares {} outside the std catalog", sig);
    }
}
