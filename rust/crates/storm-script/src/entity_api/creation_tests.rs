use crate::{Host, Limits, Program, Runtime, Value, Variable, World};
use anyhow::{Result, bail};
use std::{cell::RefCell, rc::Rc};
use storm_core::{AttributeId, AttributeStore, Entity, EntityId, EntityInit, EntityManager};

#[derive(Default)]
struct Records {
    initialized: Vec<(EntityId, bool)>,
    notifications: usize,
}
struct Instance {
    records: Rc<RefCell<Records>>,
    mode: u8,
}
impl Entity for Instance {
    fn init(&mut self, ctx: &mut EntityInit<'_>) -> Result<bool> {
        assert!(ctx.manager.is_valid(ctx.id));
        self.records
            .borrow_mut()
            .initialized
            .push((ctx.id, ctx.attributes.is_some()));
        if let Some(root) = ctx.attributes {
            ctx.store.set_attribute(root, "initialized", Some("1"))?;
        }
        match self.mode {
            0 => Ok(true),
            1 => Ok(false),
            3 => {
                let root = ctx.attributes.unwrap();
                for _ in 0..32768 {
                    ctx.store.create_attribute(root, "repeat", None)?;
                }
                Ok(true)
            }
            _ => bail!("diagnostic Init failure"),
        }
    }
    fn attribute_changed(&mut self, _: &AttributeStore, _: AttributeId) -> Result<u32> {
        self.records.borrow_mut().notifications += 1;
        Ok(0)
    }
}
struct TestHost {
    world: World,
    ship: Variable,
    entities: EntityManager,
    records: Rc<RefCell<Records>>,
}
impl TestHost {
    fn new() -> Result<Self> {
        let world = World::default();
        let ship = Variable::new(Value::Object(world.object(EntityId::INVALID)?));
        let records = Rc::new(RefCell::new(Records::default()));
        let mut entities = EntityManager::default();
        for (name, mode) in [("ok", 0), ("reject", 1), ("error", 2), ("overflow", 3)] {
            let records = records.clone();
            entities.register_class(name, move || {
                Ok(Box::new(Instance {
                    records: records.clone(),
                    mode,
                }))
            })?;
        }
        Ok(Self {
            world,
            ship,
            entities,
            records,
        })
    }
    fn id(&self) -> Result<EntityId> {
        let Value::Object(o) = self.ship.value()? else {
            bail!("not object")
        };
        Ok(o.entity())
    }
}
impl Host for TestHost {
    fn call(&mut self, name: &str, _: &[Value]) -> Result<Value> {
        if name == "ship" {
            Ok(self.ship.reference())
        } else {
            bail!("unexpected native")
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
    fn create_entity(
        &mut self,
        class: &str,
        store: &mut AttributeStore,
        root: Option<AttributeId>,
    ) -> Result<EntityId> {
        self.entities.create(class, store, root)
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
    Runtime::compile(&format!("native ref Ship();{source}"))
}
fn call(vm: &mut Runtime, name: &str, host: &mut impl Host) -> Result<Value> {
    vm.call(name, &[], host, Limits::default())
}

#[test]
fn create_entity_binds_the_existing_tree_and_sets_ref_identity_after_init() -> Result<()> {
    let mut host = TestHost::new()?;
    let root = host.ship.value()?.attributes()?;
    let mut vm = runtime(
        r#"int main(){ref s;s=Ship();s.index=19;
        if(CreateEntity(s,"OK")!=1)return -1;s.hd=548;
        return IsEntity(s)*100+sti(s.index)+sti(s.initialized);}"#,
    )?;
    assert_eq!(call(&mut vm, "main", &mut host)?, Value::Int(120));
    let id = host.id()?;
    assert_eq!(id.raw(), 1_u64 << 32);
    assert_eq!(
        host.entities.attribute_root(id),
        root.inspect(|_, id| Some(id))?
    );
    assert_eq!(host.records.borrow().notifications, 1); // Init population is silent.
    assert_eq!(host.records.borrow().initialized, [(id, true)]);
    Ok(())
}

#[test]
fn replacement_keeps_prior_entities_alive_and_failed_init_preserves_the_destination_id()
-> Result<()> {
    let mut host = TestHost::new()?;
    let mut vm = runtime(
        r#"object previous;
        int main(){ref s;s=Ship();CreateEntity(s,"ok");previous=s;
            CreateEntity(s,"ok");if(CreateEntity(s,"reject")!=0)return -1;
            return IsEntity(s)+IsEntity(previous);}
        void error(){CreateEntity(Ship(),"error");}"#,
    )?;
    assert_eq!(call(&mut vm, "main", &mut host)?, Value::Int(2));
    let saved = host.id()?;
    assert_eq!(saved.raw(), (2_u64 << 32) | 1);
    assert_eq!(host.entities.ids("ok").len(), 2);
    assert!(
        !host
            .entities
            .is_valid(host.records.borrow().initialized[2].0)
    );
    assert!(call(&mut vm, "error", &mut host).is_err());
    assert_eq!(host.id()?, saved);
    assert!(host.entities.is_valid(saved));
    assert_eq!(host.records.borrow().initialized.len(), 4);
    Ok(())
}

#[test]
fn plain_object_arguments_use_stack_copies_and_expired_attribute_bindings_fail_safely() -> Result<()>
{
    let mut host = TestHost::new()?;
    let mut vm = runtime(
        r#"int main(){object o;o.index=19;
        if(CreateEntity(o,"ok")!=1)return -1;
        if(CheckAttribute(o,"initialized")!=0)return -2;return IsEntity(o);}"#,
    )?;
    assert_eq!(call(&mut vm, "main", &mut host)?, Value::Int(0));
    let id = host.entities.ids("ok")[0];
    let binding = host.entities.attribute_root(id).unwrap();
    host.world.live_nodes()?; // Drain the stack-copy owner's deferred release.
    assert!(host.world.read(|store| store.get(binding).is_none())?);
    assert!(
        host.world
            .initialize(None, |store, _| host.entities.set_attribute(
                id,
                store,
                "late",
                Some("1")
            ))
            .is_err()
    );
    Ok(())
}

#[test]
fn aref_values_keep_snapshot_ids_while_refs_to_arefs_update_only_that_variable() -> Result<()> {
    let mut host = TestHost::new()?;
    let mut vm = runtime(
        r#"int main(){object o;aref a,old;makearef(a,o.child);old=a;
        CreateEntity(a,"ok");if(IsEntity(a)!=0 || IsEntity(o)!=0)return -1;
        ref r;makeref(r,a);CreateEntity(r,"ok");
        if(IsEntity(a)!=1 || IsEntity(old)!=0 || IsEntity(o)!=0)return -2;
        return sti(o.child.initialized);}"#,
    )?;
    assert_eq!(call(&mut vm, "main", &mut host)?, Value::Int(1));
    assert_eq!(host.entities.ids("ok").len(), 2);
    Ok(())
}

