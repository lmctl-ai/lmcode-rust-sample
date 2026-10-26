//! Immutable JSON values. Parsing keeps the original token spelling so `to_json` is
//! canonical compact JSON while preserving numeric and escaped-string tokens.
use super::collections::CollectionList;
use crate::{Context, Error, Registry, Value};
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct JsonNode {
    kind: Kind,
    raw: Vec<u8>,
    text: Vec<u8>,
    integer: i64,
    float: f64,
    boolean: bool,
    array: Vec<Rc<JsonNode>>,
    object: Vec<(Vec<u8>, Vec<u8>, Rc<JsonNode>)>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Null,
    Bool,
    Int,
    Float,
    String,
    Array,
    Object,
}
impl JsonNode {
    fn empty(kind: Kind) -> Self {
        Self {
            kind,
            raw: Vec::new(),
            text: Vec::new(),
            integer: 0,
            float: 0.0,
            boolean: false,
            array: Vec::new(),
            object: Vec::new(),
        }
    }
}

struct Parser<'a> {
    s: &'a [u8],
    p: usize,
}
impl<'a> Parser<'a> {
    fn fail<T>(&self, msg: &str, at: usize) -> Result<T, Error> {
        Err(Error::host(format!("json_parse: {} at byte {}", msg, at)))
    }
    fn ws(&mut self) {
        while self.p < self.s.len() && matches!(self.s[self.p], b' ' | b'\t' | b'\r' | b'\n') {
            self.p += 1;
        }
    }
    fn take(&mut self, c: u8) -> bool {
        if self.p < self.s.len() && self.s[self.p] == c {
            self.p += 1;
            true
        } else {
            false
        }
    }
    fn literal(&mut self, lit: &[u8]) -> Result<(), Error> {
        let at = self.p;
        if self.s.get(self.p..self.p + lit.len()) != Some(lit) {
            return self.fail("invalid literal", at);
        }
        self.p += lit.len();
        Ok(())
    }
    fn hex4(&self, at: usize) -> Result<u32, Error> {
        if at + 4 > self.s.len() {
            return self.fail("invalid Unicode escape", at);
        }
        let mut n = 0u32;
        for (i, &c) in self.s[at..at + 4].iter().enumerate() {
            n = (n << 4)
                | match c {
                    b'0'..=b'9' => (c - b'0') as u32,
                    b'a'..=b'f' => (c - b'a' + 10) as u32,
                    b'A'..=b'F' => (c - b'A' + 10) as u32,
                    _ => return self.fail("invalid Unicode escape", at + i),
                };
        }
        Ok(n)
    }
    fn utf8(out: &mut Vec<u8>, c: u32) {
        if c < 0x80 {
            out.push(c as u8);
        } else if c < 0x800 {
            out.extend([0xc0 | (c >> 6) as u8, 0x80 | (c & 63) as u8]);
        } else if c < 0x10000 {
            out.extend([
                0xe0 | (c >> 12) as u8,
                0x80 | ((c >> 6) & 63) as u8,
                0x80 | (c & 63) as u8,
            ]);
        } else {
            out.extend([
                0xf0 | (c >> 18) as u8,
                0x80 | ((c >> 12) & 63) as u8,
                0x80 | ((c >> 6) & 63) as u8,
                0x80 | (c & 63) as u8,
            ]);
        }
    }
    fn string(&mut self) -> Result<Vec<u8>, Error> {
        let start = self.p;
        if !self.take(b'"') {
            return self.fail("expected string", start);
        }
        let mut out = Vec::new();
        while self.p < self.s.len() {
            let c = self.s[self.p];
            self.p += 1;
            if c == b'"' {
                return Ok(out);
            }
            if c < 0x20 {
                return self.fail("control byte in string", self.p - 1);
            }
            if c != b'\\' {
                out.push(c);
                continue;
            }
            if self.p >= self.s.len() {
                return self.fail("unterminated escape", self.p);
            }
            let e = self.s[self.p];
            self.p += 1;
            match e {
                b'"' | b'\\' | b'/' => out.push(e),
                b'b' => out.push(8),
                b'f' => out.push(12),
                b'n' => out.push(10),
                b'r' => out.push(13),
                b't' => out.push(9),
                b'u' => {
                    let at = self.p;
                    let mut cp = self.hex4(at)?;
                    self.p += 4;
                    if (0xd800..=0xdbff).contains(&cp) {
                        if self.p + 6 > self.s.len()
                            || self.s[self.p] != b'\\'
                            || self.s[self.p + 1] != b'u'
                        {
                            return self.fail("unpaired high surrogate", self.p);
                        }
                        let lo = self.hex4(self.p + 2)?;
                        if !(0xdc00..=0xdfff).contains(&lo) {
                            return self.fail("unpaired high surrogate", self.p);
                        }
                        self.p += 6;
                        cp = 0x10000 + ((cp - 0xd800) << 10) + (lo - 0xdc00);
                    } else if (0xdc00..=0xdfff).contains(&cp) {
                        return self.fail("unpaired low surrogate", at);
                    }
                    Self::utf8(&mut out, cp);
                }
                _ => return self.fail("invalid escape", self.p - 1),
            }
        }
        self.fail("unterminated string", self.p)
    }
    fn number(&mut self) -> Result<JsonNode, Error> {
        let st = self.p;
        if self.take(b'-') && self.p >= self.s.len() {
            return self.fail("invalid number", self.p);
        }
        if self.p >= self.s.len() || !self.s[self.p].is_ascii_digit() {
            return self.fail("invalid number", self.p);
        }
        if self.s[self.p] == b'0' {
            self.p += 1;
        } else {
            while self.p < self.s.len() && self.s[self.p].is_ascii_digit() {
                self.p += 1;
            }
        }
        let mut fl = false;
        if self.take(b'.') {
            fl = true;
            if self.p >= self.s.len() || !self.s[self.p].is_ascii_digit() {
                return self.fail("invalid number", self.p);
            }
            while self.p < self.s.len() && self.s[self.p].is_ascii_digit() {
                self.p += 1;
            }
        }
        if self.p < self.s.len() && (self.s[self.p] == b'e' || self.s[self.p] == b'E') {
            fl = true;
            self.p += 1;
            if self.p < self.s.len() && (self.s[self.p] == b'+' || self.s[self.p] == b'-') {
                self.p += 1;
            }
            if self.p >= self.s.len() || !self.s[self.p].is_ascii_digit() {
                return self.fail("invalid number", self.p);
            }
            while self.p < self.s.len() && self.s[self.p].is_ascii_digit() {
                self.p += 1;
            }
        }
        let raw = self.s[st..self.p].to_vec();
        let mut n = JsonNode::empty(if fl { Kind::Float } else { Kind::Int });
        n.raw = raw.clone();
        if fl {
            let x = std::str::from_utf8(&raw).unwrap().parse::<f64>().ok();
            if x.is_none() || !x.unwrap().is_finite() {
                return self.fail("nonfinite number", st);
            }
            n.float = x.unwrap();
        } else {
            let x = std::str::from_utf8(&raw).unwrap().parse::<i64>();
            if x.is_err() {
                return self.fail("integer out of range", st);
            }
            n.integer = x.unwrap();
        }
        Ok(n)
    }
    fn value(&mut self, depth: usize) -> Result<JsonNode, Error> {
        self.ws();
        let st = self.p;
        if self.p >= self.s.len() {
            return self.fail("expected value", self.p);
        }
        let mut n = match self.s[self.p] {
            b'n' => {
                self.literal(b"null")?;
                JsonNode::empty(Kind::Null)
            }
            b't' => {
                self.literal(b"true")?;
                let mut x = JsonNode::empty(Kind::Bool);
                x.boolean = true;
                x
            }
            b'f' => {
                self.literal(b"false")?;
                JsonNode::empty(Kind::Bool)
            }
            b'"' => {
                let text = self.string()?;
                let mut x = JsonNode::empty(Kind::String);
                x.text = text;
                x
            }
            b'[' => {
                if depth + 1 > 256 {
                    return self.fail("maximum depth exceeded", self.p);
                }
                self.p += 1;
                let mut x = JsonNode::empty(Kind::Array);
                self.ws();
                if !self.take(b']') {
                    loop {
                        x.array.push(Rc::new(self.value(depth + 1)?));
                        self.ws();
                        if self.take(b']') {
                            break;
                        }
                        if !self.take(b',') {
                            return self.fail("expected comma", self.p);
                        }
                        self.ws();
                    }
                }
                x
            }
            b'{' => {
                if depth + 1 > 256 {
                    return self.fail("maximum depth exceeded", self.p);
                }
                self.p += 1;
                let mut x = JsonNode::empty(Kind::Object);
                let mut keys = std::collections::HashSet::new();
                self.ws();
                if !self.take(b'}') {
                    loop {
                        self.ws();
                        let kp = self.p;
                        if self.p >= self.s.len() || self.s[self.p] != b'"' {
                            return self.fail("object key must be a string", self.p);
                        }
                        let raw_key_start = self.p;
                        let key = self.string()?;
                        if !keys.insert(key.clone()) {
                            return self.fail("duplicate object key", kp);
                        }
                        let raw_key = self.s[raw_key_start..self.p].to_vec();
                        self.ws();
                        if !self.take(b':') {
                            return self.fail("expected colon", self.p);
                        }
                        x.object
                            .push((key, raw_key, Rc::new(self.value(depth + 1)?)));
                        self.ws();
                        if self.take(b'}') {
                            break;
                        }
                        if !self.take(b',') {
                            return self.fail("expected comma", self.p);
                        }
                        self.ws();
                    }
                }
                x
            }
            b'-' | b'0'..=b'9' => self.number()?,
            _ => return self.fail("unexpected value", self.p),
        };
        if matches!(n.kind, Kind::String) {
            n.raw = self.s[st..self.p].to_vec();
        }
        Ok(n)
    }
}

