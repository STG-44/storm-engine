use super::*;
use anyhow::Result;
use std::{cell::RefCell, rc::Rc};

#[test]
fn attribute_message_context_preserves_legacy_handlers_cursor_and_invalid_lookup() -> Result<()> {
    let mut manager = EntityManager::default();
    let mut store = AttributeStore::default();
    let root = store.create_root()?;
    let id = manager.insert(
        "probe",
        Probe {
            name: 10,
            log: Rc::new(RefCell::new(vec![])),
        },
    )?;
    manager.bind_attributes(id, &store, Some(root))?;
    let mut message = Message::new("ll", vec![Value::Long(99), Value::Long(5)])?;
    assert_eq!(message.long()?, 99);
    assert_eq!(message.current_value(), Some(&Value::Long(5)));
    assert_eq!(
        manager.send_message_with_attributes(id, &mut message, &mut store)?,
        15
    );
    assert_eq!(message.current_value(), None);
    manager.erase(id);
    let mut foreign = AttributeStore::default();
    let mut message = Message::new("l", vec![Value::Long(7)])?;
    assert_eq!(
        manager.send_message_with_attributes(id, &mut message, &mut foreign)?,
        0
    );
    assert_eq!(message.current_value(), Some(&Value::Long(7)));
    Ok(())
}

#[test]
fn context_callbacks_mutate_the_selected_store_and_preserve_full_width_results() -> Result<()> {
    struct ContextEntity(AttributeId);
    impl Entity for ContextEntity {
        fn process_message_with_attributes(
            &mut self,
            message: &mut Message,
            store: &mut AttributeStore,
        ) -> Result<u64> {
            let before = store.as_dword(self.0, Some("index"), 0);
            store.set_dword(self.0, Some("index"), before + message.long()? as u32)?;
            Ok(u64::MAX)
        }
    }
    let mut manager = EntityManager::default();
    let mut store = AttributeStore::default();
    let root = store.create_root()?;
    store.set_dword(root, Some("index"), 19)?;
    let id = manager.insert("context", ContextEntity(root))?;
    manager.bind_attributes(id, &store, Some(root))?;
    assert_eq!(
        manager.send_message_with_attributes(
            id,
            &mut Message::new("l", vec![Value::Long(8)])?,
            &mut store
        )?,
        u64::MAX
    );
    assert_eq!(store.as_dword(root, Some("index"), 0), 27);
    Ok(())
}

#[test]
fn context_checks_foreign_and_stale_bindings_before_consuming_messages() -> Result<()> {
    let mut manager = EntityManager::default();
    let mut store = AttributeStore::default();
    let root = store.create_root()?;
    let id = manager.insert(
        "probe",
        Probe {
            name: 10,
            log: Rc::new(RefCell::new(vec![])),
        },
    )?;
    manager.bind_attributes(id, &store, Some(root))?;
    let mut foreign = AttributeStore::default();
    foreign.create_root()?;
    let mut message = Message::new("l", vec![Value::Long(5)])?;
    assert!(
        manager
            .send_message_with_attributes(id, &mut message, &mut foreign)
            .is_err()
    );
    assert_eq!(message.current_value(), Some(&Value::Long(5)));
    store.drop_root(root)?;
    assert!(
        manager
            .send_message_with_attributes(id, &mut message, &mut store)
            .is_err()
    );
    assert_eq!(message.current_value(), Some(&Value::Long(5)));
    Ok(())
}

#[test]
fn attribute_message_callbacks_reject_reentry_during_init() -> Result<()> {
    struct Reentrant;
    impl Entity for Reentrant {
        fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
            let mut message = Message::new("", vec![])?;
            let error = context
                .manager
                .send_message_with_attributes(context.id, &mut message, context.store)
                .unwrap_err();
            assert!(error.to_string().contains("reentrant entity callback"));
            Ok(true)
        }
    }
    let mut manager = EntityManager::default();
    let mut store = AttributeStore::default();
    manager.register_class("reentrant", || Ok(Box::new(Reentrant)))?;
    let id = manager.create("reentrant", &mut store, None)?;
    assert!(manager.is_valid(id));
    Ok(())
}

