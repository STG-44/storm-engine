use crate::{Host, Limits, Program, Runtime, Type, Value, Variable, World, preview::AttributeHost};
use anyhow::{Result, bail};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, rc::Rc};
use storm_core::{AttributeStore, Entity, EntityId, EntityManager, Message};

#[test]
fn retained_data_pointer_conversions_booleans_and_comparisons_match() -> Result<()> {
    use crate::value::Op;
    let fixture: Json =
        serde_json::from_str(include_str!("../../../reference/messages/ptr_cases.json"))?;
    for case in fixture["cases"].as_array().unwrap() {
        let bits = case["bits"].as_u64().unwrap();
        let initial = if case["integer"].as_bool().unwrap() {
            Value::Int(bits as i32)
        } else {
            Value::Ptr(bits)
        };
        let variable = Variable::new(initial.clone());
        let value = if case["ref"].as_bool().unwrap() {
            variable.reference()
        } else {
            initial
        };
        let result = match case["op"].as_str().unwrap() {
            "int" | "ptr" => {
                let kind = if case["op"] == "int" {
                    Type::Int
                } else {
                    Type::Ptr
                };
                if let Value::Reference(reference) = value {
                    reference.convert(kind)?
                } else {
                    value.convert(kind)?
                }
            }
            "bool" => value.bool_convert()?.resolved()?,
            "!" => value.not()?,
            "neg" => value.apply_negate()?.resolved()?,
            operation => {
                let op = match operation {
                    "==" => Op::Eq,
                    "!=" => Op::Ne,
                    "<" => Op::Lt,
                    "<=" => Op::Le,
                    ">" => Op::Gt,
                    ">=" => Op::Ge,
                    _ => unreachable!(),
                };
                value.apply_binary(
                    op,
                    Value::Int(case["right"].as_i64().unwrap() as i32),
                    &mut 1000,
                )?
            }
        };
        let expected = &case["expected"];
        let expected = if expected["kind"] == "p" {
            Value::Ptr(expected["value"].as_u64().unwrap())
        } else {
            Value::Int(expected["value"].as_i64().unwrap() as i32)
        };
        assert_eq!(result, expected, "{case}");
        if case["ref"].as_bool().unwrap() {
            assert_eq!(variable.value()?, expected, "{case}");
        }
    }
    Ok(())
}

#[derive(Default)]
struct Records {
    fields: Vec<String>,
    calls: usize,
}
struct Probe(Rc<RefCell<Records>>, u64);
impl Entity for Probe {
    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        let mut records = self.0.borrow_mut();
        records.calls += 1;
        while let Some(code) = message.current_type() {
            let value = match code {
                'l' => message.long()?.to_string(),
                'f' => message.float()?.to_bits().to_string(),
                's' => message.string()?.to_owned(),
                'p' => message.pointer()?.to_string(),
                'i' => message.entity()?.raw().to_string(),
                'a' => {
                    message.attribute()?;
                    "91".to_owned()
                }
                _ => bail!("unexpected test field"),
            };
            records.fields.push(format!("{code}:{value}"));
        }
        Ok(self.1)
    }
}
struct TestHost {
    world: World,
    entities: EntityManager,
    id: EntityId,
    records: Rc<RefCell<Records>>,
}
impl TestHost {
    fn new(result: u64) -> Result<Self> {
        let records = Rc::new(RefCell::new(Records::default()));
        let mut entities = EntityManager::default();
        let id = entities.insert("probe", Probe(records.clone(), result))?;
        Ok(Self {
            world: World::default(),
            entities,
            id,
            records,
        })
    }
}
impl Host for TestHost {
    fn call(&mut self, _: &str, _: &[Value]) -> Result<Value> {
        bail!("unbound")
    }
    fn world(&self) -> World {
        self.world.clone()
    }
    fn send_message(
        &mut self,
        entity: EntityId,
        message: &mut Message,
        store: &mut AttributeStore,
    ) -> Result<u64> {
        for field in message.values() {
            if let storm_core::Value::Attribute(id) = field {
                assert!(
                    store.get(*id).is_some(),
                    "a field must belong to this store"
                );
            }
        }
        self.entities.send_message(entity, message)
    }
}