fn emit(n: &JsonNode, out: &mut Vec<u8>) {
    match n.kind {
        Kind::Null => out.extend(b"null"),
        Kind::Bool => out.extend(if n.boolean {
            &b"true"[..]
        } else {
            &b"false"[..]
        }),
        Kind::Int | Kind::Float | Kind::String => out.extend(&n.raw),
        Kind::Array => {
            out.push(b'[');
            for (i, x) in n.array.iter().enumerate() {
                if i > 0 {
                    out.push(b',')
                }
                emit(x.as_ref(), out)
            }
            out.push(b']')
        }
        Kind::Object => {
            out.push(b'{');
            for (i, (_k, r, x)) in n.object.iter().enumerate() {
                if i > 0 {
                    out.push(b',')
                }
                out.extend(r);
                out.push(b':');
                emit(x.as_ref(), out)
            }
            out.push(b'}')
        }
    }
}
fn kind(n: &JsonNode) -> &'static str {
    match n.kind {
        Kind::Null => "null",
        Kind::Bool => "bool",
        Kind::Int => "int",
        Kind::Float => "float",
        Kind::String => "string",
        Kind::Array => "array",
        Kind::Object => "object",
    }
}
fn borrowed(
    ctx: &mut Context,
    type_name: &'static str,
    node: Rc<JsonNode>,
) -> Result<Value, Error> {
    Ok(Value::object(
        type_name,
        ctx.insert_borrowed_collection(type_name, node),
    ))
}

