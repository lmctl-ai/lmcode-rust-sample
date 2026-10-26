//! Optional host functions matching lmcode-cpp's string catalog. Copy the declarations from
//! `stdlib/string.schema-fragment.json` into the host schema, then call [`register_string`]
//! once per registry. Every operation uses bytes; UTF-8 is never decoded. Repeat and replace
//! cap results at 64 MiB, and glob matching caps work at 2^27 units. These are host policies,
//! independent of the VM, which imposes no string or execution limits.
pub mod collections;
pub mod containers;
pub mod json;
pub use containers::register_containers;
pub use json::register_json;
pub mod testing;
pub mod file;
pub use collections::register_collections;
pub use testing::register_testing;
pub use file::register_file;

use crate::{Error, Registry, Value};
use std::collections::HashSet;
pub mod math;
pub use math::*;

/// Install the complete built-in lmcode standard catalog once. Explicit subset
/// binders remain compatible: built-in entries may be replaced by a host binder.
pub fn register_std(registry: &mut Registry) -> Result<&mut Registry, Error> {
    if registry.std_enabled() { return Ok(registry); }
    let before: HashSet<String> = registry.signatures().into_iter().collect();
    register_string(registry)?;
    register_math(registry)?;
    register_collections(registry)?;
    register_file(registry)?;
    register_containers(registry)?;
    register_json(registry)?;
    registry.mark_std_since(&before);
    Ok(registry)
}
const MAX_BYTES: usize = 64 * 1024 * 1024;
/// Byte length, including embedded NULs.
pub fn str_len<S: AsRef<[u8]>>(s: S) -> i64 {
    s.as_ref().len() as i64
}
/// Prefix test; an empty prefix matches.
pub fn str_starts_with<S: AsRef<[u8]>, P: AsRef<[u8]>>(s: S, p: P) -> bool {
    s.as_ref().starts_with(p.as_ref())
}
/// Suffix test; an empty suffix matches.
pub fn str_ends_with<S: AsRef<[u8]>, P: AsRef<[u8]>>(s: S, p: P) -> bool {
    s.as_ref().ends_with(p.as_ref())
}
/// Substring test; an empty needle matches.
pub fn str_contains<S: AsRef<[u8]>, N: AsRef<[u8]>>(s: S, n: N) -> bool {
    str_find(s, n) >= 0
}
/// First byte offset, or -1 if absent; an empty needle returns 0.
pub fn str_find<S: AsRef<[u8]>, N: AsRef<[u8]>>(s: S, n: N) -> i64 {
    let (s, n) = (s.as_ref(), n.as_ref());
    if n.is_empty() {
        return 0;
    }
    s.windows(n.len())
        .position(|w| w == n)
        .map(|x| x as i64)
        .unwrap_or(-1)
}
/// Byte slice: negative starts clamp to zero, nonpositive counts return empty, and ends clamp.
pub fn str_substr<S: AsRef<[u8]>>(s: S, start: i64, count: i64) -> Vec<u8> {
    let s = s.as_ref();
    let start = start.max(0) as u64;
    if count <= 0 || start >= s.len() as u64 {
        return Vec::new();
    }
    let offset = start as usize;
    let count = (count as u64).min((s.len() - offset) as u64) as usize;
    s[offset..offset + count].to_vec()
}

