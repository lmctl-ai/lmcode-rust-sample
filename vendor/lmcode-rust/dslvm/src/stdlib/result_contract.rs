//! Optional host-owned result metadata. Invocation metadata has no type authority.
use super::{
    json::{parse_document, JsonNode, Kind},
    result::{pointer, OwnedJson},
};
use crate::{Error, Value};
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResultContract {
    kind: String,
    nullable: bool,
}
pub(super) fn member<'a>(n: &'a JsonNode, key: &str) -> Option<&'a JsonNode> {
    n.object
        .iter()
        .find(|(k, _, _)| k == key.as_bytes())
        .map(|(_, _, v)| v.as_ref())
}
pub(super) fn node_integer(n: &JsonNode) -> Option<i64> {
    let v = match n.kind {
        Kind::Int => n.integer as f64,
        Kind::Float => n.float,
        _ => return None,
    };
    (v.is_finite() && v.fract() == 0.0 && v.abs() <= 9007199254740991.0).then_some(v as i64)
}
fn mismatch(path: &str, want: &str, got: &str) -> Error {
    Error::host(format!(
        "lmcode return: result type mismatch at {path}: expected {want}; got {got}"
    ))
}
fn kind(n: &OwnedJson) -> &str {
    match n {
        OwnedJson::Null => "null",
        OwnedJson::Bool(_) => "bool",
        OwnedJson::Number(_) => "int",
        OwnedJson::Float(_) => "float",
        OwnedJson::String(_) => "string",
        OwnedJson::Array(_) => "array",
        OwnedJson::Object(_) => "object",
    }
}
fn primitive(n: &OwnedJson, t: &str) -> bool {
    match (n, t) {
        (OwnedJson::Number(s) | OwnedJson::Float(s), "int") => {
            s.parse::<f64>().unwrap().fract() == 0.0
        }
        (OwnedJson::Number(_) | OwnedJson::Float(_), "float")
        | (OwnedJson::Bool(_), "bool")
        | (OwnedJson::String(_), "string") => true,
        _ => false,
    }
}
impl ResultContract {
    pub fn from_schema(raw: &[u8]) -> Result<Self, Error> {
        let parse = || -> Result<Self, Error> {
            let root = parse_document(raw)?;
            if root.kind != Kind::Object {
                return Err(Error::host("schema must be an object"));
            }
            if member(&root, "dsl.schema").and_then(node_integer) != Some(2) {
                return Err(Error::host("requires schema 2"));
            }
            let Some(r) = member(&root, "result") else {
                return Ok(Self::default());
            };
            if r.kind != Kind::Object {
                return Err(Error::host("result must be an object"));
            }
            for (k, _, _) in &r.object {
                if k != b"type" && k != b"nullable" {
                    return Err(Error::host(format!(
                        "unknown key {}",
                        String::from_utf8_lossy(k)
                    )));
                }
            }
            let t = member(r, "type")
                .filter(|n| n.kind == Kind::String)
                .ok_or_else(|| Error::host("type must be a string"))?;
            let kind = String::from_utf8(t.text.clone()).unwrap();
            if ![
                "null",
                "bool",
                "int",
                "float",
                "string",
                "Json",
                "StrList",
                "IntList",
                "FloatList",
                "StrMap",
                "IntMap",
                "FloatMap",
            ]
            .contains(&kind.as_str())
            {
                return Err(Error::host(format!("unknown type {kind}")));
            }
            let mut nullable = false;
            if let Some(n) = member(r, "nullable") {
                if kind == "null" {
                    return Err(Error::host("type null forbids nullable"));
                }
                if n.kind != Kind::Bool {
                    return Err(Error::host("nullable must be bool"));
                }
                nullable = n.boolean;
            }
            Ok(Self { kind, nullable })
        };
        parse().map_err(|e| {
            Error::new(
                "LOAD_BAD_SCHEMA",
                format!("lmcode result contract: {}", e.message),
            )
        })
    }
    pub fn result_type(&self) -> &str {
        &self.kind
    }
    pub fn nullable(&self) -> bool {
        self.nullable
    }
    pub(super) fn validate(&self, n: &OwnedJson, path: &str) -> Result<(), Error> {
        let t = self.kind.as_str();
        if t.is_empty() {
            return Ok(());
        }
        if matches!(n, OwnedJson::Null) {
            return if self.nullable || t == "null" {
                Ok(())
            } else {
                Err(mismatch(path, t, "null"))
            };
        }
        if t == "Json" || primitive(n, t) {
            return Ok(());
        }
        let elem = if t.starts_with("Str") {
            "string"
        } else if t.starts_with("Int") {
            "int"
        } else {
            "float"
        };
        match n {
            OwnedJson::Array(items) if ["StrList", "IntList", "FloatList"].contains(&t) => {
                for (i, v) in items.iter().enumerate() {
                    if !primitive(v, elem) {
                        return Err(mismatch(&pointer(path, &i.to_string()), elem, kind(v)));
                    }
                }
                Ok(())
            }
            OwnedJson::Object(items) if ["StrMap", "IntMap", "FloatMap"].contains(&t) => {
                for (k, v) in items {
                    if !primitive(v, elem) {
                        return Err(mismatch(&pointer(path, k), elem, kind(v)));
                    }
                }
                Ok(())
            }
            _ => Err(mismatch(path, t, kind(n))),
        }
    }
    pub(super) fn validate_source(&self, v: &Value, key: Option<&str>) -> Result<(), Error> {
        if self.kind.is_empty()
            || self.kind == "Json"
            || matches!(v,Value::Obj{type_name,..}if type_name=="Json")
        {
            return Ok(());
        }
        let actual = match v {
            Value::None => "null",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "string",
            Value::Obj { type_name, .. } => type_name,
        };
        let mut expected = self.kind.as_str();
        let mut path = String::new();
        if let Some(key) = key {
            expected = match expected {
                "IntMap" => "int",
                "FloatMap" => "float",
                "StrMap" => "string",
                _ => return Ok(()),
            };
            path = pointer("", key)
        } else if matches!(v, Value::None) {
            return Ok(());
        }
        if actual == expected
            || matches!(
                (actual, expected),
                ("int", "float") | ("IntList", "FloatList") | ("IntMap", "FloatMap")
            )
        {
            Ok(())
        } else {
            Err(mismatch(&path, expected, actual))
        }
    }
}
