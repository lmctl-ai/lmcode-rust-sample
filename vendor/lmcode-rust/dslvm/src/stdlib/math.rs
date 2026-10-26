//! Optional numeric functions matching the shared lmcode math catalog.

use crate::{Error, Registry, Value};

pub fn math_abs(i: i64) -> i64 {
    i.wrapping_abs()
}
pub fn math_abs_float(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & !(1u64 << 63))
}

pub fn math_min(i: i64, j: i64) -> i64 {
    if i <= j {
        i
    } else {
        j
    }
}
pub fn math_max(i: i64, j: i64) -> i64 {
    if i >= j {
        i
    } else {
        j
    }
}

fn float_min(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        a
    } else if b.is_nan() {
        b
    } else if a <= b {
        a
    } else {
        b
    }
}
fn float_max(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        a
    } else if b.is_nan() {
        b
    } else if a >= b {
        a
    } else {
        b
    }
}
pub fn math_min_float(a: f64, b: f64) -> f64 {
    float_min(a, b)
}
pub fn math_max_float(a: f64, b: f64) -> f64 {
    float_max(a, b)
}

pub fn math_floor(i: i64) -> i64 {
    i
}
pub fn math_ceil(i: i64) -> i64 {
    i
}
pub fn math_round(i: i64) -> i64 {
    i
}
pub fn math_floor_float(x: f64) -> f64 {
    x.floor()
}
pub fn math_ceil_float(x: f64) -> f64 {
    x.ceil()
}
pub fn math_round_float(x: f64) -> f64 {
    x.round()
}

pub fn math_pow(i: i64, exponent: i64) -> Result<i64, Error> {
    if exponent < 0 {
        return Err(Error::host(
            "math_pow: integer exponent must be nonnegative",
        ));
    }
    let mut base = i as u64;
    let mut n = exponent as u64;
    let mut result = 1u64;
    while n != 0 {
        if n & 1 != 0 {
            result = result.wrapping_mul(base);
        }
        n >>= 1;
        if n != 0 {
            base = base.wrapping_mul(base);
        }
    }
    Ok(result as i64)
}
pub fn math_pow_float(base: f64, exponent: f64) -> f64 {
    if exponent == 0.0 || base == 1.0 {
        return 1.0;
    }
    if base == -1.0 && exponent.is_infinite() {
        return 1.0;
    }
    if base.is_nan() || exponent.is_nan() {
        return f64::NAN;
    }
    base.powf(exponent)
}

fn integer_sqrt(n: u64) -> u64 {
    let mut lo = 0u64;
    let mut hi = 1u64 << 32;
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if mid <= n / mid {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}
pub fn math_sqrt(i: i64) -> Result<i64, Error> {
    if i < 0 {
        return Err(Error::host(
            "math_sqrt: integer argument must be nonnegative",
        ));
    }
    Ok(integer_sqrt(i as u64) as i64)
}
pub fn math_sqrt_float(x: f64) -> f64 {
    x.sqrt()
}

/// Register the numeric catalog. Each name has an int and float overload where applicable.
pub fn register_math(reg: &mut Registry) -> Result<&mut Registry, Error> {
    reg.begin_compatibility_registration();
    reg.function("math_abs", &["int"], "int", |_, a| {
        Ok(Value::Int(math_abs(a[0].as_int()?)))
    })?;
    reg.function("math_abs", &["float"], "float", |_, a| {
        Ok(Value::Float(math_abs_float(a[0].as_float()?)))
    })?;
    reg.function("math_min", &["int", "int"], "int", |_, a| {
        Ok(Value::Int(math_min(a[0].as_int()?, a[1].as_int()?)))
    })?;
    reg.function("math_min", &["float", "float"], "float", |_, a| {
        Ok(Value::Float(math_min_float(
            a[0].as_float()?,
            a[1].as_float()?,
        )))
    })?;
    reg.function("math_max", &["int", "int"], "int", |_, a| {
        Ok(Value::Int(math_max(a[0].as_int()?, a[1].as_int()?)))
    })?;
    reg.function("math_max", &["float", "float"], "float", |_, a| {
        Ok(Value::Float(math_max_float(
            a[0].as_float()?,
            a[1].as_float()?,
        )))
    })?;
    reg.function("math_floor", &["int"], "int", |_, a| {
        Ok(Value::Int(math_floor(a[0].as_int()?)))
    })?;
    reg.function("math_floor", &["float"], "float", |_, a| {
        Ok(Value::Float(math_floor_float(a[0].as_float()?)))
    })?;
    reg.function("math_ceil", &["int"], "int", |_, a| {
        Ok(Value::Int(math_ceil(a[0].as_int()?)))
    })?;
    reg.function("math_ceil", &["float"], "float", |_, a| {
        Ok(Value::Float(math_ceil_float(a[0].as_float()?)))
    })?;
    reg.function("math_round", &["int"], "int", |_, a| {
        Ok(Value::Int(math_round(a[0].as_int()?)))
    })?;
    reg.function("math_round", &["float"], "float", |_, a| {
        Ok(Value::Float(math_round_float(a[0].as_float()?)))
    })?;
    reg.function("math_pow", &["int", "int"], "int", |_, a| {
        Ok(Value::Int(math_pow(a[0].as_int()?, a[1].as_int()?)?))
    })?;
    reg.function("math_pow", &["float", "float"], "float", |_, a| {
        Ok(Value::Float(math_pow_float(
            a[0].as_float()?,
            a[1].as_float()?,
        )))
    })?;
    reg.function("math_sqrt", &["int"], "int", |_, a| {
        Ok(Value::Int(math_sqrt(a[0].as_int()?)?))
    })?;
    reg.function("math_sqrt", &["float"], "float", |_, a| {
        Ok(Value::Float(math_sqrt_float(a[0].as_float()?)))
    })?;
    reg.end_compatibility_registration();
    Ok(reg)
}
