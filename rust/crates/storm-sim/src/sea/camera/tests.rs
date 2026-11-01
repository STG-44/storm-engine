use super::*;
use crate::sea::{Frames, Parameters, Sea};
use serde_json::{Value as Json, json};
use std::sync::Arc;
fn v<const N: usize>(j: &Json) -> [f32; N] {
    std::array::from_fn(|i| f32::from_bits(j[i].as_u64().unwrap() as u32))
}
#[test]
fn native_build_view_and_complete_rigid_transpose_match_cpp() {
    let data: Json =
        serde_json::from_str(include_str!("../../../reference/sea_camera/cases.json")).unwrap();
    for c in data["cases"].as_array().unwrap() {
        let (view, valid) = build_view(v(&c["from"]), v(&c["to"]), v(&c["up"])).unwrap();
        assert_eq!(json!(valid), c["valid"]);
        assert_eq!(json!(view.map(f32::to_bits)), c["view"]);
        let input = v(&c["input"]);
        let t = transpose(input).unwrap();
        assert_eq!(json!(t.map(f32::to_bits)), c["transposed"]);
        assert_eq!(
            reflect_view(reflect_view(input)).map(f32::to_bits),
            input.map(f32::to_bits)
        );
    }
}
#[test]
fn realize_camera_stop_zero_refresh_and_invalid_provider_are_atomic() {
    let mut sea = Sea::new(Arc::new(Frames::fixture()), Parameters::default()).unwrap();
    sea.enable_weather();
    let view = build_view([7., 8., -9.], [0.; 3], [0., 1., 0.]).unwrap().0;
    sea.set_realize_view(view);
    assert!(sea.realize(17).unwrap());
    let camera = Camera::from_view(view).unwrap();
    assert_eq!(sea.surface().camera, [camera.eye[0], camera.eye[2]]);
    let saved = sea.clone();
    let mut store = storm_core::AttributeStore::default();
    let root = store.create_root().unwrap();
    let stop = store.create_path(root, "Stop").unwrap();
    store.set_value(stop, Some("1")).unwrap();
    sea.attribute_changed(&store, root, stop).unwrap();
    sea.set_realize_view([f32::NAN; 16]);
    assert!(!sea.realize(u32::MAX).unwrap());
    assert_eq!(sea.revision(), saved.revision());
    assert_eq!(sea.realized_camera(), saved.realized_camera());
    store.set_value(stop, Some("0")).unwrap();
    sea.attribute_changed(&store, root, stop).unwrap();
    assert!(sea.realize(17).is_err());
    assert_eq!(sea.revision(), saved.revision());
    assert_eq!(sea.surface().animation, saved.surface().animation);
    sea.set_realize_view(view);
    let refresh = store.create_path(root, "isDone.refresh").unwrap();
    let before = sea.revision();
    sea.attribute_changed(&store, root, refresh).unwrap();
    assert_eq!(sea.revision(), before + 1);
    assert_eq!(sea.surface().animation, saved.surface().animation);
}
#[test]
fn native_core_stage_loops_query_old_state_and_retain_stopped_camera() {
    use std::{cell::RefCell, rc::Rc};
    use storm_core::{AttributeStore, Entity, EntityManager, LayerType, Stage};
    struct Query {
        id: u32,
        sea: Rc<RefCell<Sea>>,
        rows: Rc<RefCell<Vec<Json>>>,
    }
    impl Entity for Query {
        fn process_stage(&mut self, stage: Stage, _: u32) {
            if stage == Stage::Execute {
                let s = self.sea.borrow();
                let a = s.surface().animation;
                let c = s.realized_camera().unwrap().eye;
                self.rows.borrow_mut().push(json!([
                    self.id,
                    a.frame[0].to_bits(),
                    a.frame[1].to_bits(),
                    c[0].to_bits(),
                    c[1].to_bits(),
                    c[2].to_bits()
                ]));
            }
        }
    }
    struct Realize(Rc<RefCell<Sea>>);
    impl Entity for Realize {
        fn process_stage(&mut self, stage: Stage, delta: u32) {
            if stage == Stage::Realize {
                self.0.borrow_mut().realize(delta).unwrap();
            }
        }
    }
    let data: Json =
        serde_json::from_str(include_str!("../../../reference/sea_stages/cases.json")).unwrap();
    let frames = Arc::new(Frames::fixture());
    for case in data["cases"].as_array().unwrap() {
        let sea = Rc::new(RefCell::new(
            Sea::new(frames.clone(), Parameters::default()).unwrap(),
        ));
        sea.borrow_mut().enable_weather();
        sea.borrow_mut().set_realize_view(IDENTITY);
        sea.borrow_mut().realize(0).unwrap();
        let rows = Rc::new(RefCell::new(Vec::new()));
        let mut manager = EntityManager::default();
        manager.set_layer_type(0, LayerType::Execute).unwrap();
        manager.set_layer_type(1, LayerType::Realize).unwrap();
        let first = manager
            .insert(
                "query",
                Query {
                    id: 0,
                    sea: sea.clone(),
                    rows: rows.clone(),
                },
            )
            .unwrap();
        let second = manager
            .insert(
                "query",
                Query {
                    id: 1,
                    sea: sea.clone(),
                    rows: rows.clone(),
                },
            )
            .unwrap();
        let actor = manager.insert("sea", Realize(sea.clone())).unwrap();
        manager.add_to_layer(0, first, 0).unwrap();
        manager.add_to_layer(0, second, 0).unwrap();
        manager.add_to_layer(1, actor, 0).unwrap();
        let mut attributes = AttributeStore::default();
        let root = attributes.create_root().unwrap();
        let stop = attributes.create_path(root, "Stop").unwrap();
        let refresh = attributes.create_path(root, "isDone.refresh").unwrap();
        for row in case["frames"].as_array().unwrap() {
            let step = row["step"].as_u64().unwrap();
            let delta = row["delta"].as_u64().unwrap() as u32;
            sea.borrow_mut().set_realize_view(v(&row["view"]));
            if step == 4 || step == 7 {
                attributes
                    .set_value(stop, Some(if step == 4 { "1" } else { "0" }))
                    .unwrap();
                sea.borrow_mut()
                    .attribute_changed(&attributes, root, stop)
                    .unwrap();
            }
            if step == 5 || step == 7 {
                sea.borrow_mut()
                    .attribute_changed(&attributes, root, refresh)
                    .unwrap();
            }
            if step == 10 {
                manager.erase(first);
            }
            rows.borrow_mut().clear();
            manager.process_stage(Stage::Execute, delta).unwrap();
            manager.process_stage(Stage::Realize, delta).unwrap();
            let s = sea.borrow();
            let a = s.surface().animation;
            let c = s.realized_camera().unwrap();
            let w = s.weather().unwrap();
            assert_eq!(json!(*rows.borrow()), row["queries"]);
            assert_eq!(json!(a.frame.map(f32::to_bits)), row["frame"]);
            assert_eq!(
                json!(std::array::from_fn::<_, 2, _>(|i| [
                    a.movement[i][0],
                    w.movement_y[i],
                    a.movement[i][1]
                ]
                .map(f32::to_bits))),
                row["movement"]
            );
            assert_eq!(json!(c.eye.map(f32::to_bits)), row["eye"]);
            assert_eq!(json!(c.world_offset.map(f32::to_bits)), row["offset"]);
        }
    }
}
