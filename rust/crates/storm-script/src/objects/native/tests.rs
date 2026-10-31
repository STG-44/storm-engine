use super::*;
use crate::{Host, Limits, Runtime, preview::AttributeHost};
use serde_json::{Value as Json, json};
use storm_core::Message;

type Callback = Box<dyn FnMut(&mut Message, &mut AttributeStore) -> Result<u64>>;
struct NativeHost {
    world: World,
    callback: Callback,
}
impl Host for NativeHost {
    fn world(&self) -> World {
        self.world.clone()
    }
    fn call(&mut self, _: &str, _: &[Value]) -> Result<Value> {
        bail!("unbound native")
    }
    fn send_message(
        &mut self,
        _: EntityId,
        m: &mut Message,
        s: &mut AttributeStore,
    ) -> Result<u64> {
        (self.callback)(m, s)
    }
}
fn run_main(runtime: &mut Runtime, host: &mut NativeHost) -> Result<Value> {
    let object = host.world.object(EntityId::from_raw(4294967296))?;
    runtime.call("main", &[Value::Object(object)], host, Limits::default())
}
fn snapshot(cell: &ScriptVariable, store: &AttributeStore) -> Result<String> {
    use ScriptVariableKind as K;
    Ok(match cell.kind()? {
        K::Int => format!("i:{}", cell.integer()?),
        K::Float => format!("f:{}", cell.float()?.to_bits()),
        K::String => format!("s:{}", cell.string()?),
        K::Ptr => format!("p:{}", cell.pointer()?),
        K::Object => format!("o:{}", cell.entity(store)?.raw()),
        K::AttributeReference => format!("a:{}", cell.entity(store)?.raw()),
        _ => bail!("unexpected corpus cell kind"),
    })
}
fn caller_snapshot(variable: &Variable) -> Result<String> {
    Ok(match variable.value()? {
        Value::Int(n) => format!("i:{n}"),
        Value::Float(n) => format!("f:{}", n.to_bits()),
        Value::String(s) => format!("s:{s}"),
        Value::Ptr(n) => format!("p:{n}"),
        Value::Object(o) => format!("o:{}", o.entity().raw()),
        Value::AReference(Some(a)) => format!("a:{}", a.entity()?.raw()),
        _ => bail!("unexpected corpus caller kind"),
    })
}

#[test]
fn retained_e_binding_native_setters_and_aliases_match_all_cpp_cases() -> Result<()> {
    let corpus: Json = serde_json::from_str(include_str!(
        "../../../reference/messages/variable_cases.json"
    ))?;
    for case in corpus["cases"].as_array().unwrap() {
        let world = World::default();
        let id = EntityId::from_raw(9920249030594524246);
        let owner = world.object(id)?;
        let initial = match case["initial"].as_str().unwrap() {
            "i" => Value::Int(7),
            "f" => Value::Float(7.25),
            "s" => Value::String("before".into()),
            "p" => Value::Ptr(18364758544493064720),
            "o" => Value::Object(owner.clone()),
            "a" => Value::AReference(Some(owner.root())),
            _ => unreachable!(),
        };
        let variable = Variable::new(initial);
        let alias = Variable::new(variable.reference());
        let arguments = if case["ref"] == true {
            vec![alias.reference(), alias.reference()]
        } else {
            (0..2)
                .map(|_| {
                    Ok(match variable.value()? {
                        Value::Object(o) => Value::Object(o.copied()?),
                        value => value,
                    })
                })
                .collect::<Result<Vec<_>>>()?
        };
        let observed = Rc::new(RefCell::new(
            json!({"result":0,"calls":0,"same":false,"before":"-","first":"-","second":"-"}),
        ));
        let records = observed.clone();
        let write = case["write"].as_str().unwrap().to_owned();
        let valid = case["valid"] == true;
        let mut host = NativeHost {
            world: world.clone(),
            callback: Box::new(move |message, store| {
                if !valid {
                    return Ok(0);
                }
                let first = message.script_variable()?;
                let second = message.script_variable()?;
                let before = snapshot(&first, store)?;
                let same = first == second;
                match write.as_str() {
                    "i" => first.set(ScriptScalar::Int(-19))?,
                    "f" => first.set(ScriptScalar::Float(3.25))?,
                    "s" => first.set(ScriptScalar::String("after".into()))?,
                    "p" => first.set(ScriptScalar::Ptr(u64::MAX))?,
                    "o" => first.set_entity(EntityId::from_raw(18364758544493064720), store)?,
                    _ => unreachable!(),
                }
                *records.borrow_mut() = json!({"result":18364758544493064720_u64,"calls":1,"same":same,
                "before":before,"first":snapshot(&first,store)?,"second":snapshot(&second,store)?});
                Ok(18364758544493064720)
            }),
        };
        let mut args = vec![
            Value::Object(world.object(EntityId::from_raw(4294967296))?),
            Value::String("ee".into()),
        ];
        args.extend(arguments);
        let mut runtime = Runtime::compile("void main(){}")?;
        let result = runtime.call("SendMessage", &args, &mut host, Limits::default())?;
        assert_eq!(
            result,
            Value::Ptr(case["expected"]["result"].as_u64().unwrap()),
            "{case}"
        );
        observed.borrow_mut()["caller"] = json!(caller_snapshot(&variable)?);
        assert_eq!(*observed.borrow(), case["expected"], "{case}");
    }
    Ok(())
}

