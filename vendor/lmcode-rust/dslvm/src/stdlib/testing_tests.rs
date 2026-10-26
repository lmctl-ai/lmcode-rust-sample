//! Shared tests for the optional testing catalog.
use super::{register_testing, testing_assert_close, Sink};
use crate::{Registry, Value};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn assert_close_matches_cpp_policy_and_message() {
    assert!(testing_assert_close(1.0, 1.0001, 0.001).is_ok());
    let error = testing_assert_close(1.2345678901234567, 2.0, 0.25).unwrap_err();
    assert_eq!(error.code, "RUN_HOST");
    assert_eq!(error.message, "assert_close failed: 1.23456789012346 vs 2 (tol 0.25)");
    for (a, b, tolerance) in [(f64::NAN, 1.0, 1.0), (1.0, f64::NAN, 1.0), (1.0, 1.0, f64::NAN), (1.0, 1.0, -1.0), (f64::INFINITY, f64::INFINITY, 0.0)] {
        assert!(testing_assert_close(a, b, tolerance).is_err());
    }
    assert!(testing_assert_close(f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY).is_ok());
}

#[test]
fn print_writes_raw_bytes_and_newline() {
    let bytes = Rc::new(RefCell::new(Vec::<u8>::new()));
    let output: Sink = bytes.clone();
    let mut registry = Registry::without_std();
    register_testing(&mut registry, Some(output.clone())).unwrap();
    let (_, _, print) = registry.functions.get("fn print(string) -> void").unwrap();
    let args = [Value::string(b"a\0b".as_slice())];
    print(&mut crate::Context::new(), &args).unwrap();
    assert_eq!(&*bytes.borrow(), b"a\0b\n");
}
