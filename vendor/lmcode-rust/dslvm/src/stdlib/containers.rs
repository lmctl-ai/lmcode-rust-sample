//! Typed maps and pop-ordered containers. Values and byte keys cross unchanged.
use super::collections::{index, Element};
use crate::{ByteString, Error, Registry, Value};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, VecDeque};

#[derive(Debug)]
pub struct TypedMap<T> {
    pub items: Vec<(ByteString, T)>,
    index: HashMap<ByteString, usize>,
}
impl<T> Default for TypedMap<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            index: HashMap::new(),
        }
    }
}
pub type IntMap = TypedMap<i64>;
pub type FloatMap = TypedMap<f64>;
impl<T> TypedMap<T> {
    pub fn set(&mut self, key: ByteString, value: T) {
        if let Some(&i) = self.index.get(&key) {
            self.items[i].1 = value;
        } else {
            self.index.insert(key.clone(), self.items.len());
            self.items.push((key, value));
        }
    }
}
#[derive(Debug)]
pub struct PopContainer<T> {
    pub items: VecDeque<T>,
}
impl<T> Default for PopContainer<T> {
    fn default() -> Self {
        Self {
            items: VecDeque::new(),
        }
    }
}
pub type IntStack = PopContainer<i64>;
pub type FloatStack = PopContainer<f64>;
pub type StrStack = PopContainer<ByteString>;
pub type IntQueue = PopContainer<i64>;
pub type FloatQueue = PopContainer<f64>;
pub type StrQueue = PopContainer<ByteString>;

#[derive(Debug)]
struct PriorityItem<T> {
    value: T,
    priority: f64,
    sequence: i64,
}
// BinaryHeap is max-first, so reverse priority and insertion order. PartialCmp
// deliberately treats signed zero as equal; finite priorities exclude NaN.
impl<T> PartialEq for PriorityItem<T> {
    fn eq(&self, b: &Self) -> bool {
        self.priority == b.priority && self.sequence == b.sequence
    }
}
impl<T> Eq for PriorityItem<T> {}
impl<T> PartialOrd for PriorityItem<T> {
    fn partial_cmp(&self, b: &Self) -> Option<Ordering> {
        Some(self.cmp(b))
    }
}
impl<T> Ord for PriorityItem<T> {
    fn cmp(&self, b: &Self) -> Ordering {
        b.priority
            .partial_cmp(&self.priority)
            .unwrap()
            .then_with(|| b.sequence.cmp(&self.sequence))
    }
}
#[derive(Debug)]
pub struct PriorityQueue<T> {
    items: BinaryHeap<PriorityItem<T>>,
    next_sequence: i64,
}
impl<T> Default for PriorityQueue<T> {
    fn default() -> Self {
        Self {
            items: BinaryHeap::new(),
            next_sequence: 0,
        }
    }
}
pub type IntPQ = PriorityQueue<i64>;
pub type FloatPQ = PriorityQueue<f64>;
pub type StrPQ = PriorityQueue<ByteString>;

