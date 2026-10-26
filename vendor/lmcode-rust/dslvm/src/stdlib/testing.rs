//! Optional testing helpers matching the C++ catalog.
use crate::{Error, Registry, Value};
use std::cell::RefCell;
use std::io::{self, Write};
use std::rc::Rc;

pub type Sink = Rc<RefCell<dyn Write>>;

fn print_bytes(sink: &Sink, value: &[u8]) -> Result<(), Error> {
    let mut out = sink.borrow_mut();
    out.write_all(value).map_err(|e| Error::host(format!("print: {}", e)))?;
    out.write_all(b"\n").map_err(|e| Error::host(format!("print: {}", e)))
}

/// Compare two floats with an explicit absolute tolerance using the direct C++ comparison
/// semantics; NaN and ordinary non-matching infinities fail, while a sufficiently large
/// tolerance can make any finite or infinite difference pass.
pub fn testing_assert_close(a: f64, b: f64, tolerance: f64) -> Result<(), Error> {
    if (a - b).abs() <= tolerance { return Ok(()); }
    Err(Error::host(format!("assert_close failed: {} vs {} (tol {})", crate::format_g15(a), crate::format_g15(b), crate::format_g15(tolerance))))
}

/// Register `print(string)` and `assert_close(float,float,float)`. `None` writes to stdout.
pub fn register_testing(registry: &mut Registry, sink: Option<Sink>) -> Result<&mut Registry, Error> {
    let sink = sink.unwrap_or_else(|| Rc::new(RefCell::new(io::stdout())));
    registry.function("print", &["string"], "void", move |_, args| {
        print_bytes(&sink, args[0].as_bytes()?)?;
        Ok(Value::None)
    })?;
    registry.function("assert_close", &["float", "float", "float"], "void", |_, args| {
        testing_assert_close(args[0].as_float()?, args[1].as_float()?, args[2].as_float()?)?;
        Ok(Value::None)
    })?;
    Ok(registry)
}

#[cfg(test)]
#[path = "testing_tests.rs"]
mod testing_tests;

