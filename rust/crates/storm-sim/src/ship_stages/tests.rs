use super::*;
use serde_json::{Value as Json, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use storm_core::{Entity, LayerType};
struct Probe {
    id: Rc<Cell<EntityId>>,
    calls: Rc<RefCell<Vec<Json>>>,
    dropped: Rc<RefCell<Vec<u64>>>,
}
impl StagePort for Probe {
    fn execute(&mut self, delta: u32) -> Result<()> {
        self.calls
            .borrow_mut()
            .push(json!([self.id.get().raw(), 0, delta]));
        Ok(())
    }
    fn realize(&mut self, delta: u32) -> Result<()> {
        self.calls
            .borrow_mut()
            .push(json!([self.id.get().raw(), 1, delta]));
        Ok(())
    }
}
impl Entity for Probe {
    fn process_stage(&mut self, stage: Stage, delta: u32) {
        dispatch(stage, delta, self).unwrap();
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.dropped.borrow_mut().push(self.id.get().raw());
    }
}
#[test]
fn native_ship_priorities_dispatch_registry_and_lifecycle_match_complete_cpp_snapshots() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/ship_stages/cases.json")).unwrap();
    for case in data["cases"].as_array().unwrap() {
        let profile = case["profile"].as_u64().unwrap() as usize;
        let mut manager = EntityManager::default();
        manager.set_layer_type(1, LayerType::Execute).unwrap();
        manager.set_layer_type(0, LayerType::Realize).unwrap();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let dropped = Rc::new(RefCell::new(Vec::new()));
        let mut ids = Vec::new();
        let mut ps = Vec::new();
        for frame in case["frames"].as_array().unwrap() {
            let step = frame["step"].as_u64().unwrap();
            let count = if step == 0 {
                2
            } else if step == 12 {
                1
            } else {
                0
            };
            for _ in 0..count {
                let n = ids.len();
                let values = [2, 31, -1, 0, i32::MAX, -2147483647];
                let priorities = Priorities::from_script(
                    (!profile.is_multiple_of(4)).then_some(values[(profile + n) % 6]),
                    (!profile.is_multiple_of(3)).then_some(values[(profile + 2 * n + 1) % 6]),
                );
                let handle = Rc::new(Cell::new(EntityId::INVALID));
                let id = manager
                    .insert(
                        "ship",
                        Probe {
                            id: handle.clone(),
                            calls: calls.clone(),
                            dropped: dropped.clone(),
                        },
                    )
                    .unwrap();
                handle.set(id);
                priorities.register(&mut manager, id, 1, 0).unwrap();
                ids.push(id);
                ps.push([priorities.execute, priorities.realize]);
            }
            if step == 4 {
                manager.freeze_layer(1, true).unwrap();
            }
            if step == 6 {
                manager.freeze_layer(1, false).unwrap();
            }
            if step == 8 {
                manager.erase(ids[0]);
            }
            if step == 14 {
                manager.remove_from_layer(0, ids[1]).unwrap();
            }
            calls.borrow_mut().clear();
            let delta = frame["delta"].as_u64().unwrap() as u32;
            manager.process_stage(Stage::Execute, delta).unwrap();
            manager.process_stage(Stage::Realize, delta).unwrap();
            let before = ids
                .iter()
                .map(|&id| json!([id.raw(), manager.is_valid(id), manager.get(id).is_some()]))
                .collect::<Vec<_>>();
            manager.new_lifecycle();
            let after = ids
                .iter()
                .map(|&id| json!([id.raw(), manager.is_valid(id), manager.get(id).is_some()]))
                .collect::<Vec<_>>();
            let actual = json!({"step":step,"delta":delta,"priorities":ps,"calls":*calls.borrow(),"before_lifecycle":before,"after_lifecycle":after,
                "execute_ids":manager.layer_ids(LayerType::Execute).iter().map(|i|i.raw()).collect::<Vec<_>>(),"realize_ids":manager.layer_ids(LayerType::Realize).iter().map(|i|i.raw()).collect::<Vec<_>>(),"dropped":*dropped.borrow()});
            assert_eq!(&actual, frame, "profile {profile}, step {step}");
        }
    }
}
#[test]
fn invalid_layer_pair_does_not_partially_register_a_ship() {
    let mut manager = EntityManager::default();
    manager.set_layer_type(0, LayerType::Realize).unwrap();
    let id = manager
        .insert(
            "ship",
            Probe {
                id: Rc::new(Cell::new(EntityId::INVALID)),
                calls: Rc::default(),
                dropped: Rc::default(),
            },
        )
        .unwrap();
    assert!(
        Priorities::from_script(None, None)
            .register(&mut manager, id, 32, 0)
            .is_err()
    );
    assert!(manager.layer_ids(LayerType::Realize).is_empty());
}
