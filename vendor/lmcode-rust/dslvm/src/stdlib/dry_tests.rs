use super::dry_dummy;
use crate::stdlib::collections::{FloatList, IntList, StrList, StrMap};
use crate::stdlib::containers::{FloatMap, IntMap};
use crate::stdlib::result::{set_result, ResultSink};
use crate::stdlib::ResultContract;
use crate::{Context, Value};

#[test]
fn typed_defaults_match_declared_kinds_and_unknown_objects_are_dry_null() {
    let mut context = Context::new();
    assert!(matches!(
        dry_dummy(&mut context, "void").unwrap(),
        Value::None
    ));
    assert!(matches!(
        dry_dummy(&mut context, "null").unwrap(),
        Value::DryNull(_)
    ));
    assert!(matches!(
        dry_dummy(&mut context, "bool").unwrap(),
        Value::Bool(true)
    ));
    assert!(matches!(
        dry_dummy(&mut context, "integer").unwrap(),
        Value::Int(0)
    ));
    assert!(matches!(
        dry_dummy(&mut context, "number").unwrap(),
        Value::Float(0.0)
    ));
    assert!(
        matches!(dry_dummy(&mut context, "string").unwrap(), Value::Str(ref value) if value.as_bytes().is_empty())
    );
    for type_name in ["Thing", "Json", "Union"] {
        let value = dry_dummy(&mut context, type_name).unwrap();
        assert_eq!(value.dry_null_type(), Some(type_name));
    }
}

#[test]
fn collection_dummies_are_fresh_and_mutable_for_lists_and_maps() {
    let mut context = Context::new();
    let first = dry_dummy(&mut context, "IntList")
        .unwrap()
        .as_obj()
        .unwrap();
    context.get_mut::<IntList>(first).unwrap().items.push(9);
    let second = dry_dummy(&mut context, "IntList")
        .unwrap()
        .as_obj()
        .unwrap();
    assert!(context.get::<IntList>(second).unwrap().items.is_empty());
    assert_eq!(context.get::<IntList>(first).unwrap().items, [9]);

    let first = dry_dummy(&mut context, "FloatList")
        .unwrap()
        .as_obj()
        .unwrap();
    context.get_mut::<FloatList>(first).unwrap().items.push(1.5);
    let second = dry_dummy(&mut context, "FloatList")
        .unwrap()
        .as_obj()
        .unwrap();
    assert!(context.get::<FloatList>(second).unwrap().items.is_empty());

    let first = dry_dummy(&mut context, "StrList")
        .unwrap()
        .as_obj()
        .unwrap();
    context
        .get_mut::<StrList>(first)
        .unwrap()
        .items
        .push("first".into());
    let second = dry_dummy(&mut context, "StrList")
        .unwrap()
        .as_obj()
        .unwrap();
    assert!(context.get::<StrList>(second).unwrap().items.is_empty());

    let first = dry_dummy(&mut context, "IntMap").unwrap().as_obj().unwrap();
    context
        .get_mut::<IntMap>(first)
        .unwrap()
        .set("first".into(), 1);
    let second = dry_dummy(&mut context, "IntMap").unwrap().as_obj().unwrap();
    assert!(context.get::<IntMap>(second).unwrap().items.is_empty());

    let first = dry_dummy(&mut context, "FloatMap")
        .unwrap()
        .as_obj()
        .unwrap();
    context
        .get_mut::<FloatMap>(first)
        .unwrap()
        .set("first".into(), 1.0);
    let second = dry_dummy(&mut context, "FloatMap")
        .unwrap()
        .as_obj()
        .unwrap();
    assert!(context.get::<FloatMap>(second).unwrap().items.is_empty());

    let first = dry_dummy(&mut context, "StrMap").unwrap().as_obj().unwrap();
    context
        .get_mut::<StrMap>(first)
        .unwrap()
        .set("first".into(), "value".into());
    let second = dry_dummy(&mut context, "StrMap").unwrap().as_obj().unwrap();
    assert!(context.get::<StrMap>(second).unwrap().entries().is_empty());
}

#[test]
fn dry_null_is_json_null_and_still_obeys_nonnullable_result_contracts() {
    let contract =
        ResultContract::from_schema(br#"{"dsl.schema":2,"result":{"type":"int"}}"#).unwrap();
    let mut context = Context::new();
    context.result_sink = Some(ResultSink::new(contract));
    let dry_null = dry_dummy(&mut context, "Thing").unwrap();
    let error = set_result(&mut context, &dry_null).unwrap_err();
    assert!(error.message.contains("expected int; got null"), "{error}");
    assert_eq!(context.result_sink.as_ref().unwrap().json(), "null");

    let nullable =
        ResultContract::from_schema(br#"{"dsl.schema":2,"result":{"type":"int","nullable":true}}"#)
            .unwrap();
    context.result_sink = Some(ResultSink::new(nullable));
    let dry_null = dry_dummy(&mut context, "Thing").unwrap();
    set_result(&mut context, &dry_null).unwrap();
    assert_eq!(context.result_sink.as_ref().unwrap().json(), "null");
}
