use super::*;
fn fleet(cleanup: bool) -> Fleet {
    let mut f = generated().unwrap();
    f.enable_native_motion();
    f.enable_native_rocking().unwrap();
    f.enable_native_sea(None).unwrap();
    f.enable_native_sea_weather().unwrap();
    f.enable_native_placement(0.0, 0.0).unwrap();
    f.enable_native_presentation().unwrap();
    f.enable_native_parameters(None).unwrap();
    for a in &mut f.assets {
        a.add_sail_rope_fixture().unwrap();
    }
    f.enable_native_drive(1.0, 2.3561945).unwrap();
    f.drive
        .as_mut()
        .unwrap()
        .enable_motion(&mut f.assets)
        .unwrap();
    f.drive
        .as_mut()
        .unwrap()
        .enable_ropes(&mut f.assets)
        .unwrap();
    f.cloth = Some(waves::ClothFleet::with_rolls(&mut f.assets).unwrap());
    f.enable_native_sea_stages().unwrap();
    f.enable_native_ship_stages().unwrap();
    if cleanup {
        f.enable_native_ship_cleanup().unwrap();
    }
    f
}
fn cleanup(f: &Fleet) -> Json {
    f.motion_report().unwrap()["rocking"]["sea"]["stages"]["ship_entities"]["cleanup"].clone()
}
#[test]
fn ship_destructor_routes_real_group_owners_and_releases_only_retired_preview_effects() {
    let mut f = fleet(true);
    f.advance_frames(3.0).unwrap();
    let c = cleanup(&f);
    let model = &c["models"][0];
    let ship = model["ship"].clone();
    let modelid = model["model"].clone();
    assert_eq!(c["retired_owners"], json!([1]));
    assert_eq!(c["preview_cleanup_applied"], json!([true, false]));
    assert_eq!(c["tracks"], json!([c["models"][1]["ship"]]));
    for g in c["groups"].as_array().unwrap() {
        assert_eq!(g["owners"][0][1], true);
        assert_eq!(g["owners"][1][1], false);
        assert_eq!(g["pending"], true);
    }
    assert_eq!(model["model_valid"], false);
    assert_eq!(model["model_lookup"], false);
    assert_eq!(model["blots_lookup"], false);
    assert_eq!(c["models"][1]["model_valid"], true);
    let calls = c["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v[0] == "send")
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 6);
    for (i, v) in calls.iter().enumerate() {
        assert_eq!(v[2], CODES[i]);
        assert_eq!(
            v[3],
            if i == 0 {
                ship.clone()
            } else {
                modelid.clone()
            }
        );
        assert_eq!(v[4], true);
        assert_eq!(v[5], true);
        assert_eq!(v[6], false);
    }
    let drops = c["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v[0] == "drop")
        .map(|v| v[2].clone())
        .collect::<Vec<_>>();
    assert_eq!(drops, vec![json!("ship"), json!("blots"), json!("model")]);
    assert!(f.drive.as_ref().unwrap().groups[0].deleted);
    assert!(!f.drive.as_ref().unwrap().groups[1].deleted);
    assert!(f.sessions[0].controller.is_none());
    assert!(f.sessions[1].controller.is_some());
    assert_eq!(
        f.report().unwrap()["service"]["manager"]["owners"],
        json!([2])
    );
}
#[test]
fn cleanup_preserves_native_ship_and_cloth_outputs_and_fixed_step_partition() {
    let mut a = fleet(true);
    let mut b = fleet(true);
    let mut old = fleet(false);
    a.advance_frames(3.0).unwrap();
    old.advance_frames(3.0).unwrap();
    for t in [0.25, 0.75, 1.5, 1.75, 2.25, 3.0] {
        b.advance_frames(t).unwrap();
    }
    assert_eq!(a.report().unwrap(), b.report().unwrap());
    let aa = a.motion_report().unwrap();
    let bb = old.motion_report().unwrap();
    assert_eq!(aa["bodies"], bb["bodies"]);
    assert_eq!(aa["rocking"]["bodies"], bb["rocking"]["bodies"]);
    assert_eq!(
        a.drive.as_ref().unwrap().report(),
        old.drive.as_ref().unwrap().report()
    );
    let released = a
        .snapshots
        .iter()
        .find(|v| v["phase"] == "first_released")
        .unwrap();
    assert_eq!(released["native_motion"]["bodies"][0], aa["bodies"][0]);
    assert_ne!(released["native_motion"]["bodies"][1], aa["bodies"][1]);
}
#[test]
fn repeated_erase_is_idempotent_and_fresh_session_restores_all_services() {
    let mut f = fleet(true);
    f.advance_frames(3.0).unwrap();
    let before = f.report().unwrap();
    let native = f
        .native_motion
        .as_mut()
        .unwrap()
        .rocking
        .as_mut()
        .unwrap()
        .native
        .as_mut()
        .unwrap();
    native.erase_ship(0).unwrap();
    native.finish_lifecycle().unwrap();
    f.apply_ship_cleanup().unwrap();
    assert_eq!(before, f.report().unwrap());
    let mut restarted = fleet(true);
    restarted.advance_frames(3.0).unwrap();
    assert_eq!(before, restarted.report().unwrap());
}
