use super::*;
use anyhow::bail;

#[derive(Default)]
struct Trace {
    initialized: Vec<EntityId>,
    dropped: Vec<EntityId>,
    tags: Vec<u32>,
}
struct Probe {
    trace: Rc<RefCell<Trace>>,
    id: EntityId,
    tag: u32,
    result: u8,
}
impl Entity for Probe {
    fn init(&mut self, ctx: &mut EntityInit<'_>) -> Result<bool> {
        self.id = ctx.id;
        self.trace.borrow_mut().initialized.push(ctx.id);
        self.trace.borrow_mut().tags.push(self.tag);
        assert!(ctx.manager.is_valid(ctx.id));
        assert_eq!(ctx.manager.get(ctx.id).unwrap().id(), ctx.id);
        assert!(ctx.manager.get(ctx.id).unwrap().borrow().is_err());
        assert_eq!(ctx.manager.attribute_root(ctx.id), ctx.attributes);
        if let Some(root) = ctx.attributes {
            ctx.manager
                .set_attribute(ctx.id, ctx.store, "initialized", Some("1"))?;
            assert_eq!(ctx.store.get(root).unwrap().parent(), None);
        }
        ctx.manager.add_to_layer(2, ctx.id, self.tag)?;
        match self.result {
            0 => Ok(true),
            1 => Ok(false),
            _ => bail!("fixture Init error"),
        }
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.trace.borrow_mut().dropped.push(self.id);
    }
}
fn register(
    manager: &mut EntityManager,
    name: &str,
    result: u8,
    tag: u32,
    trace: &Rc<RefCell<Trace>>,
) -> Result<bool> {
    let trace = trace.clone();
    manager.register_class(name, move || {
        Ok(Box::new(Probe {
            trace: trace.clone(),
            id: EntityId::INVALID,
            tag,
            result,
        }))
    })
}

#[test]
fn creation_registers_identity_and_attributes_before_init() -> Result<()> {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut manager = EntityManager::default();
    let mut store = AttributeStore::default();
    let root = store.create_root()?;
    register(&mut manager, "Ship", 0, 7, &trace)?;
    manager.set_layer_type(2, LayerType::Execute)?;
    let id = manager.create("sHIP", &mut store, Some(root))?;
    assert_eq!(id.raw(), 1_u64 << 32);
    assert_eq!(manager.ids("SHIP"), [id]);
    assert_eq!(manager.layer_ids(LayerType::Execute), [id]);
    assert_eq!(
        store
            .get(store.child(root, "initialized").unwrap())
            .unwrap()
            .value(),
        Some("1")
    );
    assert!(manager.get(id).unwrap().borrow().is_ok());
    assert!(trace.borrow().dropped.is_empty());
    Ok(())
}

#[test]
fn rejected_and_error_initialization_defer_destruction_without_rolling_back_attributes()
-> Result<()> {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut manager = EntityManager::default();
    let mut store = AttributeStore::default();
    let root = store.create_root()?;
    for (name, result) in [("ok", 0), ("reject", 1), ("error", 2)] {
        register(&mut manager, name, result, 0, &trace)?;
    }
    assert_eq!(
        manager.create("reject", &mut store, Some(root))?,
        EntityId::INVALID
    );
    assert!(
        format!(
            "{:#}",
            manager.create("error", &mut store, None).unwrap_err()
        )
        .contains("Init entity error")
    );
    let ids = trace.borrow().initialized.clone();
    assert_eq!(
        ids.iter().map(|id| id.raw()).collect::<Vec<_>>(),
        [1_u64 << 32, (2_u64 << 32) | 1]
    );
    for id in &ids {
        assert!(!manager.is_valid(*id));
        assert!(manager.get(*id).is_some());
    }
    assert_eq!(manager.attribute_root(ids[0]), Some(root));
    assert!(store.child(root, "initialized").is_some());
    assert!(trace.borrow().dropped.is_empty());
    let before_cleanup = manager.create("ok", &mut store, None)?;
    assert_eq!(before_cleanup.raw(), (3_u64 << 32) | 2);
    manager.new_lifecycle();
    assert_eq!(trace.borrow().dropped, [ids[1], ids[0]]);
    let reused = manager.create("ok", &mut store, None)?;
    assert_eq!(reused.raw(), 4_u64 << 32);
    assert_eq!(manager.ids("ok"), [before_cleanup, reused]);
    assert!(!manager.is_valid(ids[0]));
    Ok(())
}