/// Register JSON parsing and immutable navigation. Roots consume one arena slot;
/// navigation shares immutable nodes through reference-counted handles.
pub fn register_json(r: &mut Registry) -> Result<&mut Registry, Error> {
    r.begin_compatibility_registration();
    r.function("json_parse", &["string"], "Json", |ctx, args| {
        let b = args[0].as_bytes()?;
        std::str::from_utf8(b).map_err(|e| {
            Error::host(format!(
                "json_parse: invalid UTF-8 at byte {}",
                e.valid_up_to()
            ))
        })?;
        if b.starts_with(&[0xef, 0xbb, 0xbf]) {
            return Err(Error::host("json_parse: BOM is not allowed at byte 0"));
        }
        let mut p = Parser { s: b, p: 0 };
        let n = p.value(0)?;
        p.ws();
        if p.p != b.len() {
            return Err(Error::host(format!(
                "json_parse: trailing input at byte {}",
                p.p
            )));
        }
        Ok(Value::object("Json", ctx.insert_collection("Json", Rc::new(n))?))
    })?;
    r.method("Json", "kind", &[], "string", |ctx, t, _| {
        Ok(Value::string(kind(ctx.get::<Rc<JsonNode>>(t)?)))
    })?;
    r.method("Json", "len", &[], "int", |ctx, t, _| {
        let n = ctx.get::<Rc<JsonNode>>(t)?;
        match n.kind {
            Kind::Array => Ok(Value::Int(n.array.len() as i64)),
            Kind::Object => Ok(Value::Int(n.object.len() as i64)),
            _ => Err(Error::host("Json.len: expected array or object")),
        }
    })?;
    r.method("Json", "to_json", &[], "string", |ctx, t, _| {
        let mut o = Vec::new();
        emit(ctx.get::<Rc<JsonNode>>(t)?, &mut o);
        Ok(Value::string(o))
    })?;
    r.method("Json", "as_string", &[], "string", |ctx, t, _| {
        let n = ctx.get::<Rc<JsonNode>>(t)?;
        if n.kind == Kind::String {
            Ok(Value::string(n.text.clone()))
        } else {
            Err(Error::host("Json.as_string: expected string"))
        }
    })?;
    r.method("Json", "as_int", &[], "int", |ctx, t, _| {
        let n = ctx.get::<Rc<JsonNode>>(t)?;
        if n.kind == Kind::Int {
            Ok(Value::Int(n.integer))
        } else {
            Err(Error::host("Json.as_int: expected int"))
        }
    })?;
    r.method("Json", "as_float", &[], "float", |ctx, t, _| {
        let n = ctx.get::<Rc<JsonNode>>(t)?;
        match n.kind {
            Kind::Float => Ok(Value::Float(n.float)),
            Kind::Int => Ok(Value::Float(n.integer as f64)),
            _ => Err(Error::host("Json.as_float: expected number")),
        }
    })?;
    r.method("Json", "as_bool", &[], "bool", |ctx, t, _| {
        let n = ctx.get::<Rc<JsonNode>>(t)?;
        if n.kind == Kind::Bool {
            Ok(Value::Bool(n.boolean))
        } else {
            Err(Error::host("Json.as_bool: expected bool"))
        }
    })?;
    r.method("Json", "has", &["string"], "bool", |ctx, t, a| {
        let n = ctx.get::<Rc<JsonNode>>(t)?;
        if n.kind != Kind::Object {
            return Err(Error::host("Json.has: expected object"));
        }
        let key = a[0].as_bytes()?;
        Ok(Value::Bool(n.object.iter().any(|(k, _, _)| k == key)))
    })?;
    r.method("Json", "get", &["string"], "Json", |ctx, t, a| {
        let n = ctx.get::<Rc<JsonNode>>(t)?;
        if n.kind != Kind::Object {
            return Err(Error::host("Json.get: expected object"));
        }
        let key = a[0].as_bytes()?;
        let x = n
            .object
            .iter()
            .find(|(k, _, _)| k == key)
            .map(|x| x.2.clone())
            .ok_or_else(|| Error::host("Json.get: missing key"))?;
        borrowed(ctx, "Json", x)
    })?;
    r.method("Json", "at", &["int"], "Json", |ctx, t, a| {
        let n = ctx.get::<Rc<JsonNode>>(t)?;
        if n.kind != Kind::Array {
            return Err(Error::host("Json.at: expected array"));
        }
        let i = a[0].as_int()?;
        if i < 0 || i as usize >= n.array.len() {
            return Err(Error::host(format!("Json.at: index out of range: {}", i)));
        }
        borrowed(ctx, "Json", n.array[i as usize].clone())
    })?;
    r.method("Json", "keys", &[], "StrList", |ctx, t, _| {
        let n = ctx.get::<Rc<JsonNode>>(t)?;
        if n.kind != Kind::Object {
            return Err(Error::host("Json.keys: expected object"));
        }
        let items = n
            .object
            .iter()
            .map(|(k, _, _)| crate::ByteString::from(k.as_slice()))
            .collect();
        Ok(Value::object(
            "StrList",
            ctx.insert_collection("StrList", CollectionList { items })?,
        ))
    })?;
    r.end_compatibility_registration();
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_unicode_reports_the_failing_hex_byte() {
        let mut p = Parser {
            s: br#""\u00Z0""#,
            p: 0,
        };
        let error = p.value(0).unwrap_err();
        assert_eq!(
            error.message,
            "json_parse: invalid Unicode escape at byte 5"
        );
    }

    #[test]
    fn navigation_nodes_are_shared_and_do_not_clone_subtrees() {
        let mut p = Parser {
            s: br#"[1,{"x":[2]}]"#,
            p: 0,
        };
        let root = Rc::new(p.value(0).unwrap());
        let object = root.array[1].clone();
        let nested = object.object[0].2.clone();
        assert!(Rc::ptr_eq(&root.array[1], &object));
        assert_eq!(nested.array[0].integer, 2);
    }
    #[test]
    fn navigation_does_not_consume_the_shared_object_quota() {
        let r = Registry::new();
        let mut ctx = Context::new();
        let parse = &r.functions["fn json_parse(string) -> Json"].2;
        let at = &r.methods["method Json.at(int) -> Json"].3;
        let root = parse(&mut ctx, &[Value::string("[null]")])
            .unwrap()
            .as_obj()
            .unwrap();
        for _ in 0..65_537 {
            at(&mut ctx, root, &[Value::Int(0)]).unwrap();
        }
        assert_eq!(ctx.collection_objects, 1);
        // Parsing failures consume no slot, even after successful navigation.
        assert!(parse(&mut ctx, &[Value::string("[")]).is_err());
        assert_eq!(ctx.collection_objects, 1);
        ctx.clear_collections();
        assert_eq!(ctx.collection_objects, 0);
        assert!(ctx.objects.iter().all(Option::is_none));
    }
}
