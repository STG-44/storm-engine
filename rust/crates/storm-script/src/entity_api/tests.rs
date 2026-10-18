use crate::{Host, Limits, Program, Runtime, Value, Variable, World, preview::AttributeHost};
use anyhow::{Result, bail};
use std::{cell::Cell, rc::Rc};
use storm_core::{AttributeId, AttributeStore, Entity, EntityId, EntityManager};

struct Observer(Rc<Cell<usize>>);
impl Entity for Observer {
    fn attribute_changed(&mut self, _: &AttributeStore, _: AttributeId) -> Result<u32> {
        self.0.set(self.0.get() + 1);
        Ok(0)
    }
}
struct TestHost {
    world: World,
    ship: Variable,
    entities: EntityManager,
    id: EntityId,
    notifications: Rc<Cell<usize>>,
}
impl TestHost {
    fn new() -> Result<Self> {
        let world = World::default();
        let notifications = Rc::new(Cell::new(0));
        let mut entities = EntityManager::default();
        let id = entities.insert("observer", Observer(notifications.clone()))?;
        let object = world.object(id)?;
        object
            .root()
            .inspect(|store, root| entities.bind_attributes(id, store, Some(root)))??;
        Ok(Self {
            world,
            ship: Variable::new(Value::Object(object)),
            entities,
            id,
            notifications,
        })
    }
}
impl Host for TestHost {
    fn call(&mut self, name: &str, _: &[Value]) -> Result<Value> {
        if name == "ship" {
            Ok(self.ship.reference())
        } else {
            bail!("unexpected native {name}")
        }
    }
    fn world(&self) -> World {
        self.world.clone()
    }
    fn is_entity(&self, id: EntityId) -> Result<bool> {
        Ok(self.entities.is_valid(id))
    }
    fn delete_entity(&mut self, id: EntityId) -> Result<()> {
        self.entities.erase(id);
        Ok(())
    }
    fn attribute_changed(
        &mut self,
        id: EntityId,
        store: &AttributeStore,
        changed: AttributeId,
    ) -> Result<()> {
        self.entities.notify_attribute_changed(id, store, changed)?;
        Ok(())
    }
}
fn runtime(source: &str) -> Result<Runtime> {
    Runtime::compile(&format!("native ref Ship(); {source}"))
}
fn call(runtime: &mut Runtime, name: &str, host: &mut impl Host) -> Result<Value> {
    runtime.call(name, &[], host, Limits::default())
}

#[test]
fn lookup_and_delete_preserve_full_ids_and_defer_native_destruction() -> Result<()> {
    let mut host = TestHost::new()?;
    assert!(host.id.raw() > u64::from(u32::MAX));
    let root = host.entities.attribute_root(host.id);
    let mut vm = runtime(
        r#"int main(){ref s; s=Ship(); object copy; copy=s;
            if(IsEntity(copy)!=1)return -1;
            DeleteClass(copy); DeleteClass(s);
            s.after=19; return IsEntity(s)*10+IsEntity(copy);}
        int check(){return IsEntity(Ship());}"#,
    )?;
    assert_eq!(call(&mut vm, "main", &mut host)?, Value::Int(0));
    assert_eq!(host.notifications.get(), 1); // Teardown lookup still works.
    assert!(host.entities.get(host.id).is_some());
    assert_eq!(host.entities.attribute_root(host.id), root);
    assert_eq!(
        host.ship
            .value()?
            .attributes()?
            .walk(&[("after".into(), false)], false)?
            .value()?,
        "19"
    );
    host.entities.new_lifecycle();
    assert!(host.entities.get(host.id).is_none());
    let new = host
        .entities
        .insert("observer", Observer(host.notifications.clone()))?;
    assert_eq!(new.raw() as u32, host.id.raw() as u32);
    assert_ne!(new, host.id);
    assert_eq!(call(&mut vm, "check", &mut host)?, Value::Int(0));
    host.entities.erase(host.id);
    assert!(host.entities.is_valid(new));
    Ok(())
}