#[test]
fn registry_preserves_first_case_insensitive_match_and_distinguishes_hash_collisions() -> Result<()>
{
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut manager = EntityManager::default();
    let mut store = AttributeStore::default();
    assert_eq!(class_hash("aq"), class_hash("ba"));
    assert!(register(&mut manager, "aq", 0, 1, &trace)?);
    assert!(!register(&mut manager, "AQ", 0, 99, &trace)?);
    assert!(register(&mut manager, "ba", 0, 2, &trace)?);
    let a = manager.create("aQ", &mut store, None)?;
    let b = manager.create("BA", &mut store, None)?;
    assert_eq!(trace.borrow().tags, [1, 2]);
    // Original class queries use the hash, whereas factory lookup also compares names.
    assert_eq!(manager.ids("aq"), [a, b]);
    Ok(())
}

#[test]
fn missing_classes_constructor_errors_and_invalid_bindings_do_not_consume_ids() -> Result<()> {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut manager = EntityManager::default();
    let mut store = AttributeStore::default();
    manager.register_class("error", || bail!("constructor failure"))?;
    register(&mut manager, "ok", 0, 0, &trace)?;
    for name in ["missing", "error", "", "bad\0name", "корабль"] {
        assert!(manager.create(name, &mut store, None).is_err());
    }
    assert!(
        manager
            .create("ok", &mut store, Some(AttributeId::from_raw(u64::MAX)))
            .is_err()
    );
    assert_eq!(manager.create("ok", &mut store, None)?.raw(), 1_u64 << 32);
    Ok(())
}

#[test]
fn init_can_create_children_and_parent_failure_keeps_child_side_effects() -> Result<()> {
    struct Parent;
    impl Entity for Parent {
        fn init(&mut self, ctx: &mut EntityInit<'_>) -> Result<bool> {
            assert_eq!(ctx.manager.ids("parent"), [ctx.id]);
            let child = ctx.manager.create("child", ctx.store, ctx.attributes)?;
            assert!(ctx.manager.is_valid(child));
            assert_eq!(
                ctx.manager
                    .send_message(child, &mut Message::new("", vec![])?)?,
                0
            );
            Ok(false)
        }
    }
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut manager = EntityManager::default();
    let mut store = AttributeStore::default();
    manager.register_class("parent", || Ok(Box::new(Parent)))?;
    register(&mut manager, "child", 0, 0, &trace)?;
    assert_eq!(
        manager.create("parent", &mut store, None)?,
        EntityId::INVALID
    );
    assert!(manager.ids("parent").is_empty());
    let child = manager.ids("child")[0];
    assert_eq!(child.raw(), (2_u64 << 32) | 1);
    manager.new_lifecycle();
    assert!(manager.is_valid(child));
    Ok(())
}

