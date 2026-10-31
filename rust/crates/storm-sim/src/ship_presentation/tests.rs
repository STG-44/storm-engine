use super::*;
use crate::{
    ship_motion::{FlatMovement, MotionState, Strength},
    ship_placement::ShipPlacement,
};
use serde_json::{Value as Json, json};
use std::collections::VecDeque;

#[derive(Default)]
struct Port {
    trace: VecDeque<[u32; 3]>,
    trig: Vec<[u32; 3]>,
    calls: Vec<String>,
}
impl Port {
    fn math(&mut self, kind: u32, value: f32) -> f32 {
        let expected = self.trace.pop_front().expect("unrecorded native trig");
        assert_eq!([kind, value.to_bits()], [expected[0], expected[1]]);
        let actual = if kind == 0 {
            libm::sinf(value)
        } else {
            libm::cosf(value)
        };
        let ordered = |bits: u32| {
            if bits >> 31 == 0 {
                bits | 0x80000000
            } else {
                !bits
            }
        };
        assert!(ordered(actual.to_bits()).abs_diff(ordered(expected[2])) <= 1);
        assert_eq!(actual.to_bits() >> 31, expected[2] >> 31);
        if actual == 0.0 || f32::from_bits(expected[2]) == 0.0 {
            assert_eq!(actual.to_bits(), expected[2]);
        }
        self.trig.push(expected);
        f32::from_bits(expected[2])
    }
}
impl MovementPort for Port {
    fn sin(&mut self, v: f32) -> f32 {
        self.math(0, v)
    }
    fn cos(&mut self, v: f32) -> f32 {
        self.math(1, v)
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
#[test]
fn living_frames_state_root_and_script_writes_match_all_native_snapshots() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/ship_presentation/cases.json")).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 48);
    for case in data["cases"].as_array().unwrap() {
        let f = |k: &str| case[k].as_f64().unwrap() as f32;
        let mut body = ShipMotion {
            state: MotionState {
                position: [3.0, 7.0, -5.0],
                angles: [0.125, f("yaw"), -0.2],
                speed: [2.0, 0.5, 8.0],
                rotation: [0.1, f("turn"), -0.1],
                inertia_accel: [0.5, 0.8, 0.25],
                inertia_brake: [0.2, 0.7, 0.45],
                mass_inertia: 0.1,
            },
            offsets: [1000.0, -2000.0],
            fixed_speed: case["fixed"].as_bool().unwrap().then_some(9.0),
            perk_angle: case["perk"].as_bool().unwrap().then_some(f("angle")),
            ..Default::default()
        };
        body.strengths[0].speed[2] = f("mainz");
        body.strengths[0].rotation[1] = 0.3;
        let mut port = Port {
            trace: case["snapshots"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|s| s["trig"].as_array().unwrap())
                .map(|r| serde_json::from_value(r.clone()).unwrap())
                .collect(),
            ..Default::default()
        };
        let mut store = AttributeStore::default();
        let character = store.create_root().unwrap();
        for (step, (delta, expected)) in [0, 17, 16, 250, 1000, 17]
            .into_iter()
            .zip(case["snapshots"].as_array().unwrap())
            .enumerate()
        {
            if step == 2 {
                body.strengths[1] = Strength {
                    used: true,
                    speed: [0.375, -1.0, 2.0],
                    rotation: [1.0, 0.8, -0.2],
                    ..Default::default()
                };
            }
            if step == 4 {
                body.strengths[0].speed[2] = -body.strengths[0].speed[2];
                body.strengths[0].rotation[1] = -body.strengths[0].rotation[1];
            }
            let pose = advance_living(&mut body, delta, &mut port).unwrap();
            let root = ShipPlacement {
                immersion: 0.75,
                waterline: 0.5,
            }
            .root(pose.position, pose.angles, body.offsets, [0.0; 2])
            .unwrap();
            let paths = ["ship.speed", "ship.pos", "ship.ang"];
            let created = paths
                .into_iter()
                .filter(|p| store.lookup_path(character, p).unwrap().is_none())
                .collect::<Vec<_>>();
            publish_character(&mut store, character, &body).unwrap();
            let writes = paths
                .into_iter()
                .flat_map(|p| {
                    let node = store.lookup_path(character, p).unwrap().unwrap();
                    store
                        .get(node)
                        .unwrap()
                        .children()
                        .iter()
                        .map(|id| {
                            json!([
                                format!("{p}.{}", store.name(*id).unwrap()),
                                store.value(*id, None).unwrap()
                            ])
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let s = body.state;
            let actual = json!({"state":[s.position.map(f32::to_bits),s.angles.map(f32::to_bits),s.speed.map(f32::to_bits),s.rotation.map(f32::to_bits)],
                "pose":[pose.position.map(f32::to_bits),pose.angles.map(f32::to_bits)],"root":[root.position.map(f32::to_bits),root.angles.map(f32::to_bits)],
                "accel":body.speed_accel.map(f32::to_bits),"strengths":body.strengths.iter().map(|s|json!([s.used,s.inertia,s.speed.map(f32::to_bits),s.rotation.map(f32::to_bits)])).collect::<Vec<_>>(),
                "calls":std::mem::take(&mut port.calls),"trig":std::mem::take(&mut port.trig),"created":created,"writes":writes});
            assert_eq!(actual, *expected, "case {} step {step}", case["id"]);
        }
        assert!(port.trace.is_empty());
    }
}
#[test]
fn zero_time_heel_is_clamped_and_never_accumulates_in_body_angles() {
    for rate in [-0.6, -0.25, -0.0, 0.0, 0.25, 0.6] {
        let mut body = ShipMotion::default();
        body.state.angles = [0.1, 0.0, -0.2];
        body.state.rotation[1] = rate;
        let mut port = FlatMovement::default();
        for _ in 0..8 {
            let pose = advance_living(&mut body, 0, &mut port).unwrap();
            assert_eq!(
                pose.angles[2].to_bits(),
                (-0.2 + rate.clamp(-0.25, 0.25)).to_bits()
            );
            assert_eq!(body.state.angles, [0.1, 0.0, -0.2]);
        }
    }
}
#[test]
fn dead_frames_and_invalid_output_bindings_fail_before_body_or_tree_changes() {
    let mut body = ShipMotion {
        dead: true,
        ..Default::default()
    };
    let original = body.state;
    let mut port = Port::default();
    assert!(advance_living(&mut body, 17, &mut port).is_err());
    assert_eq!(body.state, original);
    assert!(port.calls.is_empty());
    let mut store = AttributeStore::default();
    let root = store.create_root().unwrap();
    assert!(publish_character(&mut AttributeStore::default(), root, &body).is_err());
    let saved = store.len();
    body.state.rotation[1] = f32::NAN;
    assert!(publish_character(&mut store, root, &body).is_err());
    assert_eq!(store.len(), saved);
    body.state.rotation[1] = 0.0;
    body.state.position[0] = f32::MAX;
    body.offsets[0] = f32::MAX;
    assert!(publish_character(&mut store, root, &body).is_err());
    assert_eq!(store.len(), saved);
}