#[test]
fn retained_cpp_message_builder_and_native_result_match_all_cases() -> Result<()> {
    let fixture: Json =
        serde_json::from_str(include_str!("../../../reference/messages/cases.json"))?;
    for case in fixture["cases"].as_array().unwrap() {
        let mut host = TestHost::new(case["result"].as_u64().unwrap())?;
        if !case["valid"].as_bool().unwrap() {
            host.entities.erase(host.id);
        }
        let mut args = vec![Value::Object(host.world.object(host.id)?)];
        let mut variables = Vec::new();
        let format = Value::String(case["format"].as_str().unwrap().into());
        if case["format_ref"].as_bool().unwrap() {
            let variable = Variable::new(format);
            args.push(variable.reference());
            variables.push(variable);
        } else {
            args.push(format);
        }
        let mut converted = Vec::new();
        for field in case["fields"].as_array().unwrap() {
            let code = field["code"].as_str().unwrap();
            let value = match code {
                "f" if field["float"] == true => {
                    Value::Float(field["value"].as_f64().unwrap() as f32)
                }
                "l" | "f" => Value::Int(field["value"].as_i64().unwrap() as i32),
                "p" => Value::Ptr(field["value"].as_u64().unwrap()),
                "s" => Value::String(field["value"].as_str().unwrap().into()),
                "i" => Value::Object(
                    host.world
                        .object(EntityId::from_raw(field["value"].as_u64().unwrap()))?,
                ),
                "a" => Value::Object(host.world.object(EntityId::INVALID)?),
                _ => unreachable!(),
            };
            if field["ref"].as_bool().unwrap() {
                let variable = Variable::new(value);
                args.push(variable.reference());
                if code == "f" {
                    converted.push(variable.clone());
                }
                variables.push(variable);
            } else {
                args.push(value);
            }
        }
        if case["extra"].as_bool().unwrap() {
            args.push(Value::Int(99));
        }
        let mut runtime = Runtime::compile("void main(){}")?;
        let result = runtime.call("SendMessage", &args, &mut host, Limits::default())?;
        let expected = &case["expected"];
        assert_eq!(
            result,
            Value::Ptr(expected["result"].as_u64().unwrap()),
            "{case}"
        );
        let records = host.records.borrow();
        assert_eq!(json!(records.fields), expected["fields"], "{case}");
        assert_eq!(json!(records.calls), expected["calls"], "{case}");
        for variable in converted {
            assert_eq!(variable.value()?.kind(), Type::Float);
        }
    }
    Ok(())
}

#[test]
fn source_fixture_preserves_full_width_results_and_skips_pending_entities() -> Result<()> {
    let mut runtime = Runtime::compile(include_str!("../../../fixtures/entity_messages.c"))?;
    let mut host = AttributeHost::new()?;
    let result = runtime.call("main", &[], &mut host, Limits::default())?;
    assert_eq!(result, Value::Ptr(0xfedc_ba98_7654_3210));
    let report = host.report(&result);
    assert_eq!(
        report["entity_messages"],
        json!({"total":3,"recent":[
        {"format":"lfsiap","result":0xfedc_ba98_7654_3210_u64},
        {"format":"p","result":0xfedc_ba98_7654_3210_u64},
        {"format":"","result":0}]})
    );
    let children = report["script_ship"]["children"].as_array().unwrap();
    assert_eq!(
        children
            .iter()
            .map(|n| (&n["name"], &n["value"]))
            .collect::<Vec<_>>(),
        vec![
            (&json!("index"), &json!("19")),
            (&json!("number"), &json!("7.0000000")),
            (&json!("low_bits"), &json!("1985229328")),
            (&json!("deleted_result"), &json!("0"))
        ]
    );
    assert_eq!(report["ship_attribute_notifications"], 1);
    Ok(())
}

#[test]
fn ptr_storage_conversions_and_integer_returns_keep_the_original_bits() -> Result<()> {
    let mut runtime = Runtime::compile(
        r#"
        ptr saved; ptr slots[2];
        ptr main(ptr p){saved=p;slots[1]=saved;return slots[1];}
        int low(){return saved;}
        int negative(){ptr p;p=-1;return p;}
        ptr copy(){return -saved;}
        int boolean(){return saved&&1;}
        int truncated_not(){return !saved;}
        int compare(){return saved==0;}
    "#,
    )?;
    let bits = 0x8000_0000_0000_0000;
    assert_eq!(
        runtime.call("main", &[Value::Ptr(bits)], &mut (), Limits::default())?,
        Value::Ptr(bits)
    );
    for (entry, expected) in [
        ("low", 0),
        ("negative", -1),
        ("boolean", 1),
        ("truncated_not", 1),
        ("compare", 1),
    ] {
        assert_eq!(
            runtime.call(entry, &[], &mut (), Limits::default())?,
            Value::Int(expected),
            "{entry}"
        );
    }
    assert_eq!(
        runtime.call("copy", &[], &mut (), Limits::default())?,
        Value::Ptr(bits)
    );
    Ok(())
}

#[test]
fn invalid_payloads_fail_before_native_dispatch_with_source_locations() -> Result<()> {
    for (setup, expression) in [
        ("", "SendMessage(target,\"l\",1.0)"),
        ("", "SendMessage(target,\"p\",1)"),
        ("", "SendMessage(target,\"f\",\"7\")"),
        ("", "SendMessage(target,\"i\",1)"),
        ("", "SendMessage(target,\"a\",1)"),
        (
            "string text;ref r;makeref(r,text);",
            "SendMessage(target,\"s\",r)",
        ),
        ("", "SendMessage(target,\"ll\",1)"),
        ("", "SendMessage(target,\"e\",1)"),
        ("", "SendMessage(target,\"b\",1)"),
        ("", "SendMessage(1,\"\")"),
        ("aref a;", "SendMessage(target,\"a\",a)"),
        ("ref r;", "SendMessage(target,\"i\",r)"),
    ] {
        let mut host = TestHost::new(u64::MAX)?;
        let mut runtime =
            Runtime::compile(&format!("void main(object target){{{setup}{expression};}}"))?;
        let object = host.world.object(host.id)?;
        let error = runtime
            .call(
                "main",
                &[Value::Object(object)],
                &mut host,
                Limits::default(),
            )
            .unwrap_err();
        assert!(format!("{error:#}").contains("main <source>:"), "{error:#}");
        assert_eq!(host.records.borrow().calls, 0);
    }
    Ok(())
}

