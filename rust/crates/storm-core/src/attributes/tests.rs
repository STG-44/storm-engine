use super::*;
use crate::{Entity, EntityManager, Message, Value};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, rc::Rc};

fn snapshot(store: &AttributeStore, id: AttributeId) -> Json {
    let node = store.get(id).unwrap();
    json!({"name":store.name(id),"code":node.name_code(),"value":node.value(),
        "children":node.children().iter().map(|id|snapshot(store,*id)).collect::<Vec<_>>()})
}

#[test]
fn trees_and_string_codes_match_retained_cpp_operations() -> Result<()> {
    let reference: Json = serde_json::from_str(include_str!("reference/cases.json"))?;
    for (case_index, case) in reference["cases"].as_array().unwrap().iter().enumerate() {
        let mut store = AttributeStore::default();
        let mut ids = BTreeMap::<String, Option<AttributeId>>::new();
        let mut roots = vec![];
        let mut results = vec![];
        for op in case["operations"].as_array().unwrap() {
            let a = op.as_array().unwrap();
            let s = |i: usize| a[i].as_str().unwrap();
            let id = |i: usize| ids.get(s(i)).copied().flatten().unwrap();
            let result = match s(0) {
                "root" => {
                    let root = store.create_root()?;
                    ids.insert(s(1).into(), Some(root));
                    roots.push(root);
                    json!(store.get(root).unwrap().name_code())
                }
                "create" | "verify" | "create_path" | "find" | "copy" => {
                    let node = match s(0) {
                        "create" => Some(store.create_attribute(id(1), s(3), a[4].as_str())?),
                        "verify" => Some(store.verify_attribute(id(1), s(3))?),
                        "create_path" => Some(store.create_path(id(1), s(3))?),
                        "find" => store.find_path(id(1), s(3))?,
                        "copy" => Some(store.copy(id(1))?),
                        _ => unreachable!(),
                    };
                    ids.insert(s(2).into(), node);
                    if s(0) == "copy" {
                        roots.push(node.unwrap());
                    }
                    json!(node.map(|id| store.get(id).unwrap().name_code()))
                }
                "set" => json!(store.set_attribute(id(1), s(2), a[3].as_str())?),
                "get" => json!(store.value(id(1), Some(s(2)))),
                "value" => {
                    store.set_value(id(1), a[2].as_str())?;
                    Json::Null
                }
                "rename" => {
                    let node = id(1);
                    store.set_name(node, s(2))?;
                    json!(store.get(node).unwrap().name_code())
                }
                "parent" => json!(store.get(id(1)).unwrap().parent() == Some(id(2))),
                "same" => json!(ids[s(1)] == ids[s(2)]),
                "break" => {
                    store.set_break(id(1), a[2].as_bool().unwrap())?;
                    Json::Null
                }
                "breaks" => json!(store.break_notifications()),
                "names" => json!(store.name_count()),
                "delete" => json!(store.delete_class(id(1), id(2))?),
                "dword" => {
                    json!(store.set_dword(id(1), a[2].as_str(), a[3].as_u64().unwrap() as u32)?)
                }
                "float" => {
                    json!(store.set_float(id(1), a[2].as_str(), a[3].as_f64().unwrap() as f32)?)
                }
                "get_dword" => {
                    json!(store.as_dword(id(1), a[2].as_str(), a[3].as_u64().unwrap() as u32))
                }
                other => panic!("unknown reference operation {other}"),
            };
            results.push(result);
        }
        assert_eq!(
            json!(results),
            case["results"],
            "case {case_index} operation results"
        );
        assert_eq!(
            json!(
                roots
                    .iter()
                    .map(|id| snapshot(&store, *id))
                    .collect::<Vec<_>>()
            ),
            case["trees"],
            "case {case_index} trees"
        );
        assert_eq!(
            json!(store.name_count()),
            case["names"],
            "case {case_index} string codec"
        );
        assert_eq!(
            json!(store.break_notifications()),
            case["breaks"],
            "case {case_index} break notifications"
        );
    }
    Ok(())
}

#[test]
fn numeric_prefixes_and_printf_g_match_retained_cpp_reference() -> Result<()> {
    let reference: Json = serde_json::from_str(include_str!("reference/cases.json"))?;
    for case in reference["parses"].as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        assert_eq!(
            u64::from(numbers::parse_dword(input)),
            case["dword"].as_u64().unwrap(),
            "{input:?} dword"
        );
        let value = numbers::parse_float(input);
        if case["nan"].as_bool().unwrap() {
            assert!(value.is_nan(), "{input:?}");
        } else {
            assert_eq!(
                u64::from(value.to_bits()),
                case["float_bits"].as_u64().unwrap(),
                "{input:?} float"
            );
        }
    }
    for case in reference["formats"].as_array().unwrap() {
        let bits = case["bits"].as_u64().unwrap() as u32;
        assert_eq!(
            format_float(f32::from_bits(bits)),
            case["text"].as_str().unwrap(),
            "float {bits:#010x}"
        );
    }
    Ok(())
}