#[test]
fn native_source_fixture_retypes_outputs_fills_arrays_and_retains_a_global() -> Result<()> {
    let mut runtime = Runtime::compile(include_str!("../../../fixtures/variable_messages.c"))?;
    let mut host = AttributeHost::new()?;
    let result = runtime.call("main", &[], &mut host, Limits::default())?;
    assert_eq!(result, Value::Int(19));
    let report = host.report(&result);
    let fields = report["script_ship"]["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| (n["name"].as_str().unwrap(), n["value"].as_str().unwrap()))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        fields,
        std::collections::BTreeMap::from([
            ("index", "19"),
            ("native_value", "19"),
            ("native_wind", "0.5000000"),
            ("native_text", "парус"),
            ("native_ptr_low", "1985229328"),
            ("array_total", "60"),
            ("array_count", "3"),
            ("temporary", "99"),
            ("kept", "19"),
            ("deleted_value", "19")
        ])
    );
    assert_eq!(report["ship_attribute_notifications"], 1);
    assert_eq!(
        report["entity_messages"],
        json!({"total":5,"recent":[
        {"format":"leeee","result":18364758544493064720_u64},{"format":"lee","result":0},
        {"format":"le","result":88},{"format":"le","result":7},{"format":"l","result":19}]})
    );
    Ok(())
}

#[test]
fn native_scalar_setters_retype_refs_but_plain_arguments_are_stack_copies() -> Result<()> {
    let mut host = NativeHost {
        world: World::default(),
        callback: Box::new(|m, _| {
            m.script_variable()?.set(ScriptScalar::Float(3.25))?;
            Ok(0)
        }),
    };
    let mut runtime = Runtime::compile(
        r#"int main(object target){int n;n=7;
        SendMessage(target,"e",n);if(n!=7)return -1;
        SendMessage(target,"e",&n);return n*4;}"#,
    )?;
    // n became float, so n*4 cannot be returned by an int function.
    assert!(
        format!("{:#}", run_main(&mut runtime, &mut host).unwrap_err())
            .contains("return type mismatch")
    );
    let mut runtime = Runtime::compile(
        r#"float main(object target){int n;n=7;
        SendMessage(target,"e",n);if(n!=7)return -1.0;
        SendMessage(target,"e",&n);return n;}"#,
    )?;
    assert_eq!(run_main(&mut runtime, &mut host)?, Value::Float(3.25));
    Ok(())
}

#[test]
fn e_capture_follows_get_var_pointer_once_and_preserves_alias_identity() -> Result<()> {
    let world = World::default();
    let original = Variable::new(Value::Int(7));
    let replacement = Variable::new(Value::Int(99));
    let alias = Variable::new(original.reference());
    let mut arguments = NativeArguments::new(&world);
    let first = arguments.capture(&alias.reference())?;
    let second = arguments.capture(&original.reference())?;
    assert_eq!(first, second);
    alias.bind(&replacement)?;
    first.set(ScriptScalar::Int(19))?;
    assert_eq!(second.integer()?, 19);
    assert_eq!(original.value()?, Value::Int(19));
    assert_eq!(alias.value()?, Value::Int(99));
    let mut runtime = Runtime::compile("void main(object o,int x){SendMessage(o,\"ef\",&x,&x);}")?;
    let mut host = NativeHost {
        world,
        callback: Box::new(|m, _| {
            assert_eq!(m.script_variable()?.float()?, 7.0);
            assert_eq!(m.float()?, 7.0);
            Ok(0)
        }),
    };
    let object = host.world.object(EntityId::from_raw(4294967296))?;
    runtime.call(
        "main",
        &[Value::Object(object), Value::Int(7)],
        &mut host,
        Limits::default(),
    )?;
    Ok(())
}