#[test]
fn refs_array_elements_arefs_and_enumeration_keep_their_entity_identity() -> Result<()> {
    let mut host = TestHost::new()?;
    let mut vm = runtime(
        r#"int probe(ref p){return IsEntity(p);}
        int main(){ref s;s=Ship();object list[2];list[1]=s;
            ref a,b;makeref(a,list);makeref(b,a); s.child=1;
            aref root,child;makearef(root,s);makearef(child,s.child);
            if(probe(b[1])!=1 || IsEntity(list[1])!=1)return -1;
            if(IsEntity(root)!=1 || IsEntity(child)!=1)return -2;
            aref enumerated;enumerated=GetAttributeN(root,0);
            if(IsEntity(enumerated)!=0)return -3;
            DeleteClass(enumerated);if(IsEntity(s)!=1)return -4;
            DeleteClass(child);return IsEntity(root);}"#,
    )?;
    assert_eq!(call(&mut vm, "main", &mut host)?, Value::Int(0));
    Ok(())
}

#[test]
fn suppression_persists_across_calls_nested_events_and_posted_frames() -> Result<()> {
    let mut host = TestHost::new()?;
    let mut vm = runtime(
        r#"void write(){ref s;s=Ship();s.hd=548;}
        void later(){write();EntityUpdate(-3);write();}
        void main(){SetEventHandler("Now","write",0);
            SetEventHandler("Later","later",0);EntityUpdate(0);
            Event("Now");PostEvent("Later",0);}
        void enable(){EntityUpdate(1);}"#,
    )?;
    call(&mut vm, "main", &mut host)?;
    call(&mut vm, "write", &mut host)?;
    assert_eq!(host.notifications.get(), 0);
    assert_eq!(
        host.ship
            .value()?
            .attributes()?
            .walk(&[("hd".into(), false)], false)?
            .value()?,
        "548"
    );
    assert_eq!(vm.advance_frame(0, &mut host, Limits::default())?, 0);
    assert_eq!(vm.advance_frame(0, &mut host, Limits::default())?, 1);
    assert_eq!(host.notifications.get(), 1);
    call(&mut vm, "write", &mut host)?;
    assert_eq!(host.notifications.get(), 2);
    Ok(())
}

#[test]
fn event_handler_can_disable_notifications_in_the_calling_function() -> Result<()> {
    let mut host = TestHost::new()?;
    let mut vm = runtime(
        r#"void off(){EntityUpdate(0);}
        void main(){ref s;s=Ship();SetEventHandler("Off","off",0);
            s.a=1;Event("Off");s.b=2;EntityUpdate(7);s.c=3;}"#,
    )?;
    call(&mut vm, "main", &mut host)?;
    assert_eq!(host.notifications.get(), 2);
    assert_eq!(
        host.ship
            .value()?
            .attributes()?
            .walk(&[("b".into(), false)], false)?
            .value()?,
        "2"
    );
    Ok(())
}

#[test]
fn fresh_program_calls_and_independent_runtimes_start_enabled() -> Result<()> {
    let source =
        "native ref Ship(); void off(){EntityUpdate(0);} void write(){ref s;s=Ship();s.a=1;}";
    let mut host = TestHost::new()?;
    let program = Program::compile(source)?;
    program.call("off", &[], &mut host, Limits::default())?;
    program.call("write", &[], &mut host, Limits::default())?;
    let mut first = Runtime::compile(source)?;
    call(&mut first, "off", &mut host)?;
    call(&mut first, "write", &mut host)?;
    let mut second = Runtime::compile(source)?;
    call(&mut second, "write", &mut host)?;
    assert_eq!(host.notifications.get(), 2);
    Ok(())
}

