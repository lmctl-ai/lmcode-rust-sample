//! The domain knows prices and ledger entries, not bytecode opcodes.
//! Context owns every allocation. ObjRef handles stay valid for this run.
use dslvm::{ByteString, Context, Error, ObjRef, Registry, Value};

struct Money {
    cents: i64,
}
struct Items {
    values: Vec<ObjRef>,
}
struct Shop {
    items: ObjRef,
}
#[derive(Default)]
pub struct Ledger {
    pub balance: i64,
    pub entries: i64,
    pub note: ByteString,
}

pub struct Host {
    pub registry: Registry,
    pub context: Context,
    pub ledger: ObjRef,
}

fn money(ctx: &mut Context, cents: i64) -> Value {
    Value::object("Money", ctx.insert("Money", Money { cents }))
}
fn units(ctx: &mut Context, amount: f64) -> Result<Value, Error> {
    // Match C++ round: half away from zero. Validate before the float -> int cast.
    let cents = (amount * 100.0).round();
    if !cents.is_finite() || !(-9223372036854775808.0..9223372036854775808.0).contains(&cents) {
        return Err(Error::host("money: amount out of range"));
    }
    Ok(money(ctx, cents as i64))
}

impl Host {
    pub fn new() -> Result<Self, Error> {
        // Default std registers strings, math, collections and files. Testing is opt-in.
        let mut registry = Registry::new();
        dslvm::stdlib::register_testing(&mut registry, None)?;
        registry
            .context("shop", "Shop")?
            .context("ledger", "Ledger")?;
        register(&mut registry)?;

        let mut context = Context::new();
        let values = [125, 250, 375]
            .into_iter()
            .map(|cents| context.insert("Money", Money { cents }))
            .collect();
        let items = context.insert("Items", Items { values });
        context.set_object("shop", "Shop", Shop { items });
        let ledger = context.set_object("ledger", "Ledger", Ledger::default());
        Ok(Self {
            registry,
            context,
            ledger,
        })
    }
}

