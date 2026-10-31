//! Invocation-owned return values. Hosts configure a sink before executing a body
//! and retain it across entry calls. This module never prints or resets on VM runs.
use super::collections::{FloatList, IntList, StrList, StrMap};
use super::containers::{FloatMap, IntMap};
use super::json::{JsonNode, Kind};
use crate::{Context, Error, Registry, Value};
use std::rc::Rc;

#[derive(Clone, Debug, Default)]
pub(super) enum OwnedJson {
    #[default]
    Null,
    Bool(bool),
    Number(String),
    Float(String),
    String(String),
    Array(Vec<OwnedJson>),
    Object(Vec<(String, OwnedJson)>),
}
/// A shared binding to one owned slot. Clones bind the SAME slot, including its seal.
/// Collection data written into it is copied, never shared with the VM arena.
#[derive(Clone, Debug, Default)]
pub struct ResultSink(std::rc::Rc<std::cell::RefCell<SinkState>>);
#[derive(Debug, Default)]
struct SinkState {
    value: OwnedJson,
    contract: super::result_contract::ResultContract,
    sealed: bool,
}
impl ResultSink {
    pub fn new(contract: super::result_contract::ResultContract) -> Self {
        Self(Rc::new(std::cell::RefCell::new(SinkState {
            contract,
            ..Default::default()
        })))
    }
    pub fn reset(&self) -> Result<(), Error> {
        self.writable()?;
        self.0.borrow_mut().value = OwnedJson::Null;
        Ok(())
    }
    pub fn json(&self) -> String {
        let mut out = String::new();
        emit(&self.0.borrow().value, &mut out);
        out
    }
    pub fn sealed(&self) -> bool {
        self.0.borrow().sealed
    }
    pub fn contract(&self) -> super::result_contract::ResultContract {
        self.0.borrow().contract.clone()
    }
    fn writable(&self) -> Result<(), Error> {
        if self.sealed() {
            return Err(Error::host("lmcode return: result sink is sealed"));
        }
        Ok(())
    }
    pub(super) fn seal(&self) -> Result<OwnedJson, Error> {
        self.writable()?;
        let mut state = self.0.borrow_mut();
        state.contract.validate(&state.value, "")?;
        let result = state.value.clone();
        state.sealed = true;
        Ok(result)
    }
}
fn invalid(path: &str, reason: &str) -> Error {
    Error::host(format!(
        "lmcode return: invalid JSON value at {path}: {reason}"
    ))
}
fn text(bytes: &[u8], path: &str) -> Result<String, Error> {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| invalid(path, "utf8"))
}
pub(super) fn pointer(path: &str, key: &str) -> String {
    format!("{}/{}", path, key.replace('~', "~0").replace('/', "~1"))
}
fn integer(n: i64, path: &str) -> Result<OwnedJson, Error> {
    if !(-9007199254740991..=9007199254740991).contains(&n) {
        return Err(invalid(path, "unsafe_integer"));
    }
    Ok(OwnedJson::Number(n.to_string()))
}
fn float(n: f64, path: &str) -> Result<OwnedJson, Error> {
    if !n.is_finite() {
        return Err(invalid(path, "nonfinite"));
    }
    if n.fract() == 0.0 && n.abs() > 9007199254740991.0 {
        return Err(invalid(path, "unsafe_integer"));
    }
    // Debug is Rust's shortest-round-trip formatter, NOT the VM's F2S/str(float).
    Ok(OwnedJson::Float(if n == 0.0 {
        "0".into()
    } else {
        canonical_float(n)
    }))
}
// Keep Rust's shortest-round-trip digits, using the canonical C++ general layout.
fn canonical_float(n: f64) -> String {
    let shortest = format!("{n:?}");
    let mantissa = shortest.split('e').next().unwrap().trim_start_matches('-');
    let joined = mantissa.replace('.', "");
    let precision = joined.trim_start_matches('0').trim_end_matches('0').len();
    // Debug's shortest formatter rounds exact decimal ties upward. The precision
    // formatter uses ties-to-even; prefer it only if it still round trips (the
    // nearest decimal at this precision can lie outside the rounding interval).
    let rounded = format!("{:.*e}", precision - 1, n);
    let raw = if rounded.parse::<f64>().ok() == Some(n) {
        rounded
    } else {
        shortest
    };
    let (sign, raw) = raw
        .strip_prefix('-')
        .map_or(("", raw.as_str()), |v| ("-", v));
    let (mantissa, e) = raw
        .split_once('e')
        .map_or((raw, 0), |(m, e)| (m, e.parse::<i32>().unwrap()));
    let dot = mantissa.find('.').unwrap_or(mantissa.len()) as i32;
    let joined = mantissa.replace('.', "");
    let zeros = joined.len() - joined.trim_start_matches('0').len();
    let digits = joined.trim_start_matches('0').trim_end_matches('0');
    let exponent = e + dot - zeros as i32 - 1;
    if !(-4..6).contains(&exponent) {
        let fraction = if digits.len() > 1 {
            format!(".{}", &digits[1..])
        } else {
            String::new()
        };
        format!(
            "{sign}{}{fraction}e{}{:#02}",
            &digits[..1],
            if exponent < 0 { "-" } else { "+" },
            exponent.abs()
        )
    } else {
        let point = exponent + 1;
        if point <= 0 {
            format!("{sign}0.{}{}", "0".repeat((-point) as usize), digits)
        } else if point as usize >= digits.len() {
            format!(
                "{sign}{digits}{}",
                "0".repeat(point as usize - digits.len())
            )
        } else {
            format!(
                "{sign}{}.{}",
                &digits[..point as usize],
                &digits[point as usize..]
            )
        }
    }
}
fn number_token(raw: &[u8], path: &str) -> Result<OwnedJson, Error> {
    let s = std::str::from_utf8(raw).map_err(|_| invalid(path, "utf8"))?;
    let unsigned = s.strip_prefix('-').unwrap_or(s);
    let (mantissa, exponent) = unsigned.split_once(['e', 'E']).unwrap_or((unsigned, "0"));
    let exponent = exponent.parse::<i64>().unwrap_or_else(|_| {
        if exponent.starts_with('-') {
            i64::MIN
        } else {
            i64::MAX
        }
    });
    let scale = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len() as i64);
    let digits = mantissa.replace('.', "");
    let digits = digits.trim_start_matches('0');
    if !digits.is_empty() {
        let significant = digits.trim_end_matches('0');
        let zeros = (digits.len() - significant.len()) as i64;
        let shift = exponent.saturating_sub(scale).saturating_add(zeros);
        if shift >= 0 {
            let len = (significant.len() as i64).saturating_add(shift);
            if len > 16
                || (len == 16
                    && format!("{}{}", significant, "0".repeat(shift as usize)).as_str()
                        > "9007199254740991")
            {
                return Err(invalid(path, "unsafe_integer"));
            }
        }
    }
    let n = s.parse::<f64>().map_err(|_| invalid(path, "nonfinite"))?;
    if n == 0.0 && !digits.is_empty() {
        return Err(invalid(path, "underflow"));
    }
    if !s.contains(['.', 'e', 'E']) {
        return integer(
            s.parse::<i64>()
                .map_err(|_| invalid(path, "unsafe_integer"))?,
            path,
        );
    }
    float(n, path)
}
pub(super) fn json_node(n: &JsonNode, path: &str) -> Result<OwnedJson, Error> {
    Ok(match n.kind {
        Kind::Null => OwnedJson::Null,
        Kind::Bool => OwnedJson::Bool(n.boolean),
        Kind::Int | Kind::Float => number_token(&n.raw, path)?,
        Kind::String => OwnedJson::String(text(&n.text, path)?),
        Kind::Array => OwnedJson::Array(
            n.array
                .iter()
                .enumerate()
                .map(|(i, v)| json_node(v, &pointer(path, &i.to_string())))
                .collect::<Result<_, _>>()?,
        ),
        Kind::Object => OwnedJson::Object(
            n.object
                .iter()
                .map(|(k, _, v)| {
                    let key = text(k, path)?;
                    let value = json_node(v, &pointer(path, &key))?;
                    Ok((key, value))
                })
                .collect::<Result<_, Error>>()?,
        ),
    })
}
fn convert(ctx: &Context, v: &Value, path: &str) -> Result<OwnedJson, Error> {
    Ok(match v {
        Value::None => OwnedJson::Null,
        Value::Bool(v) => OwnedJson::Bool(*v),
        Value::Int(v) => integer(*v, path)?,
        Value::Float(v) => float(*v, path)?,
        Value::Str(v) => OwnedJson::String(text(v.as_bytes(), path)?),
        Value::Obj { type_name, obj } => {
            macro_rules! list {
                ($ty:ty,$value:expr) => {{
                    let list = ctx.get::<$ty>(*obj).map_err(|_| invalid(path, "handle"))?;
                    OwnedJson::Array(
                        list.items
                            .iter()
                            .enumerate()
                            .map(|(i, v)| $value(v, &pointer(path, &i.to_string())))
                            .collect::<Result<_, Error>>()?,
                    )
                }};
            }
            macro_rules! map {
                ($entries:expr,$value:expr) => {{
                    OwnedJson::Object(
                        $entries
                            .iter()
                            .map(|(k, v)| {
                                let key = text(k.as_bytes(), path)?;
                                let value = $value(v, &pointer(path, &key))?;
                                Ok((key, value))
                            })
                            .collect::<Result<_, Error>>()?,
                    )
                }};
            }
            match type_name.as_str() {
                "Json" => json_node(
                    ctx.get::<Rc<JsonNode>>(*obj)
                        .map_err(|_| invalid(path, "handle"))?,
                    path,
                )?,
                "StrList" => list!(StrList, |v: &crate::ByteString, p: &str| text(
                    v.as_bytes(),
                    p
                )
                .map(OwnedJson::String)),
                "IntList" => list!(IntList, |v: &i64, p: &str| integer(*v, p)),
                "FloatList" => list!(FloatList, |v: &f64, p: &str| float(*v, p)),
                "StrMap" => map!(
                    ctx.get::<StrMap>(*obj)
                        .map_err(|_| invalid(path, "handle"))?
                        .entries(),
                    |v: &crate::ByteString, p: &str| text(v.as_bytes(), p).map(OwnedJson::String)
                ),
                "IntMap" => map!(
                    ctx.get::<IntMap>(*obj)
                        .map_err(|_| invalid(path, "handle"))?
                        .items,
                    |v: &i64, p: &str| integer(*v, p)
                ),
                "FloatMap" => map!(
                    ctx.get::<FloatMap>(*obj)
                        .map_err(|_| invalid(path, "handle"))?
                        .items,
                    |v: &f64, p: &str| float(*v, p)
                ),
                _ => return Err(invalid(path, "handle")),
            }
        }
    })
}
fn validate_keys(value: &OwnedJson, path: &str) -> Result<(), Error> {
    match value {
        OwnedJson::Object(items) => {
            let mut seen = std::collections::HashSet::new();
            for (k, v) in items {
                let child = pointer(path, k);
                if !seen.insert(k) {
                    return Err(invalid(&child, "handle"));
                }
                validate_keys(v, &child)?;
            }
        }
        OwnedJson::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                validate_keys(v, &pointer(path, &i.to_string()))?;
            }
        }
        _ => {}
    }
    Ok(())
}
/// Validate then atomically replace; a failed write leaves the previous value.
pub fn set_result(ctx: &mut Context, value: &Value) -> Result<(), Error> {
    if ctx.result_sink.is_none() {
        return Err(Error::host("lmcode return: no result sink"));
    }
    let sink = ctx.result_sink.as_ref().unwrap();
    sink.writable()?;
    let owned = convert(ctx, value, "")?;
    validate_keys(&owned, "")?;
    let mut state = sink.0.borrow_mut();
    state.contract.validate_source(value, None)?;
    state.contract.validate(&owned, "")?;
    state.value = owned;
    Ok(())
}
pub fn put_result(ctx: &mut Context, key: &[u8], value: &Value) -> Result<(), Error> {
    let sink = ctx
        .result_sink
        .as_ref()
        .ok_or_else(|| Error::host("lmcode return: no result sink"))?;
    sink.writable()?;
    if !matches!(
        sink.0.borrow().value,
        OwnedJson::Null | OwnedJson::Object(_)
    ) {
        return Err(Error::host(
            "lmcode return: field assignment requires object or null",
        ));
    }
    let key = text(key, "")?;
    let owned = convert(ctx, value, &pointer("", &key))?;
    validate_keys(&owned, &pointer("", &key))?;
    let mut state = sink.0.borrow_mut();
    state.contract.validate_source(value, Some(&key))?;
    let mut next = state.value.clone();
    if matches!(next, OwnedJson::Null) {
        next = OwnedJson::Object(Vec::new());
    }
    if let OwnedJson::Object(items) = &mut next {
        if let Some((_, old)) = items.iter_mut().find(|(k, _)| k == &key) {
            *old = owned;
        } else {
            items.push((key, owned));
        }
    }
    state.contract.validate(&next, "")?;
    state.value = next;
    Ok(())
}
pub fn register_result(registry: &mut Registry) -> Result<&mut Registry, Error> {
    registry.begin_compatibility_registration();
    for ty in [
        "int",
        "float",
        "bool",
        "string",
        "Json",
        "StrList",
        "IntList",
        "FloatList",
        "StrMap",
        "IntMap",
        "FloatMap",
    ] {
        registry.function("lmcode_return_set", &[ty], "void", |ctx, args| {
            set_result(ctx, &args[0])?;
            Ok(Value::None)
        })?;
        registry.function("lmcode_return_put", &["string", ty], "void", |ctx, args| {
            put_result(ctx, args[0].as_bytes()?, &args[1])?;
            Ok(Value::None)
        })?;
    }
    registry.function("lmcode_return_null", &[], "void", |ctx, _| {
        set_result(ctx, &Value::None)?;
        Ok(Value::None)
    })?;
    registry.function(
        "lmcode_return_put_null",
        &["string"],
        "void",
        |ctx, args| {
            put_result(ctx, args[0].as_bytes()?, &Value::None)?;
            Ok(Value::None)
        },
    )?;
    registry.end_compatibility_registration();
    Ok(registry)
}
fn quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\u{20}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
pub(super) fn emit(value: &OwnedJson, out: &mut String) {
    match value {
        OwnedJson::Null => out.push_str("null"),
        OwnedJson::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        OwnedJson::Number(v) | OwnedJson::Float(v) => out.push_str(v),
        OwnedJson::String(v) => quote(v, out),
        OwnedJson::Array(items) => {
            out.push('[');
            for (i, v) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                emit(v, out);
            }
            out.push(']');
        }
        OwnedJson::Object(items) => {
            out.push('{');
            for (i, (k, v)) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                quote(k, out);
                out.push(':');
                emit(v, out);
            }
            out.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn call(ctx: &mut Context, name: &str, params: &str, args: &[Value]) -> Result<Value, Error> {
        let reg = Registry::new();
        let key = format!(
            "fn {name}({params}) -> {}",
            if name == "json_parse" { "Json" } else { "void" }
        );
        (reg.function_by_signature(&key).unwrap().2)(ctx, args)
    }
    fn context() -> Context {
        let mut ctx = Context::new();
        ctx.result_sink = Some(ResultSink::default());
        ctx
    }
    #[test]
    fn json_shape_contract_vs_known_float() {
        let mut ctx = context();
        ctx.result_sink = Some(ResultSink::new(
            super::super::ResultContract::from_schema(
                br#"{"dsl.schema":2,"result":{"type":"int"}}"#,
            )
            .unwrap(),
        ));
        assert!(set_result(&mut ctx, &Value::Float(2.0)).is_err());
        let v = call(&mut ctx, "json_parse", "string", &[Value::string("2.0")]).unwrap();
        set_result(&mut ctx, &v).unwrap();
        assert_eq!(ctx.result_sink.as_ref().unwrap().json(), "2");
    }
    #[test]
    fn raw_json_tokens_and_pointer_errors() {
        let mut ctx = context();
        for token in [
            "9007199254740991",
            "-9007199254740991",
            "9007199254740991.0",
            "90071992547409910e-1",
            "5e-324",
            "-0.0",
            "0e-999",
        ] {
            let v = call(&mut ctx, "json_parse", "string", &[Value::string(token)]).unwrap();
            set_result(&mut ctx, &v).unwrap();
        }
        for (token, reason) in [
            ("9007199254740992", "unsafe_integer"),
            ("9007199254740993", "unsafe_integer"),
            ("9.007199254740992e15", "unsafe_integer"),
            ("9007199254740991.9", "unsafe_integer"),
            ("1e-999", "underflow"),
        ] {
            let raw = format!("{{\"a/b~c\":[{token}]}}");
            let v = call(&mut ctx, "json_parse", "string", &[Value::string(raw)]).unwrap();
            let error = set_result(&mut ctx, &v).unwrap_err();
            assert_eq!(error.code, "RUN_HOST");
            assert_eq!(
                error.message,
                format!("lmcode return: invalid JSON value at /a~1b~0c/0: {reason}")
            );
        }
        assert!(call(
            &mut ctx,
            "json_parse",
            "string",
            &[Value::string("{\"x\":1,\"x\":2}")]
        )
        .is_err());
        assert_eq!(
            number_token(b"0e9999999999999999999999", "")
                .map(|v| {
                    let mut s = String::new();
                    emit(&v, &mut s);
                    s
                })
                .unwrap(),
            "0"
        );
        assert!(number_token(b"1e9999999999999999999999", "")
            .unwrap_err()
            .message
            .ends_with("unsafe_integer"));
    }
    #[test]
    fn copy_survives_mutation_and_arena_teardown() {
        let mut ctx = context();
        let id = ctx
            .insert_collection("IntList", IntList { items: vec![1, 2] })
            .unwrap();
        set_result(&mut ctx, &Value::object("IntList", id)).unwrap();
        ctx.get_mut::<IntList>(id).unwrap().items[0] = 9;
        ctx.clear_collections();
        assert_eq!(ctx.result_sink.as_ref().unwrap().json(), "[1,2]");
        let v = call(
            &mut ctx,
            "json_parse",
            "string",
            &[Value::string("{\"a\":[\"owned\",true,null]}")],
        )
        .unwrap();
        set_result(&mut ctx, &v).unwrap();
        ctx.clear_collections();
        assert_eq!(
            ctx.result_sink.as_ref().unwrap().json(),
            "{\"a\":[\"owned\",true,null]}"
        );
    }
    #[test]
    fn invalid_handles_keys_and_numbers_are_atomic() {
        let mut ctx = context();
        set_result(&mut ctx, &Value::None).unwrap();
        put_result(&mut ctx, b"ok", &Value::Int(1)).unwrap();
        for (key, value, reason) in [
            (vec![255], Value::None, "utf8"),
            (b"bad".to_vec(), Value::Float(f64::NAN), "nonfinite"),
            (b"bad".to_vec(), Value::Int(i64::MAX), "unsafe_integer"),
        ] {
            assert!(put_result(&mut ctx, &key, &value)
                .unwrap_err()
                .message
                .ends_with(reason));
            assert_eq!(ctx.result_sink.as_ref().unwrap().json(), "{\"ok\":1}");
        }
        let handle = ctx.insert("Host", 7);
        assert!(set_result(&mut ctx, &Value::object("Host", handle))
            .unwrap_err()
            .message
            .ends_with("handle"));
        assert!(set_result(&mut ctx, &Value::object("Json", handle))
            .unwrap_err()
            .message
            .ends_with("handle"));
        let mut map = IntMap::default();
        map.items = vec![("x".into(), 1), ("x".into(), 2)];
        let id = ctx.insert("IntMap", map);
        assert_eq!(
            set_result(&mut ctx, &Value::object("IntMap", id))
                .unwrap_err()
                .message,
            "lmcode return: invalid JSON value at /x: handle"
        );
    }
}