#[test]
fn script_variable_message_cursor_and_weak_native_access_are_checked() -> Result<()> {
    use anyhow::Context;
    struct Cell(std::rc::Weak<RefCell<ScriptScalar>>);
    impl ScriptVariableAccess for Cell {
        fn read(&self) -> Result<ScriptScalar> {
            Ok(self
                .0
                .upgrade()
                .context("expired cell")?
                .try_borrow()?
                .clone())
        }
        fn write(&self, value: ScriptScalar) -> Result<()> {
            *self.0.upgrade().context("expired cell")?.try_borrow_mut()? = value;
            Ok(())
        }
    }
    let owner = Rc::new(RefCell::new(ScriptScalar::Int(7)));
    let variable = ScriptVariable::new(Cell(Rc::downgrade(&owner)));
    let mut message = Message::new("e", vec![Value::ScriptVariable(variable.clone())])?;
    assert!(message.long().is_err());
    assert_eq!(message.current_type(), Some('e'));
    let cell = message.script_variable()?;
    assert_eq!(cell, variable);
    assert_eq!(cell.integer()?, 7);
    cell.set(ScriptScalar::Float(3.25))?;
    assert_eq!(cell.float()?, 3.25);
    assert!(cell.integer().is_err());
    assert!(!message.param_valid());
    assert!(message.script_variable().is_err());
    message.move_to_start();
    assert_eq!(message.script_variable()?, cell);
    drop(owner);
    assert!(cell.read().is_err());
    assert!(cell.elements().is_err());
    assert!(cell.set(ScriptScalar::Int(1)).is_err());
    Ok(())
}

struct Probe {
    name: u32,
    log: Rc<RefCell<Vec<u32>>>,
}
impl Entity for Probe {
    fn process_stage(&mut self, _: Stage, delta: u32) {
        self.log.borrow_mut().push(self.name + delta);
    }
    fn process_message(&mut self, m: &mut Message) -> Result<u64> {
        Ok(m.long()? as u64 + u64::from(self.name))
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.log.borrow_mut().push(self.name);
    }
}

#[test]
fn deletion_defers_drop_and_slot_reuse_rejects_stale_ids() -> Result<()> {
    let log = Rc::new(RefCell::new(vec![]));
    let mut core = EntityManager::default();
    let first = core.insert(
        "SHIP",
        Probe {
            name: 10,
            log: log.clone(),
        },
    )?;
    let second = core.insert(
        "ship",
        Probe {
            name: 20,
            log: log.clone(),
        },
    )?;
    assert_eq!(first.raw(), 1_u64 << 32);
    assert_eq!(second.raw(), (2_u64 << 32) | 1);
    assert_eq!(core.ids("Ship"), [first, second]);
    core.set_layer_type(0, LayerType::Execute)?;
    core.add_to_layer(0, first, 0)?;
    core.erase(first);
    core.erase(first);
    assert!(!core.is_valid(first));
    assert!(core.get(first).is_some());
    assert_eq!(core.ids("SHIP"), [second]);
    assert_eq!(core.layer_ids(LayerType::Execute), [first]);
    assert_eq!(
        core.send_message(first, &mut Message::new("l", vec![Value::Long(5)])?)?,
        0
    );
    core.process_stage(Stage::Execute, 5)?;
    assert!(log.borrow().is_empty());
    core.new_lifecycle();
    assert_eq!(*log.borrow(), [10]);
    assert!(core.get(first).is_none());
    assert!(core.layer_ids(LayerType::Execute).is_empty());
    let replacement = core.insert("ship", Probe { name: 30, log })?;
    assert_eq!(replacement.raw(), 3_u64 << 32);
    assert!(!core.is_valid(first));
    assert!(!core.is_valid(EntityId::from_raw(u64::MAX)));
    assert!(!core.is_valid(EntityId::INVALID));
    Ok(())
}

