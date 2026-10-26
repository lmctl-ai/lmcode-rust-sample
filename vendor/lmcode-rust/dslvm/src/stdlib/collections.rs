//! Context-owned byte-oriented collection helpers matching the C++ catalog.
use crate::{Context, Error, Registry, Value};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct CollectionList<T> { pub items: Vec<T> }
pub type StrList = CollectionList<crate::ByteString>;
pub type IntList = CollectionList<i64>;
pub type FloatList = CollectionList<f64>;
#[derive(Debug, Default)]
pub struct StrMap {
    /// Physical insertion storage; removed tombstones remain until amortized compaction.
    pub items: Vec<(crate::ByteString, crate::ByteString)>,
    index: HashMap<crate::ByteString, usize>,
    active: usize,
    fenwick: Vec<usize>,
}
impl StrMap {
    fn find(&self, key: &crate::ByteString) -> Option<usize> { self.index.get(key).copied() }
    fn prefix(&self, mut end: usize) -> usize { let mut sum = 0; while end > 0 { sum += self.fenwick[end]; end &= end - 1; } sum }
    fn add_rank(&mut self, pos: usize, delta: isize) { let mut i = pos + 1; while i < self.fenwick.len() { if delta < 0 { self.fenwick[i] -= (-delta) as usize; } else { self.fenwick[i] += delta as usize; } i += i & (!i + 1); } }
    fn select_rank(&self, rank: usize) -> usize { let mut idx = 0; let mut bit = self.fenwick.len().next_power_of_two() >> 1; let mut sum = 0; while bit != 0 { let next = idx + bit; if next < self.fenwick.len() && sum + self.fenwick[next] <= rank { idx = next; sum += self.fenwick[next]; } bit >>= 1; } idx }
    fn compact(&mut self) { let old = std::mem::take(&mut self.items); self.items = old.into_iter().enumerate().filter(|(pos, (key, _))| self.index.get(key) == Some(pos)).map(|(_, item)| item).collect(); self.index.clear(); for (i, (key, _)) in self.items.iter().enumerate() { self.index.insert(key.clone(), i); } self.fenwick = vec![0; self.items.len() + 1]; for i in 1..self.fenwick.len() { self.fenwick[i] = i & (!i + 1); } }
    fn append(&mut self, key: crate::ByteString, value: crate::ByteString) { if self.items.len() > self.active.saturating_mul(2) + 1024 { self.compact(); } let pos = self.items.len(); self.items.push((key.clone(), value)); self.index.insert(key, pos); if self.fenwick.is_empty() { self.fenwick.push(0); } let i = pos + 1; let low = i & (!i + 1); self.fenwick.push(1 + self.prefix(pos) - self.prefix(i - low)); let mut parent = i + low; while parent < self.fenwick.len() { self.fenwick[parent] += 1; parent += parent & (!parent + 1); } self.active += 1; }
    pub fn entries(&self) -> Vec<(crate::ByteString, crate::ByteString)> { self.items.iter().enumerate().filter(|(pos, (key, _))| self.index.get(key) == Some(pos)).map(|(_, item)| item.clone()).collect() }
    pub fn len(&self) -> usize { self.active }
    pub fn is_empty(&self) -> bool { self.active == 0 }
    pub fn set(&mut self, key: crate::ByteString, value: crate::ByteString) { if let Some(pos) = self.find(&key) { self.items[pos].1 = value; } else { self.append(key, value); } }
}

trait Element: Clone + PartialEq + 'static {
    const TYPE: &'static str;
    fn get(value: &Value) -> Result<Self, Error>;
    fn put(value: Self) -> Value;
}
impl Element for crate::ByteString {
    const TYPE: &'static str = "string";
    fn get(value: &Value) -> Result<Self, Error> { Ok(value.as_string()?.clone()) }
    fn put(value: Self) -> Value { Value::string(value) }
}
impl Element for i64 {
    const TYPE: &'static str = "int";
    fn get(value: &Value) -> Result<Self, Error> { value.as_int() }
    fn put(value: Self) -> Value { Value::Int(value) }
}
impl Element for f64 {
    const TYPE: &'static str = "float";
    fn get(value: &Value) -> Result<Self, Error> { value.as_float() }
    fn put(value: Self) -> Value { Value::Float(value) }
}