#[test]
fn retained_handles_observe_local_global_temporary_and_runtime_lifetimes() -> Result<()> {
    let saved = Rc::new(RefCell::new(Vec::<ScriptVariable>::new()));
    let records = saved.clone();
    let mut host = NativeHost {
        world: World::default(),
        callback: Box::new(move |m, _| {
            while m.param_valid() {
                records.borrow_mut().push(m.script_variable()?);
            }
            Ok(0)
        }),
    };
    let mut runtime = Runtime::compile(
        r#"int global;
        void main(object target){int local;local=3;global=7;SendMessage(target,"eee",&local,&global,99);}
        int check(){return global;}"#,
    )?;
    run_main(&mut runtime, &mut host)?;
    let cells = saved.borrow();
    assert!(cells[0].read().is_err());
    assert!(cells[2].read().is_err());
    assert_eq!(cells[1].integer()?, 7);
    cells[1].set(ScriptScalar::Int(19))?;
    assert_eq!(
        runtime.call("check", &[], &mut host, Limits::default())?,
        Value::Int(19)
    );
    drop(runtime);
    assert!(cells[1].read().is_err());
    Ok(())
}

#[test]
fn array_views_are_bounded_and_removed_cells_expire_without_keeping_arrays_alive() -> Result<()> {
    let saved = Rc::new(RefCell::new(Vec::<ScriptVariable>::new()));
    let records = saved.clone();
    let mut host = NativeHost {
        world: World::default(),
        callback: Box::new(move |m, _| {
            let array = m.script_variable()?;
            assert_eq!(array.elements()?, 3);
            assert!(array.read().is_err());
            assert!(array.set(ScriptScalar::Int(1)).is_err());
            assert!(array.element(-1).is_err());
            assert!(array.element(3).is_err());
            let zero = array.element(0)?;
            let last = array.element(2)?;
            last.set(ScriptScalar::Int(19))?;
            records.borrow_mut().extend([array, zero, last]);
            Ok(0)
        }),
    };
    let mut runtime = Runtime::compile(
        r#"int values[3];void main(object target){SendMessage(target,"e",&values);}
        void shrink(){SetArraySize(&values,1);}int check(){return values[0];}"#,
    )?;
    run_main(&mut runtime, &mut host)?;
    runtime.call("shrink", &[], &mut host, Limits::default())?;
    let cells = saved.borrow();
    assert_eq!(cells[0].elements()?, 1);
    assert!(cells[2].read().is_err());
    cells[1].set(ScriptScalar::Int(7))?;
    assert_eq!(
        runtime.call("check", &[], &mut host, Limits::default())?,
        Value::Int(7)
    );
    drop(runtime);
    assert!(cells.iter().all(|c| c.elements().is_err()));
    Ok(())
}

#[test]
fn entity_outputs_preserve_trees_aref_snapshots_and_complete_identifiers() -> Result<()> {
    let world = World::default();
    let object = world.object(EntityId::from_raw(4294967296))?;
    object.root().create_path("old")?.set_value("9")?;
    let variable = Variable::new(Value::Object(object.clone()));
    let aref = Variable::new(Value::AReference(Some(object.root().create_path("child")?)));
    let old_aref = aref.value()?;
    let scalar = Variable::new(Value::Int(0));
    let mut args = NativeArguments::new(&world);
    let o = args.capture(&variable.reference())?;
    let a = args.capture(&aref.reference())?;
    let output = args.capture(&scalar.reference())?;
    let id = EntityId::from_raw(0xfedc_ba98_7654_3210);
    world.initialize(None, |store, _| {
        let before = o.attributes(store)?;
        o.set_entity(id, store)?;
        assert_eq!(o.attributes(store)?, before);
        assert_eq!(o.entity(store)?, id);
        a.set_entity(id, store)?;
        output.set_entity(id, store)?;
        let root = output.attributes(store)?;
        store.set_attribute(root, "native", Some("19"))?;
        Ok(())
    })?;
    assert_eq!(object.entity(), id);
    assert_eq!(object.root().create_path("old")?.value()?, "9");
    assert_eq!(aref.value()?.attributes()?.entity()?, id);
    assert_eq!(
        old_aref.attributes()?.entity()?,
        EntityId::from_raw(4294967296)
    );
    assert_eq!(
        scalar
            .value()?
            .attributes()?
            .create_path("native")?
            .value()?,
        "19"
    );
    Ok(())
}