/// ASCII lowercase; all bytes outside A-Z are unchanged.
pub fn str_lower<S: AsRef<[u8]>>(s: S) -> Vec<u8> {
    s.as_ref()
        .iter()
        .map(|&c| if c.is_ascii_uppercase() { c + 32 } else { c })
        .collect()
}
/// ASCII uppercase; all bytes outside a-z are unchanged.
pub fn str_upper<S: AsRef<[u8]>>(s: S) -> Vec<u8> {
    s.as_ref()
        .iter()
        .map(|&c| if c.is_ascii_lowercase() { c - 32 } else { c })
        .collect()
}
fn ws(c: u8) -> bool {
    c == b' ' || (b'\t'..=b'\r').contains(&c)
}
/// Trim ASCII space and bytes 0x09 through 0x0d; NUL and non-ASCII bytes are unchanged.
pub fn str_trim<S: AsRef<[u8]>>(s: S) -> Vec<u8> {
    let s = s.as_ref();
    let a = s.iter().position(|&c| !ws(c)).unwrap_or(s.len());
    let b = s.iter().rposition(|&c| !ws(c)).map(|i| i + 1).unwrap_or(a);
    s[a..b].to_vec()
}
/// Repeat count times, rejecting negative counts and results over 64 MiB before allocation.
pub fn str_repeat<S: AsRef<[u8]>>(s: S, count: i64) -> Result<Vec<u8>, Error> {
    let s = s.as_ref();
    if s.is_empty() && count >= 0 {
        return Ok(Vec::new());
    }
    if count < 0 || (!s.is_empty() && (count as u64) > (MAX_BYTES / s.len()) as u64) {
        return Err(Error::host(
            "str_repeat: count must be nonnegative and result at most 67108864 bytes",
        ));
    }
    Ok(s.repeat(count as usize))
}
/// Replace non-overlapping matches, preflighting the 64 MiB result cap even for no-op copies.
pub fn str_replace<S: AsRef<[u8]>, F: AsRef<[u8]>, T: AsRef<[u8]>>(
    s: S,
    from: F,
    to: T,
) -> Result<Vec<u8>, Error> {
    let (s, from, to) = (s.as_ref(), from.as_ref(), to.as_ref());
    if from.is_empty() {
        if s.len() > MAX_BYTES {
            return Err(Error::host(
                "str_replace: result must be at most 67108864 bytes",
            ));
        }
        return Ok(s.to_vec());
    }
    let mut size = 0usize;
    let mut at = 0;
    while let Some(i) = s[at..].windows(from.len()).position(|w| w == from) {
        let p = at + i;
        size = size
            .checked_add(p - at)
            .and_then(|n| n.checked_add(to.len()))
            .unwrap_or(MAX_BYTES + 1);
        if size > MAX_BYTES {
            return Err(Error::host(
                "str_replace: result must be at most 67108864 bytes",
            ));
        }
        at = p + from.len()
    }
    size = size.checked_add(s.len() - at).unwrap_or(MAX_BYTES + 1);
    if size > MAX_BYTES {
        return Err(Error::host(
            "str_replace: result must be at most 67108864 bytes",
        ));
    }
    let mut out = Vec::with_capacity(size);
    at = 0;
    while let Some(i) = s[at..].windows(from.len()).position(|w| w == from) {
        let p = at + i;
        out.extend_from_slice(&s[at..p]);
        out.extend_from_slice(to);
        at = p + from.len();
    }
    out.extend_from_slice(&s[at..]);
    Ok(out)
}
/// Whether a complete signed decimal token fits in an i64; no whitespace is accepted.
pub fn str_is_int<S: AsRef<[u8]>>(s: S) -> bool {
    parse_int(s.as_ref()).is_some()
}
/// Parse a signed decimal i64, returning zero for invalid or out-of-range tokens.
pub fn str_to_int<S: AsRef<[u8]>>(s: S) -> i64 {
    parse_int(s.as_ref()).unwrap_or(0)
}
fn parse_int(s: &[u8]) -> Option<i64> {
    if s.is_empty() {
        return None;
    }
    let mut b = s;
    if b[0] == b'+' || b[0] == b'-' {
        b = &b[1..]
    }
    if b.is_empty() || b.iter().any(|c| !c.is_ascii_digit()) {
        return None;
    }
    std::str::from_utf8(s).ok()?.parse().ok()
}
/// Whether a complete decimal token rounds to a finite f64 without nonzero underflow to zero.
pub fn str_is_float<S: AsRef<[u8]>>(s: S) -> bool {
    parse_float(s.as_ref()).is_some()
}
/// Parse a decimal f64; invalid syntax, overflow and nonzero underflow to zero return zero.
pub fn str_to_float<S: AsRef<[u8]>>(s: S) -> f64 {
    parse_float(s.as_ref()).unwrap_or(0.0)
}
fn parse_float(s: &[u8]) -> Option<f64> {
    if s.is_empty() {
        return None;
    }
    let mut i = usize::from(s[0] == b'+' || s[0] == b'-');
    let mut digits = false;
    let mut nonzero = false;
    while i < s.len() && s[i].is_ascii_digit() {
        digits = true;
        nonzero |= s[i] != b'0';
        i += 1
    }
    if i < s.len() && s[i] == b'.' {
        i += 1;
        while i < s.len() && s[i].is_ascii_digit() {
            digits = true;
            nonzero |= s[i] != b'0';
            i += 1
        }
    }
    if !digits {
        return None;
    }
    if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
        i += 1;
        if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
            i += 1
        }
        let e = i;
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1
        }
        if i == e {
            return None;
        }
    }
    if i != s.len() {
        return None;
    }
    let v: f64 = std::str::from_utf8(s).ok()?.parse().ok()?;
    if !v.is_finite() || (v == 0.0 && nonzero) {
        None
    } else {
        Some(v)
    }
}
struct GlobWork(u64);
impl GlobWork {
    fn step(&mut self) -> Result<(), Error> {
        if self.0 == 1 << 27 {
            Err(Error::host(
                "str_match: pattern exceeds the 2^27 work limit",
            ))
        } else {
            self.0 += 1;
            Ok(())
        }
    }
}
fn token(p: &[u8], i: usize, c: u8, next: &mut usize, w: &mut GlobWork) -> Result<bool, Error> {
    if p[i] == b'\\' && i + 1 < p.len() {
        *next = i + 2;
        return Ok(p[i + 1] == c);
    }
    if p[i] == b'?' {
        *next = i + 1;
        return Ok(true);
    }
    if p[i] == b'[' {
        let mut q = i + 1;
        let neg = q < p.len() && p[q] == b'!';
        if neg {
            q += 1
        }
        let first = q;
        let mut hit = false;
        while q < p.len() && (p[q] != b']' || q == first) {
            w.step()?;
            let lo = class_byte(p, &mut q);
            if q + 1 < p.len() && p[q] == b'-' && p[q + 1] != b']' {
                q += 1;
                let hi = class_byte(p, &mut q);
                if lo <= hi && c >= lo && c <= hi {
                    hit = true
                }
            } else if lo == c {
                hit = true
            }
        }
        if q < p.len() {
            *next = q + 1;
            return Ok(hit != neg);
        }
    }
    *next = i + 1;
    Ok(p[i] == c)
}