fn index(i: i64, size: usize, method: &str) -> Result<usize, Error> {
    if i < 0 || i as u64 >= size as u64 {
        Err(Error::host(format!("{}: index out of range: {}", method, i)))
    } else {
        Ok(i as usize)
    }
}

fn register_list<T: Element>(registry: &mut Registry, type_name: &'static str) -> Result<(), Error> {
    registry.method(type_name, "len", &[], "int", move |ctx, this, _| {
        Ok(Value::Int(ctx.get::<CollectionList<T>>(this)?.items.len() as i64))
    })?;
    registry.method(type_name, "at", &["int"], T::TYPE, move |ctx, this, args| {
        let list = ctx.get::<CollectionList<T>>(this)?;
        let i = index(args[0].as_int()?, list.items.len(), &format!("{}.at", type_name))?;
        Ok(T::put(list.items[i].clone()))
    })?;
    registry.method(type_name, "add", &[T::TYPE], type_name, move |ctx, this, args| {
        ctx.get_mut::<CollectionList<T>>(this)?.items.push(T::get(&args[0])?);
        Ok(Value::object(type_name, this))
    })?;
    registry.method(type_name, "set", &["int", T::TYPE], type_name, move |ctx, this, args| {
        let list = ctx.get_mut::<CollectionList<T>>(this)?;
        let i = index(args[0].as_int()?, list.items.len(), &format!("{}.set", type_name))?;
        list.items[i] = T::get(&args[1])?;
        Ok(Value::object(type_name, this))
    })?;
    registry.method(type_name, "contains", &[T::TYPE], "bool", move |ctx, this, args| {
        let needle = T::get(&args[0])?;
        Ok(Value::Bool(ctx.get::<CollectionList<T>>(this)?.items.iter().any(|v| v == &needle)))
    })?;
    registry.method(type_name, "index_of", &[T::TYPE], "int", move |ctx, this, args| {
        let needle = T::get(&args[0])?;
        let list = &ctx.get::<CollectionList<T>>(this)?.items;
        Ok(Value::Int(list.iter().position(|v| v == &needle).map(|i| i as i64).unwrap_or(-1)))
    })?;
    registry.method(type_name, "clear", &[], type_name, move |ctx, this, _| {
        ctx.get_mut::<CollectionList<T>>(this)?.items.clear();
        Ok(Value::object(type_name, this))
    })?;
    Ok(())
}

fn make_collection<T: 'static>(ctx: &mut Context, type_name: &'static str, value: CollectionList<T>) -> Result<Value, Error> {
    Ok(Value::object(type_name, ctx.insert_collection(type_name, value)?))
}

