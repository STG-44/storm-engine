use super::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use storm_core::{EntityManager, ScriptScalar, ScriptVariable, ScriptVariableAccess, Value};

struct Output(Rc<RefCell<ScriptScalar>>);
impl ScriptVariableAccess for Output {
    fn read(&self) -> Result<ScriptScalar> {
        Ok(self.0.borrow().clone())
    }
    fn write(&self, value: ScriptScalar) -> Result<()> {
        *self.0.borrow_mut() = value;
        Ok(())
    }
}
fn message(code: i32, output: &Rc<RefCell<ScriptScalar>>) -> Message {
    Message::new(
        "le",
        vec![
            Value::Long(code),
            Value::ScriptVariable(ScriptVariable::new(Output(output.clone()))),
        ],
    )
    .unwrap()
}

#[test]
fn retained_query_sequences_match_live_index_counts_retyping_errors_and_results() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../reference/ship_queries/cases.json")).unwrap();
    assert_eq!(
        fixture["constants"]["MSG_SHIP_GET_CHARACTER_INDEX"],
        MSG_SHIP_GET_CHARACTER_INDEX
    );
    assert_eq!(
        fixture["constants"]["MSG_SHIP_GET_NUM_FIRE_PLACES"],
        MSG_SHIP_GET_NUM_FIRE_PLACES
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 56);
    for case in cases {
        let mut store = AttributeStore::default();
        let root = store.create_root().unwrap();
        let index = case["index_exists"].as_bool().unwrap().then(|| {
            store
                .create_attribute(
                    root,
                    case["index_name"].as_str().unwrap(),
                    case["index_text"].as_str(),
                )
                .unwrap()
        });
        let count = case["fire_places"].as_u64().unwrap() as usize;
        let mut ship = ShipQueries::new(vec![(); count]);
        ship.bind_character(&store, case["bound"].as_bool().unwrap().then_some(root))
            .unwrap();
        for (step, expected) in case["expected"].as_array().unwrap().iter().enumerate() {
            if step == 2 {
                if let Some(index) = index {
                    store.set_value(index, Some("-9after")).unwrap();
                }
                ship.fire_places_mut()
                    .resize(if count == 0 { 3 } else { 0 }, ());
            }
            let output = Rc::new(RefCell::new(ScriptScalar::Float(0.25)));
            let code = if step % 2 == 0 {
                MSG_SHIP_GET_CHARACTER_INDEX
            } else {
                MSG_SHIP_GET_NUM_FIRE_PLACES
            };
            let result =
                ship.process_message_with_attributes(&mut message(code, &output), &mut store);
            assert_eq!(result.is_err(), expected["error"].as_bool().unwrap());
            if let Ok(result) = result {
                assert_eq!(result, expected["result"].as_u64().unwrap());
            }
            let actual = match *output.borrow() {
                ScriptScalar::Int(n) => serde_json::json!({"type":"int","value":n}),
                ScriptScalar::Float(n) => serde_json::json!({"type":"float","value":n.to_bits()}),
                _ => panic!("unexpected output"),
            };
            assert_eq!(actual["type"], expected["type"]);
            assert_eq!(actual["value"], expected["value"]);
        }
    }
}

#[test]
fn character_context_rejects_missing_stale_foreign_and_malformed_access_without_writing_output() {
    let mut store = AttributeStore::default();
    let root = store.create_root().unwrap();
    let mut ship = ShipQueries::<()>::new(vec![]);
    let output = Rc::new(RefCell::new(ScriptScalar::Int(99)));
    assert!(
        ship.process_message_with_attributes(
            &mut message(MSG_SHIP_GET_CHARACTER_INDEX, &output),
            &mut store
        )
        .is_err()
    );
    ship.bind_character(&store, Some(root)).unwrap();
    assert!(
        ship.process_message_with_attributes(
            &mut message(MSG_SHIP_GET_CHARACTER_INDEX, &output),
            &mut store
        )
        .is_err()
    );
    store.set_attribute(root, "index", Some("19")).unwrap();
    assert!(
        ship.process_message(&mut message(MSG_SHIP_GET_CHARACTER_INDEX, &output))
            .is_err()
    );
    let mut foreign = AttributeStore::default();
    let foreign_root = foreign.create_root().unwrap();
    assert!(
        ship.process_message_with_attributes(
            &mut message(MSG_SHIP_GET_CHARACTER_INDEX, &output),
            &mut foreign
        )
        .is_err()
    );
    assert!(ship.bind_character(&store, Some(foreign_root)).is_err());
    let mut malformed = Message::new(
        "lf",
        vec![Value::Long(MSG_SHIP_GET_CHARACTER_INDEX), Value::Float(1.0)],
    )
    .unwrap();
    assert!(
        ship.process_message_with_attributes(&mut malformed, &mut store)
            .is_err()
    );
    assert_eq!(*output.borrow(), ScriptScalar::Int(99));
    ship.process_message_with_attributes(
        &mut message(MSG_SHIP_GET_CHARACTER_INDEX, &output),
        &mut store,
    )
    .unwrap();
    assert_eq!(*output.borrow(), ScriptScalar::Int(19));
    store.drop_root(root).unwrap();
    assert!(
        ship.process_message_with_attributes(
            &mut message(MSG_SHIP_GET_CHARACTER_INDEX, &output),
            &mut store
        )
        .is_err()
    );
    assert_eq!(*output.borrow(), ScriptScalar::Int(19));
}

#[test]
fn query_uses_actual_fireplace_records_and_separately_bound_character_root() {
    struct FirePlace(Rc<Cell<u32>>);
    impl Drop for FirePlace {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let mut ship = ShipQueries::new((0..3).map(|_| FirePlace(drops.clone())).collect());
    let output = Rc::new(RefCell::new(ScriptScalar::Int(99)));
    ship.process_message(&mut message(MSG_SHIP_GET_NUM_FIRE_PLACES, &output))
        .unwrap();
    assert_eq!(*output.borrow(), ScriptScalar::Int(3));
    ship.fire_places_mut().clear();
    assert_eq!(drops.get(), 3);
    ship.process_message(&mut message(MSG_SHIP_GET_NUM_FIRE_PLACES, &output))
        .unwrap();
    assert_eq!(*output.borrow(), ScriptScalar::Int(0));
    assert!(ship.fire_places().is_empty());
    let mut store = AttributeStore::default();
    let character = store.create_root().unwrap();
    let entity_root = store.create_root().unwrap();
    store.set_dword(character, Some("index"), 27).unwrap();
    store.set_dword(entity_root, Some("index"), 99).unwrap();
    ship.bind_character(&store, Some(character)).unwrap();
    let mut manager = EntityManager::default();
    let id = manager.insert("query", ship).unwrap();
    manager
        .bind_attributes(id, &store, Some(entity_root))
        .unwrap();
    manager
        .send_message_with_attributes(
            id,
            &mut message(MSG_SHIP_GET_CHARACTER_INDEX, &output),
            &mut store,
        )
        .unwrap();
    assert_eq!(*output.borrow(), ScriptScalar::Int(27));
}