fn map<T: Element>(r: &mut Registry, t: &'static str, constructor: &str) -> Result<(), Error> {
    r.function(constructor, &[], t, move |ctx, _| {
        Ok(Value::object(
            t,
            ctx.insert_collection(t, TypedMap::<T>::default())?,
        ))
    })?;
    r.method(t, "len", &[], "int", move |ctx, this, _| {
        Ok(Value::Int(ctx.get::<TypedMap<T>>(this)?.items.len() as i64))
    })?;
    r.method(t, "has", &["string"], "bool", move |ctx, this, a| {
        Ok(Value::Bool(
            ctx.get::<TypedMap<T>>(this)?
                .index
                .contains_key(a[0].as_string()?),
        ))
    })?;
    r.method(t, "get", &["string"], T::TYPE, move |ctx, this, a| {
        let m = ctx.get::<TypedMap<T>>(this)?;
        let i = m
            .index
            .get(a[0].as_string()?)
            .ok_or_else(|| Error::host(format!("{t}.get: missing key")))?;
        Ok(T::put(m.items[*i].1.clone()))
    })?;
    r.method(
        t,
        "get_or",
        &["string", T::TYPE],
        T::TYPE,
        move |ctx, this, a| {
            let m = ctx.get::<TypedMap<T>>(this)?;
            Ok(match m.index.get(a[0].as_string()?) {
                Some(&i) => T::put(m.items[i].1.clone()),
                None => a[1].clone(),
            })
        },
    )?;
    r.method(t, "set", &["string", T::TYPE], t, move |ctx, this, a| {
        ctx.get_mut::<TypedMap<T>>(this)?
            .set(a[0].as_string()?.clone(), T::get(&a[1])?);
        Ok(Value::object(t, this))
    })?;
    r.method(t, "key_at", &["int"], "string", move |ctx, this, a| {
        let m = ctx.get::<TypedMap<T>>(this)?;
        let i = index(a[0].as_int()?, m.items.len(), &format!("{t}.key_at"))?;
        Ok(Value::string(m.items[i].0.clone()))
    })?;
    r.method(t, "remove", &["string"], "bool", move |ctx, this, a| {
        let m = ctx.get_mut::<TypedMap<T>>(this)?;
        if let Some(i) = m.index.remove(a[0].as_string()?) {
            m.items.remove(i);
            for j in i..m.items.len() {
                m.index.insert(m.items[j].0.clone(), j);
            }
            Ok(Value::Bool(true))
        } else {
            Ok(Value::Bool(false))
        }
    })?;
    r.method(t, "clear", &[], t, move |ctx, this, _| {
        let m = ctx.get_mut::<TypedMap<T>>(this)?;
        m.items.clear();
        m.index.clear();
        Ok(Value::object(t, this))
    })?;
    Ok(())
}
fn pop_container<T: Element>(
    r: &mut Registry,
    t: &'static str,
    constructor: &str,
    stack: bool,
) -> Result<(), Error> {
    r.function(constructor, &[], t, move |ctx, _| {
        Ok(Value::object(
            t,
            ctx.insert_collection(t, PopContainer::<T>::default())?,
        ))
    })?;
    r.method(t, "push", &[T::TYPE], t, move |ctx, this, a| {
        ctx.get_mut::<PopContainer<T>>(this)?
            .items
            .push_back(T::get(&a[0])?);
        Ok(Value::object(t, this))
    })?;
    r.method(t, "pop", &[], T::TYPE, move |ctx, this, _| {
        let q = &mut ctx.get_mut::<PopContainer<T>>(this)?.items;
        let v = if stack { q.pop_back() } else { q.pop_front() };
        v.map(T::put)
            .ok_or_else(|| Error::host(format!("{t}.pop: empty")))
    })?;
    r.method(t, "peek", &[], T::TYPE, move |ctx, this, _| {
        let q = &ctx.get::<PopContainer<T>>(this)?.items;
        let v = if stack { q.back() } else { q.front() };
        v.cloned()
            .map(T::put)
            .ok_or_else(|| Error::host(format!("{t}.peek: empty")))
    })?;
    r.method(t, "at", &["int"], T::TYPE, move |ctx, this, a| {
        let q = &ctx.get::<PopContainer<T>>(this)?.items;
        let i = index(a[0].as_int()?, q.len(), &format!("{t}.at"))?;
        Ok(T::put(q[if stack { q.len() - 1 - i } else { i }].clone()))
    })?;
    r.method(t, "len", &[], "int", move |ctx, this, _| {
        Ok(Value::Int(
            ctx.get::<PopContainer<T>>(this)?.items.len() as i64
        ))
    })?;
    r.method(t, "empty", &[], "bool", move |ctx, this, _| {
        Ok(Value::Bool(
            ctx.get::<PopContainer<T>>(this)?.items.is_empty(),
        ))
    })?;
    r.method(t, "clear", &[], t, move |ctx, this, _| {
        ctx.get_mut::<PopContainer<T>>(this)?.items.clear();
        Ok(Value::object(t, this))
    })?;
    Ok(())
}
fn pq<T: Element>(r: &mut Registry, t: &'static str, constructor: &str) -> Result<(), Error> {
    r.function(constructor, &[], t, move |ctx, _| {
        Ok(Value::object(
            t,
            ctx.insert_collection(t, PriorityQueue::<T>::default())?,
        ))
    })?;
    r.method(t, "push", &[T::TYPE, "float"], t, move |ctx, this, a| {
        let priority = a[1].as_float()?;
        if !priority.is_finite() {
            return Err(Error::host(format!("{t}.push: priority must be finite")));
        }
        let q = ctx.get_mut::<PriorityQueue<T>>(this)?;
        if q.next_sequence == i64::MAX {
            return Err(Error::host(format!(
                "{t}.push: insertion sequence exhausted"
            )));
        }
        q.items.push(PriorityItem {
            value: T::get(&a[0])?,
            priority,
            sequence: q.next_sequence,
        });
        q.next_sequence += 1;
        Ok(Value::object(t, this))
    })?;
    r.method(t, "pop", &[], T::TYPE, move |ctx, this, _| {
        ctx.get_mut::<PriorityQueue<T>>(this)?
            .items
            .pop()
            .map(|x| T::put(x.value))
            .ok_or_else(|| Error::host(format!("{t}.pop: empty")))
    })?;
    r.method(t, "peek", &[], T::TYPE, move |ctx, this, _| {
        ctx.get::<PriorityQueue<T>>(this)?
            .items
            .peek()
            .map(|x| T::put(x.value.clone()))
            .ok_or_else(|| Error::host(format!("{t}.peek: empty")))
    })?;
    r.method(t, "len", &[], "int", move |ctx, this, _| {
        Ok(Value::Int(
            ctx.get::<PriorityQueue<T>>(this)?.items.len() as i64
        ))
    })?;
    r.method(t, "empty", &[], "bool", move |ctx, this, _| {
        Ok(Value::Bool(
            ctx.get::<PriorityQueue<T>>(this)?.items.is_empty(),
        ))
    })?;
    r.method(t, "clear", &[], t, move |ctx, this, _| {
        let q = ctx.get_mut::<PriorityQueue<T>>(this)?;
        q.items.clear();
        q.next_sequence = 0;
        Ok(Value::object(t, this))
    })?;
    Ok(())
}
/// Register the v2 containers, with no host-owned arena or external dependencies.
pub fn register_containers(r: &mut Registry) -> Result<&mut Registry, Error> {
    r.begin_compatibility_registration();
    map::<i64>(r, "IntMap", "int_map")?;
    map::<f64>(r, "FloatMap", "float_map")?;
    pop_container::<i64>(r, "IntStack", "int_stack", true)?;
    pop_container::<f64>(r, "FloatStack", "float_stack", true)?;
    pop_container::<ByteString>(r, "StrStack", "str_stack", true)?;
    pop_container::<i64>(r, "IntQueue", "int_queue", false)?;
    pop_container::<f64>(r, "FloatQueue", "float_queue", false)?;
    pop_container::<ByteString>(r, "StrQueue", "str_queue", false)?;
    pq::<i64>(r, "IntPQ", "int_pq")?;
    pq::<f64>(r, "FloatPQ", "float_pq")?;
    pq::<ByteString>(r, "StrPQ", "str_pq")?;
    r.end_compatibility_registration();
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Context;

    #[test]
    fn priority_rejections_do_not_mutate_and_clear_resets_sequence() {
        let r = Registry::new();
        let mut ctx = Context::new();
        let h = ctx.insert_collection("IntPQ", IntPQ::default()).unwrap();
        let push = &r.methods["method IntPQ.push(int, float) -> IntPQ"].3;
        let clear = &r.methods["method IntPQ.clear() -> IntPQ"].3;
        for priority in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                push(&mut ctx, h, &[Value::Int(1), Value::Float(priority)])
                    .unwrap_err()
                    .message,
                "IntPQ.push: priority must be finite"
            );
        }
        assert_eq!(ctx.get::<IntPQ>(h).unwrap().next_sequence, 0);
        ctx.get_mut::<IntPQ>(h).unwrap().next_sequence = i64::MAX;
        assert_eq!(
            push(&mut ctx, h, &[Value::Int(1), Value::Float(0.0)])
                .unwrap_err()
                .message,
            "IntPQ.push: insertion sequence exhausted"
        );
        assert!(ctx.get::<IntPQ>(h).unwrap().items.is_empty());
        clear(&mut ctx, h, &[]).unwrap();
        assert_eq!(ctx.get::<IntPQ>(h).unwrap().next_sequence, 0);
        push(&mut ctx, h, &[Value::Int(11), Value::Float(-0.0)]).unwrap();
        push(&mut ctx, h, &[Value::Int(22), Value::Float(0.0)]).unwrap();
        let q = ctx.get_mut::<IntPQ>(h).unwrap();
        assert_eq!(q.items.pop().unwrap().value, 11);
        assert_eq!(q.items.pop().unwrap().value, 22);
    }

    #[test]
    fn floating_items_keep_nan_payload_and_signed_zero() {
        let r = Registry::new();
        let mut ctx = Context::new();
        let h = ctx
            .insert_collection("FloatPQ", FloatPQ::default())
            .unwrap();
        let push = &r.methods["method FloatPQ.push(float, float) -> FloatPQ"].3;
        let pop = &r.methods["method FloatPQ.pop() -> float"].3;
        let bits = 0x7ff8_0000_0000_0042;
        for value in [f64::from_bits(bits), -0.0] {
            push(&mut ctx, h, &[Value::Float(value), Value::Float(1.0)]).unwrap();
        }
        assert_eq!(
            pop(&mut ctx, h, &[]).unwrap().as_float().unwrap().to_bits(),
            bits
        );
        assert_eq!(
            pop(&mut ctx, h, &[]).unwrap().as_float().unwrap().to_bits(),
            (-0.0f64).to_bits()
        );
    }
}