#[test]
fn paths_distinguish_empty_values_literal_names_and_copied_parents() -> Result<()> {
    let mut store = AttributeStore::default();
    let root = store.create_root()?;
    let leaf = store.create_path(root, "ship.sails.rey_b2.s1.hd")?;
    assert_eq!(store.get(leaf).unwrap().value(), None);
    store.set_value(leaf, Some("37"))?;
    store.set_attribute(root, "ship.sails", Some("literal"))?;
    assert_eq!(store.value(root, Some("ship.sails")), Some("literal"));
    assert_eq!(
        store.lookup_path(root, "SHIP.SAILS.rey_b2.S1.hd")?,
        Some(leaf)
    );
    assert_eq!(store.value(leaf, None), Some("37"));
    let copy = store.copy(root)?;
    let copied_leaf = store.lookup_path(copy, "ship.sails.rey_b2.s1.hd")?.unwrap();
    let mut top = copied_leaf;
    while let Some(parent) = store.get(top).unwrap().parent() {
        top = parent;
    }
    assert_eq!(top, copy);
    store.drop_root(root)?;
    assert_eq!(store.as_dword(copied_leaf, None, 0), 37);
    assert!(store.get(leaf).is_none());
    store.set_value(copied_leaf, Some(""))?;
    assert_eq!(store.get(copied_leaf).unwrap().value(), Some(""));
    assert_eq!(store.as_dword(copied_leaf, None, 99), 0);
    store.set_value(copied_leaf, None)?;
    assert_eq!(store.as_dword(copied_leaf, None, 99), 99);
    Ok(())
}

#[test]
fn stale_foreign_and_null_attribute_messages_are_checked_by_the_store() -> Result<()> {
    let mut first = AttributeStore::default();
    let mut second = AttributeStore::default();
    let root = first.create_root()?;
    let foreign = second.create_root()?;
    assert_ne!(root, foreign);
    assert!(first.get(foreign).is_none());
    assert!(second.get(root).is_none());
    let mut message = Message::new(
        "aa",
        vec![
            Value::Attribute(root),
            Value::Attribute(AttributeId::INVALID),
        ],
    )?;
    first.drop_root(root)?;
    assert!(first.get(message.attribute()?).is_none());
    assert_eq!(message.attribute()?, AttributeId::INVALID);
    assert!(first.set_value(root, Some("stale")).is_err());
    assert!(first.create_path(foreign, "a.b").is_err());
    assert_eq!(first.len(), 0);
    Ok(())
}

#[test]
fn rejected_paths_do_not_create_partial_nodes_and_depth_is_bounded() -> Result<()> {
    let mut store = AttributeStore::default();
    let root = store.create_root()?;
    for path in [
        "valid.\0invalid",
        "valid.неподдерживаемое",
        &format!("valid.{}", "x".repeat(1024)),
    ] {
        assert!(store.create_path(root, path).is_err());
        assert_eq!(store.len(), 1);
    }
    let deep = std::iter::repeat_n("a", 255).collect::<Vec<_>>().join(".");
    let leaf = store.create_path(root, &deep)?;
    assert_eq!(store.len(), 256);
    assert!(store.create_attribute(leaf, "too_deep", None).is_err());
    assert_eq!(store.len(), 256);
    assert_eq!(store.lookup_path(root, &deep)?, Some(leaf));
    Ok(())
}

#[test]
fn entity_attributes_are_shared_borrowed_and_notify_only_when_requested() -> Result<()> {
    struct Watcher(Rc<RefCell<Vec<String>>>);
    impl Entity for Watcher {
        fn attribute_changed(
            &mut self,
            store: &AttributeStore,
            changed: AttributeId,
        ) -> Result<u32> {
            self.0
                .borrow_mut()
                .push(store.name(changed).unwrap().into());
            Ok(7)
        }
    }
    let calls = Rc::new(RefCell::new(vec![]));
    let mut entities = EntityManager::default();
    let a = entities.insert("Ship", Watcher(calls.clone()))?;
    let b = entities.insert("Ship", Watcher(calls.clone()))?;
    let mut store = AttributeStore::default();
    let root = store.create_root()?;
    for entity in [a, b] {
        assert!(entities.bind_attributes(entity, &store, Some(root))?);
    }
    assert_eq!(
        entities.set_attribute(a, &mut store, "Speed", Some("12"))?,
        Some(0)
    );
    assert_eq!(
        store.value(entities.attribute_root(b).unwrap(), Some("speed")),
        Some("12")
    );
    assert!(calls.borrow().is_empty());
    let speed = store.child(root, "speed").unwrap();
    assert_eq!(entities.notify_attribute_changed(a, &store, speed)?, 7);
    entities.erase(a);
    assert_eq!(entities.notify_attribute_changed(a, &store, speed)?, 7);
    entities.new_lifecycle();
    assert!(store.get(root).is_some());
    assert_eq!(entities.notify_attribute_changed(a, &store, speed)?, 0);
    store.drop_root(root)?;
    assert!(entities.notify_attribute_changed(b, &store, speed).is_err());
    entities.bind_attributes(b, &store, None)?;
    assert_eq!(entities.attribute_root(b), None);
    assert_eq!(*calls.borrow(), ["Speed", "Speed"]);
    Ok(())
}
