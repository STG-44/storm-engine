use super::*;
use anyhow::Result;
use std::{cell::RefCell, rc::Rc};

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
    core.process_stage(Stage::Execute, 5);
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
    core.process_stage(Stage::Execute, 1);
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
        "bwlufdics",
        vec![
            Value::Byte(255),
            Value::Word(65535),
            Value::Long(i32::MIN),
            Value::Dword(u32::MAX),
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
    for code in ["a", "e", "p", "?", "ф"] {
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
