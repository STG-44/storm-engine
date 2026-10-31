use super::*;
use serde_json::{Value, json};

struct Port {
    id: u32,
    bases: [f32; 7],
    calls: Vec<[u32; 6]>,
}
impl MotionPort for Port {
    fn sample(&mut self, curve: Curve, age: f32, life: f32) -> f32 {
        let index = curve as usize;
        let k = 0.25 + 0.125 * self.id as f32;
        let value =
            (self.bases[index] + [10.0, -2.5, 4.0, 0.25, -0.5, 1.0, 0.125][index] * age) + k * 0.25;
        self.calls.push([
            self.id,
            index as u32,
            age.to_bits(),
            life.to_bits(),
            k.to_bits(),
            value.to_bits(),
        ]);
        value
    }
}
fn snapshot(pool: &[(u32, Billboard)], calls: &mut Vec<[u32; 6]>, freed: &mut Vec<u32>) -> Value {
    let bodies = pool
        .iter()
        .map(|(id, b)| {
            let mut row = vec![
                *id,
                b.age.to_bits(),
                b.lifetime.to_bits(),
                b.mass.to_bits(),
                b.spin.to_bits(),
                b.angle.to_bits(),
                b.render_angle.to_bits(),
            ];
            row.extend(
                [
                    b.position,
                    b.velocity,
                    b.force,
                    b.render_position,
                    b.old_render_position,
                ]
                .into_iter()
                .flatten()
                .map(f32::to_bits),
            );
            row
        })
        .collect::<Vec<_>>();
    json!({"active":pool.len(),"bodies":bodies,"calls":std::mem::take(calls),"freed":std::mem::take(freed)})
}
#[test]
fn native_motion_and_pool_order_match_all_cpp_state_and_graph_calls() {
    let data: Value =
        serde_json::from_str(include_str!("../../reference/particle_motion/cases.json")).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 96);
    for case in data["cases"].as_array().unwrap() {
        let f = |key: &str| case[key].as_f64().unwrap() as f32;
        let mut pool = (0..3)
            .map(|id| {
                let mut t = Transform::default();
                t.0[3] = [id as f32, -(id as f32) * 0.5, 0.25];
                if id == 2 {
                    t.0[0] = [0.0, 0.0, -2.0];
                    t.0[2] = [0.5, 0.0, 0.0];
                }
                (
                    id,
                    Billboard::new(
                        [0.5, -2.0, 4.0],
                        [1.25, -0.5, 2.0],
                        [0.125, 0.75, 5.0][id as usize],
                        f("mass"),
                        1.25 * (id + 1) as f32,
                        t,
                    ),
                )
            })
            .collect::<Vec<_>>();
        let mut calls = vec![];
        let mut freed = vec![];
        let mut snapshots = vec![snapshot(&pool, &mut calls, &mut freed)];
        for (frame, delta) in data["deltas"].as_array().unwrap().iter().enumerate() {
            let mut i = 0;
            while i < pool.len() {
                let (id, body) = &mut pool[i];
                body.force = [frame as f32 * 0.125, -0.75, 0.25];
                let mut port = Port {
                    id: *id,
                    bases: [
                        f("drag"),
                        f("gravity"),
                        *id as f32 * 60.0,
                        -0.5,
                        1.5,
                        2.0,
                        f("blend"),
                    ],
                    calls: vec![],
                };
                let alive = body
                    .process(delta.as_f64().unwrap() as f32, &mut port)
                    .unwrap();
                calls.extend(port.calls);
                if alive {
                    i += 1;
                } else {
                    freed.push(*id);
                    pool.swap_remove(i);
                }
            }
            snapshots.push(snapshot(&pool, &mut calls, &mut freed));
        }
        assert_eq!(json!(snapshots), case["expected"], "{case}");
    }
}
struct Constant([f32; 7]);
impl MotionPort for Constant {
    fn sample(&mut self, c: Curve, _: f32, _: f32) -> f32 {
        self.0[c as usize]
    }
}
#[test]
fn drag_limits_displacement_without_damping_velocity_and_signed_mass_reverses_gravity() {
    let body = Billboard::new(
        [0.0; 3],
        [4.0, 0.0, 0.0],
        10.0,
        2.0,
        1.0,
        Transform::default(),
    );
    let mut blocked = body.clone();
    blocked
        .process(
            0.5,
            &mut Constant([100.0, 100.0, 100.0, 0.0, 0.0, 0.0, 0.0]),
        )
        .unwrap();
    assert_eq!(blocked.velocity, [4.0, -4.9, 0.0]);
    assert_eq!(blocked.position, [0.0; 3]);
    assert_eq!(blocked.angle, 0.0);
    let mut negative = body;
    negative.mass = -2.0;
    negative
        .process(0.5, &mut Constant([0.0, 100.0, 0.0, 0.0, 0.0, 0.0, 0.0]))
        .unwrap();
    assert_eq!(negative.velocity, [4.0, 4.9, 0.0]);
    assert_eq!(negative.position, [2.0, 2.45, 0.0]);
    assert_eq!(negative.angle, 0.5);
    assert_eq!(negative.force, [0.0; 3]);
}
#[test]
fn invalid_motion_is_atomic_and_expired_particles_skip_graphs() {
    let original = Billboard::new([0.0; 3], [0.0; 3], 1.0, 0.0, 0.0, Transform::default());
    let mut body = original.clone();
    assert!(body.process(f32::NAN, &mut Constant([0.0; 7])).is_err());
    assert_eq!(body, original);
    assert!(body.process(0.5, &mut Constant([f32::NAN; 7])).is_err());
    assert_eq!(body, original);
    let mut port = Constant([f32::NAN; 7]);
    assert!(!body.process(1.25, &mut port).unwrap());
    assert_eq!(body.age, 1.25);
    let mut body = original.clone();
    body.velocity = [f32::MAX; 3];
    let before = body.clone();
    assert!(body.process(-2.0, &mut Constant([0.0; 7])).is_err());
    assert_eq!(body, before);
}