fn register(r: &mut Registry) -> Result<(), Error> {
    // Full parameter tuples distinguish overloads: int is cents; float is units.
    r.function("money", &["int"], "Money", |ctx, a| {
        Ok(money(ctx, a[0].as_int()?))
    })?;
    r.function("money", &["float"], "Money", |ctx, a| {
        units(ctx, a[0].as_float()?)
    })?;
    r.function("fee", &["int"], "int", |_, a| {
        Ok(Value::Int(a[0].as_int()?.wrapping_add(5)))
    })?;
    r.function(
        "sample_tmp",
        &[],
        "string",
        |_, _| match std::env::var_os("SAMPLE_TMP").filter(|s| !s.is_empty()) {
            Some(path) => {
                #[cfg(unix)]
                {
                    use std::os::unix::ffi::OsStrExt;
                    Ok(Value::string(path.as_bytes()))
                }
                #[cfg(not(unix))]
                {
                    path.into_string()
                        .map(Value::string)
                        .map_err(|_| Error::host("SAMPLE_TMP is not valid UTF-8"))
                }
            }
            None => Err(Error::host("SAMPLE_TMP is required")),
        },
    )?;

    r.method("Money", "amount", &[], "int", |ctx, this, _| {
        Ok(Value::Int(ctx.get::<Money>(this)?.cents))
    })?;
    r.method("Money", "currency", &[], "Currency", |_, _, _| {
        Ok(Value::string("USD"))
    })?;
    r.method("Money", "plus", &["Money"], "Money", |ctx, this, a| {
        let cents = ctx
            .get::<Money>(this)?
            .cents
            .wrapping_add(ctx.get::<Money>(a[0].as_obj()?)?.cents);
        Ok(money(ctx, cents))
    })?;
    r.method("Money", "minus", &["Money"], "Money", |ctx, this, a| {
        let cents = ctx
            .get::<Money>(this)?
            .cents
            .wrapping_sub(ctx.get::<Money>(a[0].as_obj()?)?.cents);
        Ok(money(ctx, cents))
    })?;
    r.method("Money", "times", &["int"], "Money", |ctx, this, a| {
        let cents = ctx.get::<Money>(this)?.cents.wrapping_mul(a[0].as_int()?);
        Ok(money(ctx, cents))
    })?;
    r.method("Money", "neg", &[], "Money", |ctx, this, _| {
        let cents = ctx.get::<Money>(this)?.cents.wrapping_neg();
        Ok(money(ctx, cents))
    })?;
    // returns:self is registered as the concrete owner type, not a Rust reference.
    r.method("Money", "set", &["int"], "Money", |ctx, this, a| {
        ctx.get_mut::<Money>(this)?.cents = a[0].as_int()?;
        Ok(Value::object("Money", this))
    })?;

    // len/at make this host-owned collection iterable by the compiler's foreach.
    r.method("Items", "len", &[], "int", |ctx, this, _| {
        Ok(Value::Int(ctx.get::<Items>(this)?.values.len() as i64))
    })?;
    r.method("Items", "at", &["int"], "Money", |ctx, this, a| {
        let index = usize::try_from(a[0].as_int()?)
            .map_err(|_| Error::host("items: index out of range"))?;
        let obj = ctx
            .get::<Items>(this)?
            .values
            .get(index)
            .copied()
            .ok_or_else(|| Error::host("items: index out of range"))?;
        Ok(Value::object("Money", obj))
    })?;
    r.method("Shop", "items", &[], "Items", |ctx, this, _| {
        Ok(Value::object("Items", ctx.get::<Shop>(this)?.items))
    })?;
    r.method("Shop", "total", &[], "Money", |ctx, this, _| {
        let items = ctx.get::<Shop>(this)?.items;
        let mut total = 0i64;
        for obj in &ctx.get::<Items>(items)?.values {
            total = total.wrapping_add(ctx.get::<Money>(*obj)?.cents);
        }
        Ok(money(ctx, total))
    })?;
    r.method("Shop", "price", &["int"], "Money", |ctx, _, a| {
        Ok(money(ctx, a[0].as_int()?))
    })?;
    r.method("Shop", "price", &["float"], "Money", |ctx, _, a| {
        units(ctx, a[0].as_float()?)
    })?;
    // Enums cross the native boundary as string-backed Values.
    r.method("Shop", "currency", &[], "Currency", |_, _, _| {
        Ok(Value::string("USD"))
    })?;
    r.method("Shop", "label", &[], "string", |_, _, _| {
        Ok(Value::string("sample shop"))
    })?;

    r.method("Ledger", "record", &["Money"], "Ledger", |ctx, this, a| {
        let cents = ctx.get::<Money>(a[0].as_obj()?)?.cents;
        let ledger = ctx.get_mut::<Ledger>(this)?;
        ledger.balance = ledger.balance.wrapping_add(cents);
        ledger.entries = ledger.entries.wrapping_add(1);
        Ok(Value::object("Ledger", this))
    })?;
    r.method("Ledger", "balance", &[], "Money", |ctx, this, _| {
        let cents = ctx.get::<Ledger>(this)?.balance;
        Ok(money(ctx, cents))
    })?;
    r.method("Ledger", "count", &[], "int", |ctx, this, _| {
        Ok(Value::Int(ctx.get::<Ledger>(this)?.entries))
    })?;
    // ByteString stores the original bytes; do not silently coerce them into UTF-8.
    r.method("Ledger", "note", &["string"], "void", |ctx, this, a| {
        ctx.get_mut::<Ledger>(this)?.note = a[0].as_string()?.clone();
        Ok(Value::None)
    })?;
    r.method("Ledger", "note", &[], "string", |ctx, this, _| {
        Ok(Value::Str(ctx.get::<Ledger>(this)?.note.clone()))
    })?;
    r.method("Ledger", "reject", &["string"], "void", |_, _, a| {
        Err(Error::host(a[0].as_str()?))
    })?;
    Ok(())
}