#[test]
fn indexed_refs_bind_actual_array_cells_across_calls_and_posted_events() -> Result<()> {
    let mut host = TestHost::new()?;
    let mut vm = runtime(
        r#"object ships[2];
        void late(){ref list;makeref(list,ships);CreateEntity(list[1],"ok");}
        void main(){SetEventHandler("Create","late",0);PostEvent("Create",0);}
        int check(){return IsEntity(ships[0])+IsEntity(ships[1]);}"#,
    )?;
    call(&mut vm, "main", &mut host)?;
    assert_eq!(call(&mut vm, "check", &mut host)?, Value::Int(0));
    vm.advance_frame(0, &mut host, Limits::default())?;
    vm.advance_frame(0, &mut host, Limits::default())?;
    assert_eq!(call(&mut vm, "check", &mut host)?, Value::Int(1));
    Ok(())
}

#[test]
fn create_class_initializes_with_no_attribute_binding_and_returns_an_object() -> Result<()> {
    let mut host = TestHost::new()?;
    let mut vm = runtime(
        r#"int main(){object o;o.old=9;o=CreateClass("ok");
        if(IsEntity(o)!=1 || CheckAttribute(o,"initialized")!=0)return -1;
        o.value=7;return sti(o.old);}"#,
    )?;
    assert_eq!(call(&mut vm, "main", &mut host)?, Value::Int(9));
    let id = host.entities.ids("ok")[0];
    assert_eq!(host.entities.attribute_root(id), None);
    assert_eq!(host.records.borrow().initialized, [(id, false)]);
    assert_eq!(host.records.borrow().notifications, 1);
    let mut failed = runtime(
        r#"object o;void main(){o.old=9;o=CreateClass("reject");}int check(){return sti(o.old)+IsEntity(o);}"#,
    )?;
    assert!(
        format!("{:#}", call(&mut failed, "main", &mut host).unwrap_err())
            .contains("CreateClass Init returned false")
    );
    assert_eq!(call(&mut failed, "check", &mut host)?, Value::Int(9));
    Ok(())
}

