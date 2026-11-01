use super::*;
fn fleet(staged: bool) -> Fleet {
    let mut f = generated().unwrap();
    f.enable_native_motion();
    f.enable_native_rocking().unwrap();
    f.enable_native_sea(None).unwrap();
    f.enable_native_sea_weather().unwrap();
    f.enable_native_placement(0.3, 0.2).unwrap();
    f.enable_native_presentation().unwrap();
    f.enable_native_sea_stages().unwrap();
    if staged {
        f.enable_native_ship_stages().unwrap();
    }
    f
}
fn entities(f: &Fleet) -> Json {
    f.motion_report().unwrap()["rocking"]["sea"]["stages"]["ship_entities"].clone()
}
#[test]
fn entity_dispatch_runs_real_ship_bodies_before_sea_without_changing_numeric_outputs() {
    let mut direct = fleet(false);
    let mut staged = fleet(true);
    direct.advance_frames(3.0).unwrap();
    staged.advance_frames(3.0).unwrap();
    let mut a = direct.motion_report().unwrap();
    let mut b = staged.motion_report().unwrap();
    a["rocking"].as_object_mut().unwrap().remove("sea");
    b["rocking"].as_object_mut().unwrap().remove("sea");
    assert_eq!(a, b);
    let report = entities(&staged);
    let motion = staged.motion_report().unwrap();
    let trace = motion["rocking"]["sea"]["stages"]["trace"]
        .as_array()
        .unwrap();
    for ship in report["bodies"].as_array().unwrap() {
        for call in ship["trace"].as_array().unwrap() {
            let sea = &trace[call["frame"].as_u64().unwrap() as usize];
            let key = if call["stage"] == "execute" {
                "execute_revision"
            } else {
                "realize_revision"
            };
            assert_eq!(call["sea_revision"], sea[key]);
        }
    }
    let mut partitioned = fleet(true);
    for t in [0.25, 0.75, 1.5, 1.75, 2.25, 3.0] {
        partitioned.advance_frames(t).unwrap();
    }
    assert_eq!(staged.motion_report(), partitioned.motion_report());
}
#[test]
fn retired_ship_leaves_both_layers_and_surviving_ship_keeps_executing() {
    let mut f = fleet(true);
    f.advance_frames(2.0).unwrap();
    let retired = f.motion_report().unwrap();
    let r = entities(&f);
    assert_eq!(r["bodies"][0]["execute_calls"], 121);
    assert_eq!(r["bodies"][0]["realize_calls"], 121);
    assert_eq!(r["bodies"][0]["valid"], false);
    assert_eq!(r["bodies"][0]["lookup"], false);
    assert_eq!(r["bodies"][0]["destroyed"], true);
    assert_eq!(r["bodies"][1]["valid"], true);
    f.advance_frames(3.0).unwrap();
    let after = f.motion_report().unwrap();
    assert_eq!(retired["bodies"][0], after["bodies"][0]);
    assert_ne!(retired["bodies"][1], after["bodies"][1]);
    let r = entities(&f);
    assert_eq!(r["bodies"][0]["execute_calls"], 121);
    assert_eq!(r["bodies"][0]["realize_calls"], 121);
    assert_eq!(r["bodies"][1]["execute_calls"], 181);
    assert_eq!(r["bodies"][1]["realize_calls"], 181);
    let mut restarted = fleet(true);
    restarted.advance_frames(3.0).unwrap();
    assert_eq!(f.motion_report(), restarted.motion_report());
}
#[test]
fn paused_realize_never_advances_ship_and_numeric_callback_errors_reach_frame_owner() {
    let mut f = fleet(true);
    f.preview = true;
    f.advance_frames(1.25).unwrap();
    let before = f.motion_report().unwrap();
    let mut camera = f.camera();
    camera.distance = 4000.0;
    f.refresh_sea_view(&camera).unwrap();
    let after = f.motion_report().unwrap();
    assert_eq!(before["bodies"], after["bodies"]);
    let a = &before["rocking"]["sea"]["stages"]["ship_entities"]["bodies"][0];
    let b = &after["rocking"]["sea"]["stages"]["ship_entities"]["bodies"][0];
    assert_eq!(a["execute_calls"], b["execute_calls"]);
    assert_eq!(
        b["realize_calls"].as_u64().unwrap(),
        a["realize_calls"].as_u64().unwrap() + 1
    );
    f.refresh_sea_view(&camera).unwrap();
    assert_eq!(after, f.motion_report().unwrap());
    f.native_motion.as_mut().unwrap().inputs[0].max_speed_z = f32::NAN;
    assert!(f.advance_frames(1.267).is_err());
    assert_eq!(after["bodies"][0], f.motion_report().unwrap()["bodies"][0]);
}