#[test]
fn notification_switch_survives_errors_and_clear_events_without_rollback() -> Result<()> {
    let mut host = TestHost::new()?;
    let mut vm = runtime(
        r#"void fail(){EntityUpdate(0);ref s;s=Ship();s.a=1;int n=1/0;}
        void write(){ref s;s=Ship();s.b=2;}
        void on(){EntityUpdate(1);}"#,
    )?;
    assert!(call(&mut vm, "fail", &mut host).is_err());
    vm.clear_events()?;
    call(&mut vm, "write", &mut host)?;
    assert_eq!(host.notifications.get(), 0);
    call(&mut vm, "on", &mut host)?;
    call(&mut vm, "write", &mut host)?;
    assert_eq!(host.notifications.get(), 1);
    Ok(())
}

#[test]
fn integer_refs_are_read_without_conversion_and_other_types_are_rejected() -> Result<()> {
    let mut host = TestHost::new()?;
    let mut vm = runtime(
        r#"int main(){int n=-19;ref r;makeref(r,n);EntityUpdate(r);return n;}
        void off(){EntityUpdate(0);} void bad(){EntityUpdate("1");}
        void write(){ref s;s=Ship();s.a=1;}"#,
    )?;
    assert_eq!(call(&mut vm, "main", &mut host)?, Value::Int(-19));
    call(&mut vm, "off", &mut host)?;
    assert!(
        format!("{:#}", call(&mut vm, "bad", &mut host).unwrap_err()).contains("requires integer")
    );
    call(&mut vm, "write", &mut host)?;
    assert_eq!(host.notifications.get(), 0);
    for argument in ["1.0", "\"0\"", "Ship()"] {
        let mut bad = runtime(&format!("void main(){{EntityUpdate({argument});}}"))?;
        assert!(call(&mut bad, "main", &mut host).is_err());
    }
    Ok(())
}

#[test]
fn invalid_aliases_and_types_fail_with_locations_without_deleting_the_entity() -> Result<()> {
    for body in [
        "return IsEntity(1);",
        "DeleteClass(1.0);return 0;",
        "ref r;return IsEntity(r);",
        "aref a;DeleteClass(a);return 0;",
        "ref s;s=Ship();s.old=1;aref a;makearef(a,s.old);DeleteAttribute(s,\"old\");return IsEntity(a);",
        "object list[2];ref a;makeref(a,list);return IsEntity(a);",
    ] {
        let mut host = TestHost::new()?;
        let mut vm = runtime(&format!("int main(){{{body}}}"))?;
        let error = format!("{:#}", call(&mut vm, "main", &mut host).unwrap_err());
        assert!(error.contains("main <source>:"), "{error}");
        assert!(host.entities.is_valid(host.id));
    }
    Ok(())
}

#[test]
fn missing_entity_hooks_fail_explicitly_and_builtins_check_arity_and_redeclarations() -> Result<()>
{
    for source in [
        "int main(){object o;return IsEntity(o);}",
        "void main(){object o;DeleteClass(o);}",
    ] {
        let error = format!(
            "{:#}",
            Program::compile(source)?
                .call("main", &[], &mut (), Limits::default())
                .unwrap_err()
        );
        assert!(error.contains("not bound by this host"));
    }
    for source in [
        "int main(){return IsEntity();}",
        "void main(){DeleteClass();}",
        "void main(){EntityUpdate(0,1);}",
        "native int IsEntity(object o);",
        "void DeleteClass(object o){}",
        "void ENTITYUPDATE(int x){}",
    ] {
        assert!(Program::compile(source).is_err(), "{source}");
    }
    Ok(())
}

#[test]
fn suppressed_writes_still_convert_ref_values_and_reject_invalid_attributes() -> Result<()> {
    let mut host = TestHost::new()?;
    let mut vm = runtime(
        r#"string main(){EntityUpdate(0);ref s;s=Ship();int n=19;ref r;
            makeref(r,n);s.a=r;return n;}
        void bad(){ref s;s=Ship();aref a;makearef(a,s.old);
            DeleteAttribute(s,"old");a.value=1;}"#,
    )?;
    assert_eq!(
        call(&mut vm, "main", &mut host)?,
        Value::String("19".into())
    );
    assert_eq!(host.notifications.get(), 0);
    assert!(call(&mut vm, "bad", &mut host).is_err());
    Ok(())
}