#[test]
fn recursive_init_is_bounded_and_every_failed_instance_remains_queued_for_cleanup() -> Result<()> {
    struct Recursive(Rc<Cell<usize>>);
    use std::cell::Cell;
    impl Entity for Recursive {
        fn init(&mut self, ctx: &mut EntityInit<'_>) -> Result<bool> {
            ctx.manager.create("recursive", ctx.store, None)?;
            Ok(true)
        }
    }
    impl Drop for Recursive {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let dropped = Rc::new(Cell::new(0));
    let shared = dropped.clone();
    let mut manager = EntityManager::default();
    let mut store = AttributeStore::default();
    manager.register_class("recursive", move || Ok(Box::new(Recursive(shared.clone()))))?;
    assert!(
        format!(
            "{:#}",
            manager.create("recursive", &mut store, None).unwrap_err()
        )
        .contains("Init depth budget")
    );
    assert_eq!(manager.init_depth, 0);
    assert_eq!(manager.deleted.len(), 32);
    assert!(manager.ids("recursive").is_empty());
    assert_eq!(dropped.get(), 0);
    manager.new_lifecycle();
    assert_eq!(dropped.get(), 32);
    Ok(())
}

#[test]
fn self_deletion_during_init_defers_cleanup_and_retains_the_original_return_id() -> Result<()> {
    struct SelfDeleting(Rc<Cell<bool>>);
    use std::cell::Cell;
    impl Entity for SelfDeleting {
        fn init(&mut self, ctx: &mut EntityInit<'_>) -> Result<bool> {
            assert!(
                ctx.manager
                    .send_message(ctx.id, &mut Message::new("", vec![])?)
                    .is_err()
            );
            ctx.manager.erase(ctx.id);
            ctx.manager.new_lifecycle();
            assert!(ctx.manager.get(ctx.id).is_some());
            assert!(!self.0.get());
            Ok(true)
        }
    }
    impl Drop for SelfDeleting {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let dropped = Rc::new(Cell::new(false));
    let shared = dropped.clone();
    let mut manager = EntityManager::default();
    let mut store = AttributeStore::default();
    manager.register_class("self", move || Ok(Box::new(SelfDeleting(shared.clone()))))?;
    let id = manager.create("self", &mut store, None)?;
    assert_ne!(id, EntityId::INVALID);
    assert!(!manager.is_valid(id));
    assert!(!dropped.get());
    manager.new_lifecycle();
    assert!(dropped.get());
    Ok(())
}

#[test]
fn registry_and_instance_budgets_recover_after_deferred_deletion() -> Result<()> {
    struct Empty;
    impl Entity for Empty {}
    let mut manager = EntityManager::default();
    let mut store = AttributeStore::default();
    for n in 0..MAX_CLASSES {
        assert!(manager.register_class(&format!("class{n}"), || Ok(Box::new(Empty)))?);
    }
    assert!(!manager.register_class("CLASS0", || Ok(Box::new(Empty)))?);
    assert!(
        manager
            .register_class("overflow", || Ok(Box::new(Empty)))
            .is_err()
    );
    for _ in 0..MAX_ENTITIES {
        manager.create("class0", &mut store, None)?;
    }
    assert!(manager.create("class0", &mut store, None).is_err());
    let first = manager.ids("class0")[0];
    manager.erase(first);
    assert!(manager.create("class0", &mut store, None).is_err());
    manager.new_lifecycle();
    let next = manager.create("class0", &mut store, None)?;
    assert_eq!(next.raw(), ((MAX_ENTITIES as u64 + 1) << 32));
    assert!(!manager.is_valid(first));
    Ok(())
}

#[test]
fn direct_insertion_does_not_initialize_but_registered_creation_does() -> Result<()> {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut manager = EntityManager::default();
    let mut store = AttributeStore::default();
    manager.insert(
        "direct",
        Probe {
            trace: trace.clone(),
            id: EntityId::INVALID,
            tag: 1,
            result: 0,
        },
    )?;
    assert!(trace.borrow().initialized.is_empty());
    register(&mut manager, "registered", 0, 2, &trace)?;
    manager.create("registered", &mut store, None)?;
    assert_eq!(trace.borrow().tags, [2]);
    Ok(())
}

#[test]
fn native_creation_matches_unedited_cpp_manager_cache_entity_and_registry() -> Result<()> {
    use serde_json::{Value as Json, json};
    struct Replay {
        roots: [AttributeId; 2],
        history: Vec<EntityId>,
        initialized: Vec<Json>,
        dropped: Vec<EntityId>,
    }
    struct Native {
        mode: u8,
        id: EntityId,
        replay: Rc<RefCell<Replay>>,
    }
    fn class(mode: u8) -> &'static str {
        match mode {
            1 => "reject",
            2 => "error",
            3 => "self",
            4 => "parent",
            5 => "aq",
            6 => "ba",
            _ => "ok",
        }
    }
    impl Entity for Native {
        fn init(&mut self, ctx: &mut EntityInit<'_>) -> Result<bool> {
            self.id = ctx.id;
            let root = ctx
                .attributes
                .map(|id| {
                    self.replay
                        .borrow()
                        .roots
                        .iter()
                        .position(|a| *a == id)
                        .unwrap() as i32
                })
                .unwrap_or(-1);
            let valid = ctx.manager.is_valid(ctx.id);
            let listed = ctx.manager.ids(class(self.mode)).contains(&ctx.id);
            self.replay.borrow_mut().history.push(ctx.id);
            self.replay.borrow_mut().initialized.push(json!([
                ctx.id.raw(),
                self.mode,
                root,
                valid,
                listed
            ]));
            if let Some(root) = ctx.attributes {
                let writes = ctx
                    .store
                    .child(root, "writes")
                    .and_then(|id| ctx.store.get(id))
                    .and_then(|n| n.value())
                    .unwrap_or("0")
                    .parse::<i32>()?;
                ctx.store
                    .set_attribute(root, "writes", Some(&(writes + 1).to_string()))?;
            }
            ctx.manager.add_to_layer(2, ctx.id, 7)?;
            match self.mode {
                1 => return Ok(false),
                2 => bail!("diagnostic Init error"),
                3 => ctx.manager.erase(ctx.id),
                4 => {
                    ctx.manager.create("ok", ctx.store, ctx.attributes)?;
                    return Ok(false);
                }
                _ => {}
            }
            Ok(true)
        }
    }
    impl Drop for Native {
        fn drop(&mut self) {
            self.replay.borrow_mut().dropped.push(self.id);
        }
    }
    let fixture: Json = serde_json::from_str(include_str!("reference/cases.json"))?;
    let mut snapshots = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let mut store = AttributeStore::default();
        let roots = [store.create_root()?, store.create_root()?];
        let replay = Rc::new(RefCell::new(Replay {
            roots,
            history: vec![],
            initialized: vec![],
            dropped: vec![],
        }));
        let mut manager = EntityManager::default();
        manager.set_layer_type(2, LayerType::Execute)?;
        for (name, mode) in [
            ("ok", 0),
            ("OK", 1),
            ("reject", 1),
            ("error", 2),
            ("self", 3),
            ("parent", 4),
            ("aq", 5),
            ("ba", 6),
            ("null", 7),
        ] {
            let replay = replay.clone();
            manager.register_class(name, move || {
                if mode == 7 {
                    bail!("CreateClass returned nullptr");
                }
                Ok(Box::new(Native {
                    mode,
                    id: EntityId::INVALID,
                    replay: replay.clone(),
                }))
            })?;
        }
        for (op, expected) in case["operations"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["snapshots"].as_array().unwrap())
        {
            let mut result = EntityId::INVALID;
            let mut error = false;
            match op[0].as_str().unwrap() {
                "c" => {
                    let n = op[2].as_i64().unwrap();
                    match manager.create(
                        op[1].as_str().unwrap(),
                        &mut store,
                        if n < 0 { None } else { Some(roots[n as usize]) },
                    ) {
                        Ok(id) => result = id,
                        Err(_) => error = true,
                    }
                }
                "d" => {
                    let n = op[1].as_i64().unwrap();
                    manager.erase(if n < 0 {
                        EntityId::INVALID
                    } else {
                        replay.borrow().history[n as usize]
                    });
                }
                "f" => manager.new_lifecycle(),
                _ => unreachable!(),
            }
            let records = replay.borrow();
            let history = records
                .history
                .iter()
                .map(|id| {
                    json!([
                        id.raw(),
                        manager.is_valid(*id),
                        manager.get(*id).is_some(),
                        manager
                            .attribute_root(*id)
                            .map(|id| roots.iter().position(|r| *r == id).unwrap() as i32)
                            .unwrap_or(-1)
                    ])
                })
                .collect::<Vec<_>>();
            let classes = ["ok", "reject", "error", "self", "parent", "aq", "ba"].map(|name| {
                manager
                    .ids(name)
                    .iter()
                    .map(|id| id.raw())
                    .collect::<Vec<_>>()
            });
            let writes = roots.map(|root| {
                store
                    .child(root, "writes")
                    .and_then(|id| store.get(id))
                    .and_then(|n| n.value())
                    .unwrap_or("0")
                    .parse::<i32>()
                    .unwrap()
            });
            let actual = json!({"error":error,"result":result.raw(),"history":history,"initialized":records.initialized,
                "dropped":records.dropped.iter().map(|id|id.raw()).collect::<Vec<_>>(),"classes":classes,
                "layer":manager.layer_ids(LayerType::Execute).iter().map(|id|id.raw()).collect::<Vec<_>>(),"writes":writes});
            assert_eq!(actual, *expected, "operation {op}");
            snapshots += 1;
        }
    }
    assert_eq!(snapshots, 404);
    Ok(())
}
