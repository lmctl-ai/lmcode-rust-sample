//! Immutable JSON values. Parsing keeps the original token spelling so `to_json` is
//! canonical compact JSON while preserving numeric and escaped-string tokens.
use super::collections::CollectionList;
use crate::{Context, Error, Registry, Value};
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct JsonNode {
    pub(super) kind: Kind,
    pub(super) raw: Vec<u8>,
    pub(super) text: Vec<u8>,
    pub(super) integer: i64,
    pub(super) float: f64,
    pub(super) boolean: bool,
    pub(super) array: Vec<Rc<JsonNode>>,
    pub(super) object: Vec<(Vec<u8>, Vec<u8>, Rc<JsonNode>)>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
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

#[derive(Default)]
struct TemplateNode {
    text: Vec<u8>,
    path: Vec<u8>,
    item: Vec<u8>,
    separator: Vec<u8>,
    conditional: bool,
    body: Vec<TemplateNode>,
    otherwise: Vec<TemplateNode>,
}
fn template_ident(s: &[u8], mut i: usize) -> Option<usize> {
    if !s
        .get(i)
        .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
    {
        return None;
    }
    i += 1;
    while s
        .get(i)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        i += 1;
    }
    Some(i)
}
fn template_path_end(s: &[u8], start: usize) -> Option<usize> {
    let mut i = template_ident(s, start)?;
    loop {
        match s.get(i) {
            Some(b'.') => {
                i = template_ident(s, i + 1)?;
            }
            Some(b'[') => {
                i += 1;
                let begin = i;
                while s.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
                if i == begin || s.get(i) != Some(&b']') {
                    return None;
                }
                i += 1;
            }
            _ => return Some(i),
        }
    }
}
fn template_error(s: &str) -> Error {
    Error::host(format!("template_render: {}", s))
}
fn flush_template(nodes: &mut Vec<TemplateNode>, literal: &mut Vec<u8>) {
    if !literal.is_empty() {
        nodes.push(TemplateNode {
            text: std::mem::take(literal),
            ..Default::default()
        });
    }
}
fn parse_template(
    s: &[u8],
    pos: &mut usize,
    mode: &str,
    depth: usize,
) -> Result<(Vec<TemplateNode>, Vec<u8>), Error> {
    if depth > 256 {
        return Err(template_error("maximum nesting exceeded"));
    }
    let mut nodes = Vec::new();
    let mut literal = Vec::new();
    while *pos < s.len() {
        let rest = &s[*pos..];
        if mode == "loop" && rest.starts_with(b"}>") {
            flush_template(&mut nodes, &mut literal);
            *pos += 2;
            return Ok((nodes, Vec::new()));
        }
        if mode == "loop" && rest.starts_with(b"};") {
            let mut i = *pos + 2;
            while s.get(i) == Some(&b' ') {
                i += 1;
            }
            if !s[i..].starts_with(b"separator") {
                return Err(template_error("invalid loop separator"));
            }
            i += 9;
            while s.get(i) == Some(&b' ') {
                i += 1;
            }
            if s.get(i) != Some(&b'=') {
                return Err(template_error("invalid loop separator"));
            }
            i += 1;
            while s.get(i) == Some(&b' ') {
                i += 1;
            }
            let mut parser = Parser { s, p: i };
            let separator = parser
                .string()
                .map_err(|_| template_error("invalid loop separator"))?;
            std::str::from_utf8(&separator)
                .map_err(|_| template_error("invalid loop separator"))?;
            if s.get(parser.p) != Some(&b'>') {
                return Err(template_error("invalid loop separator"));
            }
            *pos = parser.p + 1;
            flush_template(&mut nodes, &mut literal);
            return Ok((nodes, separator));
        }
        if rest.starts_with(b"<<") {
            literal.push(b'<');
            *pos += 2;
            continue;
        }
        if rest.starts_with(b"<else>") || rest.starts_with(b"<endif>") {
            let tag = if rest.starts_with(b"<else>") {
                "else"
            } else {
                "endif"
            };
            if (mode != "if" && mode != "else") || (mode == "else" && tag == "else") {
                return Err(template_error("unexpected conditional delimiter"));
            }
            flush_template(&mut nodes, &mut literal);
            *pos += if tag == "else" { 6 } else { 7 };
            return Ok((nodes, tag.as_bytes().to_vec()));
        }
        if rest.starts_with(b"<if(") {
            if let Some(end) = template_path_end(s, *pos + 4) {
                if s[end..].starts_with(b")>") {
                    let path = s[*pos + 4..end].to_vec();
                    *pos = end + 2;
                    flush_template(&mut nodes, &mut literal);
                    let (body, tag) = parse_template(s, pos, "if", depth + 1)?;
                    let otherwise = if tag == b"else" {
                        parse_template(s, pos, "else", depth + 1)?.0
                    } else {
                        Vec::new()
                    };
                    nodes.push(TemplateNode {
                        path,
                        conditional: true,
                        body,
                        otherwise,
                        ..Default::default()
                    });
                    continue;
                }
            }
        }
        if rest[0] == b'<' {
            if let Some(end) = template_path_end(s, *pos + 1) {
                let path = s[*pos + 1..end].to_vec();
                if s.get(end) == Some(&b'>') {
                    flush_template(&mut nodes, &mut literal);
                    nodes.push(TemplateNode {
                        path,
                        ..Default::default()
                    });
                    *pos = end + 1;
                    continue;
                }
                if s[end..].starts_with(b":{") {
                    if let Some(item_end) = template_ident(s, end + 2) {
                        if s.get(item_end) == Some(&b'|') {
                            let item = s[end + 2..item_end].to_vec();
                            if item == b"index" {
                                return Err(template_error("index is reserved"));
                            }
                            flush_template(&mut nodes, &mut literal);
                            *pos = item_end + 1;
                            let (body, separator) = parse_template(s, pos, "loop", depth + 1)?;
                            nodes.push(TemplateNode {
                                path,
                                item,
                                body,
                                separator,
                                ..Default::default()
                            });
                            continue;
                        }
                    }
                }
            }
        }
        literal.push(s[*pos]);
        *pos += 1;
    }
    if !mode.is_empty() {
        return Err(template_error("unterminated block"));
    }
    flush_template(&mut nodes, &mut literal);
    Ok((nodes, Vec::new()))
}
fn template_lookup(
    context: &Rc<JsonNode>,
    locals: &std::collections::HashMap<Vec<u8>, Rc<JsonNode>>,
    path: &[u8],
) -> Result<Rc<JsonNode>, Error> {
    let end = template_ident(path, 0).unwrap();
    let mut n = locals.get(&path[..end]).cloned();
    if n.is_none() {
        n = context
            .object
            .iter()
            .find(|m| m.0 == path[..end])
            .map(|m| m.2.clone());
    }
    let missing = || {
        template_error(&format!(
            "missing variable {}",
            String::from_utf8_lossy(path)
        ))
    };
    let mut n = n.ok_or_else(missing)?;
    let mut i = end;
    while i < path.len() {
        if path[i] == b'.' {
            let begin = i + 1;
            i = template_ident(path, begin).unwrap();
            n = if n.kind == Kind::Object {
                n.object
                    .iter()
                    .find(|m| m.0 == path[begin..i])
                    .map(|m| m.2.clone())
            } else {
                None
            }
            .ok_or_else(missing)?;
        } else {
            let begin = i + 1;
            i = begin;
            while path[i] != b']' {
                i += 1;
            }
            let digits = std::str::from_utf8(&path[begin..i]).unwrap();
            i += 1;
            let index = digits
                .parse::<usize>()
                .ok()
                .filter(|v| v.to_string() == digits);
            n = if n.kind == Kind::Array {
                index.and_then(|j| n.array.get(j).cloned())
            } else {
                None
            }
            .ok_or_else(missing)?;
        }
    }
    Ok(n)
}
fn evaluate_template(
    nodes: &[TemplateNode],
    context: &Rc<JsonNode>,
    locals: &std::collections::HashMap<Vec<u8>, Rc<JsonNode>>,
    out: &mut Vec<u8>,
) -> Result<(), Error> {
    for n in nodes {
        if n.path.is_empty() {
            out.extend_from_slice(&n.text);
            continue;
        }
        let v = template_lookup(context, locals, &n.path)?;
        if n.conditional {
            if v.kind != Kind::Bool {
                return Err(template_error(&format!(
                    "condition requires a bool: {}",
                    String::from_utf8_lossy(&n.path)
                )));
            }
            evaluate_template(
                if v.boolean { &n.body } else { &n.otherwise },
                context,
                locals,
                out,
            )?;
            continue;
        }
        if n.item.is_empty() {
            if v.kind == Kind::String {
                out.extend_from_slice(&v.text);
            } else {
                emit(&v, out);
            }
            continue;
        }
        if v.kind != Kind::Array {
            return Err(template_error(&format!(
                "loop requires a list: {}",
                String::from_utf8_lossy(&n.path)
            )));
        }
        let mut scope = locals.clone();
        for (i, item) in v.array.iter().enumerate() {
            if i > 0 {
                out.extend_from_slice(&n.separator);
            }
            let mut index = JsonNode::empty(Kind::Int);
            index.raw = i.to_string().into_bytes();
            index.integer = i as i64;
            scope.insert(n.item.clone(), item.clone());
            scope.insert(b"index".to_vec(), Rc::new(index));
            evaluate_template(&n.body, context, &scope, out)?;
        }
    }
    Ok(())
}
fn render_template(source: &[u8], context: &Rc<JsonNode>) -> Result<Vec<u8>, Error> {
    let nodes = parse_template(source, &mut 0, "", 0)?.0;
    let mut out = Vec::new();
    evaluate_template(&nodes, context, &std::collections::HashMap::new(), &mut out)?;
    Ok(out)
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
        Ok(Value::object(
            "Json",
            ctx.insert_collection("Json", Rc::new(n))?,
        ))
    })?;
    r.function(
        "template_render",
        &["string", "Json"],
        "string",
        |ctx, args| {
            let context = ctx.get::<Rc<JsonNode>>(args[1].as_obj()?)?;
            if context.kind != Kind::Object {
                return Err(Error::host("template_render: context must be an object"));
            }
            Ok(Value::string(render_template(
                args[0].as_bytes()?,
                context,
            )?))
        },
    )?;
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
    fn template_conformance() {
        let bytes = include_bytes!("../../../std/template-cases.json");
        let cases = Parser { s: bytes, p: 0 }.value(0).unwrap();
        let r = Registry::new();
        let mut ctx = Context::new();
        fn field<'a>(n: &'a JsonNode, key: &[u8]) -> Option<&'a Rc<JsonNode>> {
            n.object.iter().find(|m| m.0 == key).map(|m| &m.2)
        }
        for case in &cases.array {
            let context = field(case, b"context").unwrap().clone();
            let handle = Value::object("Json", ctx.insert_collection("Json", context).unwrap());
            let source = field(case, b"template").unwrap().text.clone();
            let result = r.functions["fn template_render(string, Json) -> string"].2(
                &mut ctx,
                &[Value::string(source), handle],
            );
            if let Some(error) = field(case, b"error") {
                assert_eq!(result.unwrap_err().message.as_bytes(), error.text);
            } else {
                assert_eq!(
                    result.unwrap().as_bytes().unwrap(),
                    field(case, b"result").unwrap().text
                );
            }
        }
        let context = Rc::new(JsonNode::empty(Kind::Object));
        assert_eq!(
            render_template(&[255, 0, 128], &context).unwrap(),
            [255, 0, 128]
        );
    }
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