#[test]
fn foreign_worlds_stores_and_removed_attributes_fail_without_borrow_panics() -> Result<()> {
    let world = World::default();
    let other = World::default();
    let foreign = other.object(EntityId::INVALID)?;
    let mut args = NativeArguments::new(&world);
    assert!(args.capture(&Value::Object(foreign)).is_err());
    let mut foreign_runtime = Runtime::compile("int values[2];ref get(){return &values;}")?;
    let mut other_host = NativeHost {
        world: other,
        callback: Box::new(|_, _| Ok(0)),
    };
    let array = foreign_runtime.call("get", &[], &mut other_host, Limits::default())?;
    assert!(args.capture(&array).is_err());
    let object = world.object(EntityId::INVALID)?;
    let child = object.root().create_path("child")?;
    let aref = Variable::new(Value::AReference(Some(child)));
    let cell = args.capture(&aref.reference())?;
    assert!(cell.attributes(&AttributeStore::default()).is_err());
    object
        .root()
        .builtin("deleteattribute", Some(&Value::String("child".into())))?;
    world.initialize(None, |store, _| {
        assert!(cell.attributes(store).is_err());
        assert!(cell.entity(store).is_err());
        Ok(())
    })?;
    Ok(())
}

#[test]
fn scalar_limits_reentrant_access_and_nan_errors_keep_original_side_effects() -> Result<()> {
    let world = World::default();
    let variable = Variable::new(Value::Int(7));
    let mut args = NativeArguments::new(&world);
    let cell = args.capture(&variable.reference())?;
    {
        let _borrow = variable.0.borrow_mut();
        assert!(cell.read().is_err());
        assert!(cell.set(ScriptScalar::Int(1)).is_err());
    }
    assert!(cell.set(ScriptScalar::String("x".repeat(4097))).is_err());
    assert!(cell.set(ScriptScalar::String("x\0y".into())).is_err());
    assert_eq!(cell.integer()?, 7);
    assert!(cell.float().is_err());
    assert!(cell.pointer().is_err());
    assert!(cell.string().is_err());
    assert!(cell.element(0).is_err());
    assert!(cell.set(ScriptScalar::Float(f32::NAN)).is_err());
    assert!(cell.float()?.is_nan());
    cell.set(ScriptScalar::Float(f32::INFINITY))?;
    assert_eq!(cell.float()?, f32::INFINITY);
    Ok(())
}

#[test]
fn native_errors_keep_reference_writes_and_expire_copied_stack_cells() -> Result<()> {
    let saved = Rc::new(RefCell::new(Vec::<ScriptVariable>::new()));
    let records = saved.clone();
    let mut host = NativeHost {
        world: World::default(),
        callback: Box::new(move |m, _| {
            let reference = m.script_variable()?;
            let temporary = m.script_variable()?;
            reference.set(ScriptScalar::Int(19))?;
            records.borrow_mut().extend([reference, temporary]);
            bail!("native e failure")
        }),
    };
    let mut runtime = Runtime::compile(
        r#"int global;void main(object target){SendMessage(target,"ee",&global,99);}
        int check(){return global;}"#,
    )?;
    let error = run_main(&mut runtime, &mut host).unwrap_err();
    assert!(format!("{error:#}").contains("main <source>:"));
    assert!(format!("{error:#}").contains("native e failure"));
    assert_eq!(
        runtime.call("check", &[], &mut host, Limits::default())?,
        Value::Int(19)
    );
    assert_eq!(saved.borrow()[0].integer()?, 19);
    assert!(saved.borrow()[1].read().is_err());
    Ok(())
}