/// Register the list and ordered string-map catalog in a registry.
pub fn register_collections(registry: &mut Registry) -> Result<&mut Registry, Error> {
    registry.begin_compatibility_registration();
    register_list::<crate::ByteString>(registry, "StrList")?;
    register_list::<i64>(registry, "IntList")?;
    register_list::<f64>(registry, "FloatList")?;
    registry.method("StrList", "join", &["string"], "string", |ctx, this, args| {
        let list = &ctx.get::<StrList>(this)?.items;
        let sep = args[0].as_bytes()?;
        let mut out = Vec::new();
        for (i, item) in list.iter().enumerate() {
            if i != 0 { out.extend_from_slice(sep); }
            out.extend_from_slice(item.as_bytes());
        }
        Ok(Value::string(out))
    })?;
    registry.function("str_list", &[], "StrList", |ctx, _| make_collection(ctx, "StrList", CollectionList::<crate::ByteString> { items: Vec::new() }))?;
    registry.function("int_list", &[], "IntList", |ctx, _| make_collection(ctx, "IntList", CollectionList::<i64> { items: Vec::new() }))?;
    registry.function("float_list", &[], "FloatList", |ctx, _| make_collection(ctx, "FloatList", CollectionList::<f64> { items: Vec::new() }))?;
    registry.function("str_map", &[], "StrMap", |ctx, _| Ok(Value::object("StrMap", ctx.insert_collection("StrMap", StrMap::default())?)))?;
    registry.function("str_split", &["string", "string"], "StrList", |ctx, args| {
        let source = args[0].as_bytes()?;
        let separator = args[1].as_bytes()?;
        if separator.is_empty() { return Err(Error::host("str_split: separator must not be empty")); }
        let mut items = Vec::new();
        let mut at = 0;
        while let Some(i) = source[at..].windows(separator.len()).position(|w| w == separator) {
            let p = at + i;
            items.push(crate::ByteString::from(&source[at..p]));
            at = p + separator.len();
        }
        items.push(crate::ByteString::from(&source[at..]));
        make_collection(ctx, "StrList", CollectionList { items })
    })?;
    registry.method("StrMap", "len", &[], "int", |ctx, this, _| Ok(Value::Int(ctx.get::<StrMap>(this)?.active as i64)))?;
    registry.method("StrMap", "has", &["string"], "bool", |ctx, this, args| {
        let key = args[0].as_string()?;
        Ok(Value::Bool(ctx.get::<StrMap>(this)?.find(key).is_some()))
    })?;
    registry.method("StrMap", "get", &["string"], "string", |ctx, this, args| {
        let key = args[0].as_string()?;
        ctx.get::<StrMap>(this)?.find(key).map(|i| Value::string(ctx.get::<StrMap>(this).unwrap().items[i].1.clone())).ok_or_else(|| Error::host("StrMap.get: missing key"))
    })?;
    registry.method("StrMap", "get_or", &["string", "string"], "string", |ctx, this, args| {
        let key = args[0].as_string()?;
        let fallback = args[1].as_string()?.clone();
        Ok(ctx.get::<StrMap>(this)?.find(key).map(|i| Value::string(ctx.get::<StrMap>(this).unwrap().items[i].1.clone())).unwrap_or_else(|| Value::string(fallback)))
    })?;
    registry.method("StrMap", "set", &["string", "string"], "StrMap", |ctx, this, args| {
        let key = args[0].as_string()?.clone();
        let value = args[1].as_string()?.clone();
        ctx.get_mut::<StrMap>(this)?.set(key, value);
        Ok(Value::object("StrMap", this))
    })?;
    registry.method("StrMap", "key_at", &["int"], "string", |ctx, this, args| {
        let map = ctx.get_mut::<StrMap>(this)?;
        let wanted = args[0].as_int()?;
        if wanted < 0 || wanted as usize >= map.active { return Err(Error::host(format!("StrMap.key_at: index out of range: {}", wanted))); }
        Ok(Value::string(map.items[map.select_rank(wanted as usize)].0.clone()))
    })?;
    registry.method("StrMap", "remove", &["string"], "bool", |ctx, this, args| {
        let key = args[0].as_string()?;
        let map = ctx.get_mut::<StrMap>(this)?;
        if let Some(pos) = map.find(key) { map.index.remove(key); map.add_rank(pos, -1); map.active -= 1; Ok(Value::Bool(true)) } else { Ok(Value::Bool(false)) }
    })?;
    registry.method("StrMap", "clear", &[], "StrMap", |ctx, this, _| { let map = ctx.get_mut::<StrMap>(this)?; map.items.clear(); map.index.clear(); map.active = 0; map.fenwick.clear(); Ok(Value::object("StrMap", this)) })?;
    registry.end_compatibility_registration();
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ByteString;

    #[test]
    fn registration_has_the_complete_catalog() {
        let mut registry = Registry::without_std();
        register_collections(&mut registry).unwrap();
        assert_eq!(registry.signatures().len(), 35);
    }

    #[test]
    fn collection_quota_counts_only_collection_objects() {
        let mut context = Context::new();
        for _ in 0..65_536 { context.insert_collection("IntList", IntList::default()).unwrap(); }
        context.insert("Helper", 1u8);
        let error = context.insert_collection("StrMap", StrMap::default()).unwrap_err();
        assert_eq!(error.code, "RUN_HOST");
        assert_eq!(error.message, "StrMap object limit exceeded (65536)");
        let mut other = Context::new();
        other.insert_collection("StrList", StrList::default()).unwrap();
    }

    #[test]
    fn cleared_collection_slots_are_reused_without_touching_host_objects() {
        let mut context = Context::new();
        let helper = context.insert("Helper", 7u8);
        for _ in 0..8 { context.insert_collection("IntList", IntList::default()).unwrap(); context.clear_collections(); }
        assert_eq!(context.len(), 2);
        assert_eq!(*context.get::<u8>(helper).unwrap(), 7);
        context.insert_collection("StrMap", StrMap::default()).unwrap();
        assert_eq!(context.len(), 2);
    }

    #[test]
    fn bulk_removal_keeps_order_indexed_without_quadratic_rebuilds() {
        let mut map = StrMap::default();
        for i in 0..20_000u32 { let key = ByteString::from(i.to_string()); map.index.insert(key.clone(), map.items.len()); map.items.push((key, ByteString::default())); map.active += 1; }
        for i in 0..20_000u32 { let key = ByteString::from(i.to_string()); let pos = map.index.remove(&key).unwrap(); map.add_rank(pos, -1); map.active -= 1; }
        assert_eq!(map.active, 0);
    }

    #[test]
    fn bulk_registered_key_at_compacts_once_then_reads_indexed_order() {
        let mut registry = Registry::without_std(); register_collections(&mut registry).unwrap();
        let mut context = Context::new(); let object = context.insert_collection("StrMap", StrMap::default()).unwrap();
        let set = &registry.methods["method StrMap.set(string, string) -> StrMap"].3;
        let remove = &registry.methods["method StrMap.remove(string) -> bool"].3;
        let key_at = &registry.methods["method StrMap.key_at(int) -> string"].3;
        for i in 0..20_000 { let key = Value::string(i.to_string()); set(&mut context, object, &[key.clone(), key]).unwrap(); }
        for i in (0..20_000).step_by(2) { remove(&mut context, object, &[Value::string(i.to_string())]).unwrap(); }
        for i in [1, 777, 9_999] { let _ = key_at(&mut context, object, &[Value::Int(i)]).unwrap(); }
        for i in (0..20_000).step_by(2) { set(&mut context, object, &[Value::string(i.to_string()), Value::string("reinsert")]).unwrap(); }
        assert_eq!(context.get::<StrMap>(object).unwrap().active, 20_000);
        assert_eq!(context.get::<StrMap>(object).unwrap().entries().len(), 20_000);
    }

    #[test]
    fn split_preserves_empty_fields_and_bytes() {
        let mut context = Context::new();
        let args = [Value::string(b",a,,".as_slice()), Value::string(b",".as_slice())];
        let result = {
            let mut registry = Registry::without_std();
            register_collections(&mut registry).unwrap();
            let (_, _, function) = registry.functions.get("fn str_split(string, string) -> StrList").unwrap();
            function(&mut context, &args).unwrap()
        };
        let object = result.as_obj().unwrap();
        assert_eq!(context.get::<StrList>(object).unwrap().items, vec![ByteString::from(b""), ByteString::from(b"a"), ByteString::from(b""), ByteString::from(b"")]);
    }
}