#[test]
fn layers_keep_priority_ties_duplicates_and_frozen_message_delivery() -> Result<()> {
    let log = Rc::new(RefCell::new(vec![]));
    let mut core = EntityManager::default();
    let a = core.insert(
        "A",
        Probe {
            name: 10,
            log: log.clone(),
        },
    )?;
    let b = core.insert(
        "B",
        Probe {
            name: 20,
            log: log.clone(),
        },
    )?;
    let c = core.insert(
        "C",
        Probe {
            name: 30,
            log: log.clone(),
        },
    )?;
    for layer in [0, 4] {
        core.set_layer_type(layer, LayerType::Execute)?;
    }
    core.add_to_layer(0, a, 9)?;
    core.add_to_layer(0, b, 2)?;
    core.add_to_layer(0, c, 9)?;
    core.add_to_layer(0, a, 0)?; // repeated insertion does not reprioritize
    core.add_to_layer(4, b, 0)?;
    assert_eq!(core.layer_ids(LayerType::Execute), [b, a, c, b]);
    core.process_stage(Stage::Execute, 1)?;
    assert_eq!(*log.borrow(), [21, 11, 31, 21]);
    core.freeze_layer(0, true)?;
    assert_eq!(core.layer_ids(LayerType::Execute), [b]);
    assert_eq!(
        core.send_message(a, &mut Message::new("l", vec![Value::Long(5)])?)?,
        15
    );
    core.remove_from_layer(4, b)?;
    assert!(core.layer_ids(LayerType::Execute).is_empty());
    assert!(core.add_to_layer(32, a, 0).is_err());
    assert!(core.freeze_layer(255, true).is_err());
    Ok(())
}

#[test]
fn owned_message_preserves_widths_and_failed_reads_do_not_advance() -> Result<()> {
    let id = EntityId::from_raw(0x1234_5678_90ab_cdef);
    let mut m = Message::new(
        "bwlupfdics",
        vec![
            Value::Byte(255),
            Value::Word(65535),
            Value::Long(i32::MIN),
            Value::Dword(u32::MAX),
            Value::Pointer(0xfedc_ba98_7654_3210),
            Value::Float(0.1),
            Value::Double(1.0 / 3.0),
            Value::Entity(id),
            Value::Vector([1., 2., 3.]),
            Value::String("парус".into()),
        ],
    )?;
    assert!(m.long().is_err());
    assert_eq!(m.current_type(), Some('b'));
    assert_eq!(m.byte()?, 255);
    assert_eq!(m.word()?, 65535);
    assert_eq!(m.long()?, i32::MIN);
    assert_eq!(m.dword()?, -1);
    assert!(m.long().is_err());
    assert_eq!(m.pointer()?, 0xfedc_ba98_7654_3210);
    assert_eq!(m.float()?.to_bits(), 0.1_f32.to_bits());
    assert_eq!(m.double()?, 1.0 / 3.0);
    assert_eq!(m.entity()?, id);
    assert_eq!(m.vector()?, [1., 2., 3.]);
    assert_eq!(m.string()?, "парус");
    assert!(!m.param_valid());
    assert_eq!(m.current_type(), None);
    assert!(m.string().is_err());
    m.move_to_start();
    assert_eq!(m.byte()?, 255);
    assert!(Message::new("l", vec![Value::Float(1.)]).is_err());
    assert!(Message::new("ll", vec![Value::Long(1)]).is_err());
    for code in ["e", "p", "?", "ф"] {
        assert!(Message::new(code, vec![Value::Long(0)]).is_err());
    }
    assert!(Message::new("", vec![])?.long().is_err());
    Ok(())
}

#[test]
fn events_rewind_for_each_handler_and_keep_last_nonvoid_result() -> Result<()> {
    let values = Rc::new(RefCell::new(vec![]));
    let mut events = EventDispatcher::default();
    for result in [Some(Value::Long(3)), Some(Value::Float(4.0)), None] {
        let values = values.clone();
        events.register("ShpSailDamage", move |m| {
            values.borrow_mut().push(m.long()?);
            Ok(result.clone())
        })?;
    }
    let mut m = Message::new("l", vec![Value::Long(-1)])?;
    m.long()?;
    assert_eq!(
        events.emit("shpsaildamage", &mut m)?,
        Some(Value::Float(4.0))
    );
    assert_eq!(*values.borrow(), [-1, -1, -1]);
    assert_eq!(events.emit("unknown", &mut m)?, None);
    assert!(events.register("", |_| Ok(None)).is_err());
    Ok(())
}

#[test]
fn class_hash_matches_original_ascii_constants() {
    assert_eq!(class_hash(""), 0);
    assert_eq!(class_hash("ship"), 0x79f00);
    assert_eq!(class_hash("SHIP"), class_hash("Ship"));
    assert_eq!(class_hash("ship\0other"), class_hash("ship"));
    assert_eq!(class_hash("abcdefghijklmnopqrstuvwxyz"), 0x08d1e00a);
}