#[test]
fn standalone_entity_fixture_exercises_real_preview_manager_bindings() -> Result<()> {
    let mut host = AttributeHost::new()?;
    let mut vm = Runtime::compile(include_str!("../../fixtures/entity_lifecycle.c"))?;
    assert_eq!(call(&mut vm, "main", &mut host)?, Value::Int(19));
    assert_eq!(vm.advance_frame(0, &mut host, Limits::default())?, 0);
    assert_eq!(vm.advance_frame(0, &mut host, Limits::default())?, 1);
    let report = host.report(&Value::Int(19));
    assert_eq!(report["ship_attribute_notifications"], 3);
    assert_eq!(
        report["ship_attribute_changes"],
        serde_json::json!([
            {"name":"index","value":"19"},
            {"name":"hd","value":"548"},
            {"name":"deleted","value":"1"}
        ])
    );
    let ship = host.call("previewship", &[])?;
    assert!(!host.is_entity(super::entity(&ship)?)?);
    Ok(())
}

#[test]
fn retained_native_cases_data_reads_and_move_gate_match_instrumented_cpp() -> Result<()> {
    use serde_json::{Value as Json, json};
    use std::{cell::RefCell, collections::BTreeSet};
    struct Dispatch {
        world: World,
        valid: BTreeSet<u64>,
        calls: RefCell<Vec<Json>>,
    }
    impl Host for Dispatch {
        fn call(&mut self, _: &str, _: &[Value]) -> Result<Value> {
            bail!("unexpected native")
        }
        fn world(&self) -> World {
            self.world.clone()
        }
        fn is_entity(&self, id: EntityId) -> Result<bool> {
            self.calls.borrow_mut().push(json!(["i", id.raw()]));
            Ok(self.valid.contains(&id.raw()))
        }
        fn delete_entity(&mut self, id: EntityId) -> Result<()> {
            self.calls.borrow_mut().push(json!(["d", id.raw()]));
            self.valid.remove(&id.raw());
            Ok(())
        }
        fn attribute_changed(
            &mut self,
            id: EntityId,
            _: &AttributeStore,
            _: AttributeId,
        ) -> Result<()> {
            self.calls.borrow_mut().push(json!(["w", id.raw()]));
            Ok(())
        }
    }
    let fixture: Json =
        serde_json::from_str(include_str!("../../reference/entity_api/cases.json"))?;
    let mut count = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let mut host = Dispatch {
            world: World::default(),
            valid: [4294967296, 9920249030594524246].into(),
            calls: RefCell::new(Vec::new()),
        };
        let mut vm = Runtime::compile("void Write(ref target){target.test=19;}")?;
        for (operation, expected) in case["operations"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["snapshots"].as_array().unwrap())
        {
            let op = operation[0].as_str().unwrap();
            let shape = operation[1].as_u64().unwrap();
            let number = operation[2].as_u64().unwrap();
            let object = host.world.object(EntityId::from_raw(number))?;
            let value = if op == "u" {
                Value::Int(number as u32 as i32)
            } else if shape % 2 == 1 {
                Value::AReference(Some(object.root()))
            } else {
                Value::Object(object.clone())
            };
            let first = Variable::new(value.clone());
            let second = Variable::new(first.reference());
            let argument = if shape >= 2 {
                second.reference()
            } else {
                value
            };
            let name = match op {
                "i" => "IsEntity",
                "d" => "DeleteClass",
                "u" => "EntityUpdate",
                "w" => "Write",
                _ => unreachable!(),
            };
            let result = vm.call(name, &[argument], &mut host, Limits::default())?;
            let result = match result {
                Value::Int(n) => json!(n),
                Value::Void => Json::Null,
                _ => unreachable!(),
            };
            assert_eq!(
                json!({"result":result,"calls":*host.calls.borrow()}),
                *expected,
                "{operation}"
            );
            count += 1;
        }
    }
    assert_eq!(count, 560);
    Ok(())
}