#[test]
fn building_invalid_destinations_still_converts_refs_and_validates_fields() -> Result<()> {
    let mut host = TestHost::new(u64::MAX)?;
    host.entities.erase(host.id);
    let variable = Variable::new(Value::Int(7));
    let args = [
        Value::Object(host.world.object(host.id)?),
        Value::String("fl".into()),
        variable.reference(),
        Value::String("bad".into()),
    ];
    let mut runtime = Runtime::compile("void main(){}")?;
    assert!(
        runtime
            .call("SendMessage", &args, &mut host, Limits::default())
            .is_err()
    );
    assert_eq!(variable.value()?, Value::Float(7.0));
    assert_eq!(host.records.borrow().calls, 0);
    Ok(())
}

#[test]
fn foreign_expired_and_deleted_attribute_handles_are_rejected_before_dispatch() -> Result<()> {
    let mut host = TestHost::new(0)?;
    let destination = Value::Object(host.world.object(host.id)?);
    let foreign = World::default();
    let object = foreign.object(EntityId::INVALID)?;
    let expired = {
        let temporary = host.world.object(EntityId::INVALID)?;
        temporary.root()
    };
    let local = host.world.object(EntityId::INVALID)?;
    let deleted = local.root().create_path("child")?;
    local
        .root()
        .builtin("deleteattribute", Some(&Value::String("child".into())))?;
    for value in [
        Value::Object(object),
        Value::AReference(Some(expired)),
        Value::AReference(Some(deleted)),
    ] {
        let args = [destination.clone(), Value::String("a".into()), value];
        let mut runtime = Runtime::compile("void main(){}")?;
        assert!(
            runtime
                .call("SendMessage", &args, &mut host, Limits::default())
                .is_err()
        );
    }
    assert_eq!(host.records.borrow().calls, 0);
    let selected = local.root().create_path("selected")?;
    let args = [
        destination,
        Value::String("ia".into()),
        Value::AReference(Some(selected.clone())),
        Value::AReference(Some(selected)),
    ];
    let mut runtime = Runtime::compile("void main(){}")?;
    assert_eq!(
        runtime.call("SendMessage", &args, &mut host, Limits::default())?,
        Value::Ptr(0)
    );
    assert_eq!(host.records.borrow().fields, ["i:0", "a:91"]);
    Ok(())
}

#[test]
fn native_messages_can_update_the_checked_attribute_store_and_errors_propagate() -> Result<()> {
    struct Native(World);
    impl Host for Native {
        fn world(&self) -> World {
            self.0.clone()
        }
        fn call(&mut self, _: &str, _: &[Value]) -> Result<Value> {
            bail!("unbound")
        }
        fn send_message(
            &mut self,
            _: EntityId,
            m: &mut Message,
            s: &mut AttributeStore,
        ) -> Result<u64> {
            let root = m.attribute()?;
            s.set_attribute(root, "native", Some("19"))?;
            bail!("native message failure")
        }
    }
    let world = World::default();
    let object = world.object(EntityId::INVALID)?;
    let variable = Variable::new(Value::Object(object.clone()));
    let mut host = Native(world);
    let mut runtime = Runtime::compile("void main(ref o){SendMessage(o,\"a\",o);}")?;
    let error = runtime
        .call(
            "main",
            &[variable.reference()],
            &mut host,
            Limits::default(),
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("native message failure"));
    assert_eq!(object.root().create_path("native")?.scalar_value()?, "19");
    Ok(())
}

#[test]
fn message_binding_arity_and_format_limits_are_explicit() -> Result<()> {
    for source in [
        "void main(){SendMessage();}",
        "void main(){object o;SendMessage(o);}",
        "native ptr SendMessage(object o,string f);",
    ] {
        assert!(Program::compile(source).is_err());
    }
    let mut host = TestHost::new(0)?;
    let object = Value::Object(host.world.object(host.id)?);
    let mut runtime = Runtime::compile("void main(){}")?;
    let mut args = vec![object, Value::String("l".repeat(129))];
    args.extend(vec![Value::Int(0); 129]);
    assert!(
        runtime
            .call("SendMessage", &args, &mut host, Limits::default())
            .is_err()
    );
    assert_eq!(host.records.borrow().calls, 0);
    let error = Program::compile("ptr main(){object o;return SendMessage(o,\"\");}")?
        .call("main", &[], &mut (), Limits::default())
        .unwrap_err();
    assert!(format!("{error:#}").contains("messages are not bound"));
    Ok(())
}
