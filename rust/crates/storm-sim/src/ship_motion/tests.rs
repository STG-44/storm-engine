use super::*;
use serde_json::{Value as Json, json};
use storm_core::Value;
#[derive(Default)]
struct Port {
    trace: Option<std::collections::VecDeque<[u32; 3]>>,
    trig: Vec<[u32; 3]>,
    calls: Vec<String>,
}
impl Port {
    fn math(&mut self, kind: u32, angle: f32) -> f32 {
        let actual = if kind == 0 {
            libm::sinf(angle)
        } else {
            libm::cosf(angle)
        };
        let Some(trace) = &mut self.trace else {
            return actual;
        };
        let expected = trace.pop_front().expect("unrecorded native trig call");
        assert_eq!([expected[0], expected[1]], [kind, angle.to_bits()]);
        let ordered = |bits: u32| {
            if bits & 0x80000000 == 0 {
                bits | 0x80000000
            } else {
                !bits
            }
        };
        assert!(
            ordered(actual.to_bits()).abs_diff(ordered(expected[2])) <= 1,
            "software/native trig diverged: {kind} {angle} {actual} {}",
            f32::from_bits(expected[2])
        );
        assert_eq!(actual.to_bits() >> 31, expected[2] >> 31, "trig sign");
        if actual == 0.0 || f32::from_bits(expected[2]) == 0.0 {
            assert_eq!(actual.to_bits(), expected[2], "trig signed zero");
        }
        self.trig.push(expected);
        f32::from_bits(expected[2])
    }
}
impl MovementPort for Port {
    fn sin(&mut self, angle: f32) -> f32 {
        self.math(0, angle)
    }
    fn cos(&mut self, angle: f32) -> f32 {
        self.math(1, angle)
    }
    fn max_speed_z(&mut self) -> f32 {
        self.calls.push("z".into());
        12.0
    }
    fn max_speed_y(&mut self) -> f32 {
        self.calls.push("y".into());
        0.08
    }
    fn rocking(&mut self, p: [f32; 3], mut a: [f32; 3], dt: f32) -> Result<(f32, [f32; 3])> {
        self.calls.push("rock".into());
        a[0] += 0.125 * dt;
        a[2] -= 0.25 * dt;
        Ok((p[1] + dt * 0.5, a))
    }
}
fn touch(t: Option<TouchParams>) -> Json {
    t.map_or(Json::Null, |t| {
        json!([
            t.speed.map(f32::to_bits),
            t.rotation.map(f32::to_bits),
            t.position.map(f32::to_bits),
            t.angles.map(f32::to_bits)
        ])
    })
}
fn snapshot(
    s: &ShipMotion,
    p: &mut Port,
    result: i32,
    old: Option<TouchParams>,
    new: Option<TouchParams>,
) -> Json {
    let state = s.state;
    json!({"state":[state.position.map(f32::to_bits),state.angles.map(f32::to_bits),state.speed.map(f32::to_bits),state.rotation.map(f32::to_bits),state.inertia_accel.map(f32::to_bits),state.inertia_brake.map(f32::to_bits),state.mass_inertia.to_bits()],
        "strengths":s.strengths.iter().map(|s|json!([s.used,s.inertia,s.speed.map(f32::to_bits),s.rotation.map(f32::to_bits)])).collect::<Vec<_>>(),"accel":s.speed_accel.map(f32::to_bits),"calls":std::mem::take(&mut p.calls),"trig":std::mem::take(&mut p.trig),"result":result,"old":touch(old),"new":touch(new)})
}
fn impulse(inertia: bool) -> Message {
    let mut args = vec![
        Value::Long(MSG_SHIP_ADD_MOVE_IMPULSE),
        Value::Long(if inertia { -7 } else { 0 }),
    ];
    args.extend([1.25, -0.5, 2.5, -0.25, 0.125, 0.5].map(Value::Float));
    Message::new("llffffff", args).unwrap()
}
#[test]
fn movement_strengths_and_touch_predictions_match_all_complete_native_snapshots() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/ship_motion/cases.json")).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 64);
    for c in data["cases"].as_array().unwrap() {
        let f = |k: &str| c[k].as_f64().unwrap() as f32;
        let b = |k: &str| c[k].as_bool().unwrap();
        let mut s = ShipMotion {
            state: MotionState {
                position: [3.0, 7.0, -5.0],
                angles: [0.1, f("yaw"), -0.2],
                speed: [2.0, 0.5, 8.0],
                rotation: [0.1, 0.02, -0.1],
                inertia_accel: [0.5, 0.8, 0.25],
                inertia_brake: [0.2, 0.7, 0.45],
                mass_inertia: f("mass"),
            },
            offsets: [1000.0, -2000.0],
            fixed_speed: b("fixed").then_some(9.0),
            perk_angle: b("perk").then_some(f("perk_angle")),
            dead: b("dead"),
            ..Default::default()
        };
        s.strengths[0].speed = [f("main_x"), 0.0, f("main_z")];
        s.strengths[0].rotation = [0.0, 0.3, 0.0];
        let trace = c["snapshots"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|s| s["trig"].as_array().unwrap().iter())
            .map(|row| {
                [
                    row[0].as_u64().unwrap() as u32,
                    row[1].as_u64().unwrap() as u32,
                    row[2].as_u64().unwrap() as u32,
                ]
            })
            .collect();
        let mut p = Port {
            trace: Some(trace),
            ..Default::default()
        };
        let mut actual = vec![snapshot(&s, &mut p, 0, None, None)];
        s.add_strength(Strength {
            speed: [0.375, -1.0, 2.0],
            rotation: [1.0, 0.05, -0.2],
            ..Default::default()
        })
        .unwrap();
        let mut t = Strength {
            inertia: true,
            speed: [5.0, 4.0, 3.0],
            rotation: [0.5, 0.25, 0.75],
            ..Default::default()
        };
        let r = s.add_strength(t).unwrap().unwrap() as i32;
        actual.push(snapshot(&s, &mut p, r, None, None));
        let r = s
            .process_message_with_port(&mut impulse(b("collision")), &mut p)
            .unwrap() as i32;
        actual.push(snapshot(&s, &mut p, r, None, None));
        let ms = c["delta_ms"].as_u64().unwrap() as u32;
        let preview = s.touch_move(ms, true, true, &mut p).unwrap().unwrap();
        actual.push(snapshot(&s, &mut p, 1, preview.old, preview.new));
        s.advance(ms, b("collision"), &mut p).unwrap();
        actual.push(snapshot(&s, &mut p, 1, None, None));
        s.advance(1000, false, &mut p).unwrap();
        actual.push(snapshot(&s, &mut p, 1, None, None));
        let r = i32::from(s.del_strength(2));
        actual.push(snapshot(&s, &mut p, r, None, None));
        t.inertia = false;
        let mut result = -1;
        for _ in 0..17 {
            result = s.add_strength(t).unwrap().map_or(-1, |i| i as i32);
        }
        actual.push(snapshot(&s, &mut p, result, None, None));
        for i in [-1, 0, 16] {
            assert!(!s.del_strength(i));
        }
        actual.push(snapshot(&s, &mut p, 0, None, None));
        assert!(s.del_strength(1));
        let r = s.add_strength(t).unwrap().unwrap() as i32;
        actual.push(snapshot(&s, &mut p, r, None, None));
        for ms in [17, 16, 17, 32] {
            s.advance(ms, false, &mut p).unwrap();
        }
        actual.push(snapshot(&s, &mut p, 1, None, None));
        for (i, (a, e)) in actual
            .iter()
            .zip(c["snapshots"].as_array().unwrap())
            .enumerate()
        {
            assert_eq!(a, e, "case {} step {i}", c["id"]);
        }
    }
}
#[test]
fn live_character_speed_limits_keep_missing_values_zero_and_reject_foreign_roots() {
    let mut store = AttributeStore::default();
    let root = store.create_root().unwrap();
    assert_eq!(
        FlatMovement::from_character(&store, root)
            .unwrap()
            .max_speed_z,
        0.0
    );
    let z = store.create_path(root, "ship.maxspeedz").unwrap();
    store.set_value(z, Some("13.5")).unwrap();
    let y = store.create_path(root, "Ship.MaxSpeedY").unwrap();
    store.set_value(y, Some("-0.125")).unwrap();
    let limits = FlatMovement::from_character(&store, root).unwrap();
    assert_eq!((limits.max_speed_z, limits.max_speed_y), (13.5, -0.125));
    store.set_value(z, Some("7.25")).unwrap();
    assert_eq!(
        FlatMovement::from_character(&store, root)
            .unwrap()
            .max_speed_z,
        7.25
    );
    assert!(FlatMovement::from_character(&AttributeStore::default(), root).is_err());
}
#[test]
fn malformed_impulses_and_nonfinite_movement_leave_the_live_body_unchanged() {
    let mut body = ShipMotion::default();
    let mut p = Port::default();
    let original = snapshot(&body, &mut p, 0, None, None);
    let mut message = Message::new(
        "llf",
        vec![
            Value::Long(MSG_SHIP_ADD_MOVE_IMPULSE),
            Value::Long(0),
            Value::Float(1.0),
        ],
    )
    .unwrap();
    assert!(body.process_message(&mut message).is_err());
    assert_eq!(snapshot(&body, &mut p, 0, None, None), original);
    assert!(
        body.add_strength(Strength {
            speed: [f32::NAN, 0.0, 0.0],
            ..Default::default()
        })
        .is_err()
    );
    body.state.mass_inertia = f32::MAX;
    body.state.inertia_accel = [f32::MAX; 3];
    body.strengths[0].speed = [1.0, 0.0, 1.0];
    let original = snapshot(&body, &mut p, 0, None, None);
    assert!(body.advance(1000, false, &mut p).is_err());
    p.calls.clear();
    assert_eq!(snapshot(&body, &mut p, 0, None, None), original);
    assert!(body.touch_move(1, false, false, &mut p).unwrap().is_none());
    assert!(p.calls.is_empty());
}
#[test]
fn partial_touch_outputs_preserve_impulses_and_zero_time_still_applies_them() {
    let mut body = ShipMotion::default();
    body.process_message(&mut impulse(false)).unwrap();
    let state = body.state;
    let mut port = FlatMovement::default();
    let preview = body.touch_move(0, false, true, &mut port).unwrap().unwrap();
    assert!(preview.old.is_none());
    assert_eq!(preview.new.unwrap().speed, [1.25, -0.5, 2.5]);
    assert_eq!(body.state, state);
    assert!(body.strengths[1].used);
    body.advance(0, false, &mut port).unwrap();
    assert_eq!(body.state.speed, [1.25, -0.5, 2.5]);
    assert!(!body.strengths[1].used);
}