fn class_byte(pattern: &[u8], cursor: &mut usize) -> u8 {
    if pattern[*cursor] == b'\\' && *cursor + 1 < pattern.len() {
        *cursor += 1;
    }
    let byte = pattern[*cursor];
    *cursor += 1;
    byte
}
/// Whole-string byte glob: *, ?, classes/ranges, [!class], and backslash escapes.
/// Uses constant auxiliary memory and charges iterations, class entries and trailing stars
/// to a per-call 2^27 work cap. Unterminated classes and trailing backslashes are literal.
pub fn str_match<S: AsRef<[u8]>, P: AsRef<[u8]>>(text: S, pattern: P) -> Result<bool, Error> {
    let (t, p) = (text.as_ref(), pattern.as_ref());
    let mut w = GlobWork(0);
    let (mut ti, mut pi) = (0, 0);
    let (mut star, mut star_t) = (None, 0);
    while ti < t.len() {
        w.step()?;
        let mut next = 0;
        if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            pi += 1;
            star_t = ti
        } else if pi < p.len() && token(p, pi, t[ti], &mut next, &mut w)? {
            pi = next;
            ti += 1
        } else if let Some(sp) = star {
            star_t += 1;
            ti = star_t;
            pi = sp + 1
        } else {
            return Ok(false);
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        w.step()?;
        pi += 1
    }
    Ok(pi == p.len())
}
/// Register all 16 optional string functions once, using their canonical schema signatures.
pub fn register_string(reg: &mut Registry) -> Result<&mut Registry, Error> {
    reg.begin_compatibility_registration();
    reg.function("str_starts_with", &["string", "string"], "bool", |_, a| {
        Ok(Value::Bool(str_starts_with(
            a[0].as_bytes()?,
            a[1].as_bytes()?,
        )))
    })?;
    reg.function("str_ends_with", &["string", "string"], "bool", |_, a| {
        Ok(Value::Bool(str_ends_with(
            a[0].as_bytes()?,
            a[1].as_bytes()?,
        )))
    })?;
    reg.function("str_contains", &["string", "string"], "bool", |_, a| {
        Ok(Value::Bool(str_contains(
            a[0].as_bytes()?,
            a[1].as_bytes()?,
        )))
    })?;
    reg.function("str_len", &["string"], "int", |_, a| {
        Ok(Value::Int(str_len(a[0].as_bytes()?)))
    })?;
    reg.function("str_substr", &["string", "int", "int"], "string", |_, a| {
        Ok(Value::string(str_substr(
            a[0].as_bytes()?,
            a[1].as_int()?,
            a[2].as_int()?,
        )))
    })?;
    reg.function("str_lower", &["string"], "string", |_, a| {
        Ok(Value::string(str_lower(a[0].as_bytes()?)))
    })?;
    reg.function("str_upper", &["string"], "string", |_, a| {
        Ok(Value::string(str_upper(a[0].as_bytes()?)))
    })?;
    reg.function("str_trim", &["string"], "string", |_, a| {
        Ok(Value::string(str_trim(a[0].as_bytes()?)))
    })?;
    reg.function("str_repeat", &["string", "int"], "string", |_, a| {
        Ok(Value::string(str_repeat(a[0].as_bytes()?, a[1].as_int()?)?))
    })?;
    reg.function("str_to_int", &["string"], "int", |_, a| {
        Ok(Value::Int(str_to_int(a[0].as_bytes()?)))
    })?;
    reg.function("str_is_int", &["string"], "bool", |_, a| {
        Ok(Value::Bool(str_is_int(a[0].as_bytes()?)))
    })?;
    reg.function("str_to_float", &["string"], "float", |_, a| {
        Ok(Value::Float(str_to_float(a[0].as_bytes()?)))
    })?;
    reg.function("str_is_float", &["string"], "bool", |_, a| {
        Ok(Value::Bool(str_is_float(a[0].as_bytes()?)))
    })?;
    reg.function("str_find", &["string", "string"], "int", |_, a| {
        Ok(Value::Int(str_find(a[0].as_bytes()?, a[1].as_bytes()?)))
    })?;
    reg.function(
        "str_replace",
        &["string", "string", "string"],
        "string",
        |_, a| {
            Ok(Value::string(str_replace(
                a[0].as_bytes()?,
                a[1].as_bytes()?,
                a[2].as_bytes()?,
            )?))
        },
    )?;
    reg.function("str_match", &["string", "string"], "bool", |_, a| {
        Ok(Value::Bool(str_match(a[0].as_bytes()?, a[1].as_bytes()?)?))
    })?;
    reg.end_compatibility_registration();
    Ok(reg)
}