#[test]
fn invalid_creation_calls_fail_before_initialization_and_retain_source_locations() -> Result<()> {
    for body in [
        "CreateEntity(1,\"ok\");",
        "CreateEntity(Ship(),1);",
        "CreateClass(1);",
        "ref r;CreateEntity(r,\"ok\");",
        "aref a;CreateEntity(a,\"ok\");",
        "CreateEntity(Ship(),\"missing\");",
    ] {
        let mut host = TestHost::new()?;
        let mut vm = runtime(&format!("void main(){{{body}}}"))?;
        let error = format!("{:#}", call(&mut vm, "main", &mut host).unwrap_err());
        assert!(error.contains("main <source>:"), "{error}");
        assert!(host.records.borrow().initialized.is_empty());
        assert_eq!(host.id()?, EntityId::INVALID);
    }
    for source in [
        "native object CreateClass(string c);",
        "native int CreateEntity(ref r,string c);",
        "void main(){CreateClass();}",
        "void main(){object o;CreateEntity(o);}",
    ] {
        assert!(Program::compile(source).is_err());
    }
    let program = Program::compile("void main(){CreateClass(\"ok\");}")?;
    assert!(
        format!(
            "{:#}",
            program
                .call("main", &[], &mut (), Limits::default())
                .unwrap_err()
        )
        .contains("creation is not bound")
    );
    Ok(())
}

#[test]
fn foreign_attribute_worlds_are_rejected_before_the_native_constructor() -> Result<()> {
    let mut host = TestHost::new()?;
    let foreign = World::default();
    let object = foreign.object(EntityId::INVALID)?;
    let mut vm = runtime("void main(){}")?;
    let error = vm
        .call(
            "CreateEntity",
            &[Value::Object(object), Value::String("ok".into())],
            &mut host,
            Limits::default(),
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("foreign attribute world"));
    assert!(host.records.borrow().initialized.is_empty());
    Ok(())
}

#[test]
fn native_init_exceeding_script_attribute_budgets_invalidates_the_unreturned_entity() -> Result<()>
{
    let mut host = TestHost::new()?;
    let mut vm = runtime(r#"void main(){CreateEntity(Ship(),"overflow");}"#)?;
    assert!(
        format!("{:#}", call(&mut vm, "main", &mut host).unwrap_err())
            .contains("script attribute node budget")
    );
    assert_eq!(host.id()?, EntityId::INVALID);
    assert!(host.entities.ids("overflow").is_empty());
    let created = host.records.borrow().initialized[0].0;
    assert!(host.entities.get(created).is_some());
    host.ship = Variable::new(Value::Int(0));
    assert_eq!(host.world.live_nodes()?, 0);
    host.entities.new_lifecycle();
    assert!(host.entities.get(created).is_none());
    Ok(())
}

#[test]
fn resource_free_creation_fixture_uses_registered_native_factories() -> Result<()> {
    let mut host = crate::preview::AttributeHost::new()?;
    let mut vm = Runtime::compile(include_str!("../../fixtures/entity_creation.c"))?;
    let result = call(&mut vm, "main", &mut host)?;
    assert_eq!(result, Value::Int(19));
    let report = host.report(&result);
    assert_eq!(report["ship_attribute_notifications"], 6);
    let tree = report["script_ship"]["children"].as_array().unwrap();
    let fields = tree
        .iter()
        .map(|v| (v["name"].as_str().unwrap(), v["value"].as_str().unwrap()))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        fields,
        [
            ("index", "19"),
            ("initialized", "1"),
            ("ready", "1"),
            ("rejected", "1"),
            ("failed", "1"),
            ("new_valid", "1"),
            ("old_valid", "0")
        ]
        .into()
    );
    Ok(())
}
