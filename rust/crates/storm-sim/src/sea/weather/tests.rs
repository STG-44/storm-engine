use super::*;
use serde_json::{Value as Json, json};
fn snapshot(sea: &Sea, zero_requests: u32, zero_applied: u32) -> Json {
    let w = sea.weather().unwrap();
    let s = sea.surface();
    let p = s.parameters;
    let a = s.animation;
    json!({"raw":w.raw_amplitude.map(f32::to_bits),"amplitude":p.amplitude.map(f32::to_bits),"scale":p.scale.map(f32::to_bits),"speed":p.animation_speed.map(f32::to_bits),"frame":a.frame.map(f32::to_bits),
        "move_speed":std::array::from_fn::<_,2,_>(|i|[p.move_speed[i][0],w.speed_y[i],p.move_speed[i][1]].map(f32::to_bits)),
        "movement":std::array::from_fn::<_,2,_>(|i|[a.movement[i][0],w.movement_y[i],a.movement[i][1]].map(f32::to_bits)),
        "max_height":w.max_height.to_bits(),"max_distance":p.max_distance.to_bits(),"stopped":w.stopped,"zero_requests":zero_requests,"zero_applied":zero_applied})
}
#[test]
fn attribute_order_root_reads_partial_vectors_stop_and_zero_realize_match_cpp() {
    let data: Json =
        serde_json::from_str(include_str!("../../../reference/sea_weather/cases.json")).unwrap();
    let frames = Arc::new(Frames::fixture());
    for case in data["cases"].as_array().unwrap() {
        let mut sea = Sea::new(frames.clone(), Parameters::default()).unwrap();
        sea.enable_weather();
        let mut store = AttributeStore::default();
        let root = store.create_root().unwrap();
        let (mut requests, mut applied) = (0, 0);
        for (op, expected) in case["operations"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["snapshots"].as_array().unwrap())
        {
            if op[0] == "advance" {
                sea.advance(op[1].as_u64().unwrap() as u32).unwrap();
            } else {
                let changed = store.create_path(root, op[1].as_str().unwrap()).unwrap();
                if let Some(value) = op[2].as_str() {
                    store.set_value(changed, Some(value)).unwrap();
                }
                if op[0] == "notify" {
                    let stopped = sea.weather().unwrap().stopped;
                    let effect = sea.attribute_changed(&store, root, changed).unwrap();
                    if effect.realize_zero_requested {
                        requests += 1;
                        if !stopped {
                            applied += 1;
                        }
                    }
                }
            }
            assert_eq!(
                snapshot(&sea, requests, applied),
                *expected,
                "case {} operation {op}",
                case["id"]
            );
        }
    }
}
#[test]
fn pending_amplitudes_apply_on_notification_and_stop_retains_prepared_maps() {
    let mut sea = Sea::new(Arc::new(Frames::fixture()), Parameters::default()).unwrap();
    sea.enable_weather();
    let mut store = AttributeStore::default();
    let root = store.create_root().unwrap();
    let before = sea.surface().sample(3.125, -4.5).unwrap();
    for path in ["Sea2.Amp1", "Sea2.Amp2"] {
        let id = store.create_path(root, path).unwrap();
        store.set_value(id, Some("0")).unwrap();
        sea.attribute_changed(&store, root, id).unwrap();
    }
    assert_eq!(sea.surface().sample(3.125, -4.5).unwrap(), before);
    let id = store.create_path(root, "MaxSeaHeight").unwrap();
    store.set_value(id, Some("1")).unwrap();
    sea.attribute_changed(&store, root, id).unwrap();
    assert_eq!(sea.surface().sample(3.125, -4.5).unwrap().height, 0.0);
    let stop = store.create_path(root, "Stop").unwrap();
    store.set_value(stop, Some("1")).unwrap();
    sea.attribute_changed(&store, root, stop).unwrap();
    let saved = sea.surface().clone();
    sea.advance(u32::MAX).unwrap();
    assert!(Arc::ptr_eq(&saved.maps, &sea.surface().maps));
    assert_eq!(saved.animation, sea.surface().animation);
    let refresh = store.create_path(root, "isDone.refresh").unwrap();
    assert!(
        sea.attribute_changed(&store, root, refresh)
            .unwrap()
            .realize_zero_requested
    );
    assert!(Arc::ptr_eq(&saved.maps, &sea.surface().maps));
}
#[test]
fn bad_callbacks_and_unrepresentable_weather_keep_controller_state() {
    let mut sea = Sea::new(Arc::new(Frames::fixture()), Parameters::default()).unwrap();
    sea.enable_weather();
    let mut store = AttributeStore::default();
    let root = store.create_root().unwrap();
    let other = store.create_root().unwrap();
    let foreign = store.create_path(other, "Sea2.Amp1").unwrap();
    assert!(sea.attribute_changed(&store, root, foreign).is_err());
    assert!(sea.attribute_changed(&store, root, root).is_err());
    for (path, value) in [
        ("Sea2.Amp1", "nan"),
        ("Sea2.Scale1", "inf"),
        ("Sea2.AnimSpeed1", "-1"),
        ("Sea2.MoveSpeed1", "1, 1e999, 2"),
        ("Sea2.MoveSpeed1", "0x1p2, 0, 1"),
        ("Sea2.MoveSpeed1", "1e+, 0, 1"),
    ] {
        let id = store.create_path(root, path).unwrap();
        store.set_value(id, Some(value)).unwrap();
        let saved = sea.clone();
        assert!(
            sea.attribute_changed(&store, root, id).is_err(),
            "{path}={value}"
        );
        assert_eq!(snapshot(&sea, 0, 0), snapshot(&saved, 0, 0));
        assert!(Arc::ptr_eq(&sea.surface.maps, &saved.surface.maps));
    }
}
