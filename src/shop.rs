//! The domain knows prices and ledger entries, not bytecode opcodes.
//! Context owns every allocation. ObjRef handles stay valid for this run.
use dslvm::stdlib::collections::{CollectionList, StrMap};
use dslvm::{ByteString, Context, Error, ObjRef, Registry, Value};

struct Money {
    cents: i64,
}
/// Typed null handles use a marker object so the VM can preserve schema type
/// while host callbacks still report the reference runtime's null diagnostic.
struct NullMoney;
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

fn money_ref(ctx: &Context, obj: ObjRef) -> Result<&Money, Error> {
    ctx.get::<Money>(obj).map_err(|_| {
        if ctx.get::<NullMoney>(obj).is_ok() {
            Error::host("money: null handle")
        } else {
            Error::host("money: invalid handle")
        }
    })
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

    // Explicit scalar boundaries preserve bytes, IEEE values, and boolean tags.
    r.function("host_int", &["int"], "int", |_, a| {
        Ok(Value::Int(a[0].as_int()?))
    })?;
    r.function("host_float", &["float"], "float", |_, a| {
        Ok(Value::Float(a[0].as_float()?))
    })?;
    r.function("host_bool", &["bool"], "bool", |_, a| {
        Ok(Value::Bool(a[0].as_bool()?))
    })?;
    r.function("host_bytes", &["string"], "string", |_, a| {
        Ok(Value::string(a[0].as_bytes()?))
    })?;
    r.function("host_hex", &["string"], "string", |_, a| {
        Ok(Value::string(
            a[0].as_bytes()?
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        ))
    })?;
    r.function("host_float_case", &["string"], "float", |_, a| {
        match a[0].as_str()? {
            "-0" => Ok(Value::Float(-0.0)),
            "nan" => Ok(Value::Float(f64::NAN)),
            "inf" => Ok(Value::Float(f64::INFINITY)),
            "-inf" => Ok(Value::Float(f64::NEG_INFINITY)),
            "large" => Ok(Value::Float(1e21)),
            "subnormal" => Ok(Value::Float(f64::from_bits(1))),
            _ => Err(Error::host("host_float_case: unknown case")),
        }
    })?;
    r.function("host_echo_money", &["Money"], "Money", |_, a| {
        Ok(a[0].clone())
    })?;
    r.function("host_same", &["Money", "Money"], "bool", |_, a| {
        Ok(Value::Bool(a[0].as_obj()? == a[1].as_obj()?))
    })?;
    r.function("host_null", &[], "Money", |ctx, _| {
        Ok(Value::object("Money", ctx.insert("Money", NullMoney)))
    })?;
    r.function("host_is_null", &["Money"], "bool", |ctx, a| {
        Ok(Value::Bool(ctx.get::<NullMoney>(a[0].as_obj()?).is_ok()))
    })?;
    r.function("host_wrong", &[], "Money", |ctx, _| {
        Ok(Value::object(
            "Items",
            ctx.insert("Items", Items { values: Vec::new() }),
        ))
    })?;
    r.function("host_void", &[], "void", |_, _| Ok(Value::None))?;
    r.function("host_str_list", &["StrList"], "StrList", |ctx, a| {
        let mut values = ctx
            .get::<CollectionList<ByteString>>(a[0].as_obj()?)?
            .items
            .clone();
        values.push(ByteString::from("host"));
        Ok(Value::object(
            "StrList",
            ctx.insert_collection("StrList", CollectionList { items: values })?,
        ))
    })?;
    r.function("host_int_list", &["IntList"], "IntList", |ctx, a| {
        let mut values = ctx
            .get::<CollectionList<i64>>(a[0].as_obj()?)?
            .items
            .clone();
        values.push(9);
        Ok(Value::object(
            "IntList",
            ctx.insert_collection("IntList", CollectionList { items: values })?,
        ))
    })?;
    r.function("host_float_list", &["FloatList"], "FloatList", |ctx, a| {
        let mut values = ctx
            .get::<CollectionList<f64>>(a[0].as_obj()?)?
            .items
            .clone();
        values.push(0.5);
        Ok(Value::object(
            "FloatList",
            ctx.insert_collection("FloatList", CollectionList { items: values })?,
        ))
    })?;
    r.function("host_str_map", &["StrMap"], "StrMap", |ctx, a| {
        let entries = ctx.get::<StrMap>(a[0].as_obj()?)?.entries();
        let obj = ctx.insert_collection("StrMap", StrMap::default())?;
        {
            let map = ctx.get_mut::<StrMap>(obj)?;
            for (key, value) in entries {
                map.set(key, value);
            }
            map.set(ByteString::from("host"), ByteString::from("yes"));
        }
        Ok(Value::object("StrMap", obj))
    })?;
    r.function("host_sum", &vec!["int"; 255], "int", |_, a| {
        let sum = a.iter().try_fold(0u64, |acc, value| {
            Ok::<_, Error>(acc.wrapping_add(value.as_int()? as u64))
        })?;
        Ok(Value::Int(sum as i64))
    })?;
    r.function(
        &format!("host_long_{}", "x".repeat(128)),
        &["int"],
        "int",
        |_, a| Ok(Value::Int(a[0].as_int()?)),
    )?;

    r.method("Money", "amount", &[], "int", |ctx, this, _| {
        Ok(Value::Int(money_ref(ctx, this)?.cents))
    })?;
    r.method("Money", "currency", &[], "Currency", |ctx, this, _| {
        money_ref(ctx, this)?;
        Ok(Value::string("USD"))
    })?;
    r.method("Money", "plus", &["Money"], "Money", |ctx, this, a| {
        let cents = money_ref(ctx, this)?
            .cents
            .wrapping_add(money_ref(ctx, a[0].as_obj()?)?.cents);
        Ok(money(ctx, cents))
    })?;
    r.method("Money", "minus", &["Money"], "Money", |ctx, this, a| {
        let cents = money_ref(ctx, this)?
            .cents
            .wrapping_sub(money_ref(ctx, a[0].as_obj()?)?.cents);
        Ok(money(ctx, cents))
    })?;
    r.method("Money", "times", &["int"], "Money", |ctx, this, a| {
        let cents = money_ref(ctx, this)?.cents.wrapping_mul(a[0].as_int()?);
        Ok(money(ctx, cents))
    })?;
    r.method("Money", "neg", &[], "Money", |ctx, this, _| {
        let cents = money_ref(ctx, this)?.cents.wrapping_neg();
        Ok(money(ctx, cents))
    })?;
    // returns:self is registered as the concrete owner type, not a Rust reference.
    r.method("Money", "set", &["int"], "Money", |ctx, this, a| {
        if ctx.get::<NullMoney>(this).is_ok() {
            return Err(Error::host("money: null handle"));
        }
        ctx.get_mut::<Money>(this)
            .map_err(|_| Error::host("money: invalid handle"))?
            .cents = a[0].as_int()?;
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
        let cents = money_ref(ctx, a[0].as_obj()?)?.cents;
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
