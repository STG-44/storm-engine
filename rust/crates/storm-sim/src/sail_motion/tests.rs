use super::*;
use serde_json::{Value as Json, json};
#[derive(Default)]
struct Node {
    position: [f32; 3],
    globals: u32,
    calls: Vec<Vec<u32>>,
}
impl NodePort for Node {
    fn position(&mut self) -> [f32; 3] {
        self.calls.push(vec![0]);
        self.position
    }
    fn set_position(&mut self, v: [f32; 3]) {
        self.position = v;
        self.calls
            .push(vec![1, v[0].to_bits(), v[1].to_bits(), v[2].to_bits()]);
    }
    fn rotate_y(&mut self, a: f32) {
        self.calls.push(vec![2, a.to_bits()]);
    }
    fn update_global(&mut self) {
        self.globals += 1;
        self.calls.push(vec![3]);
    }
}
fn snapshot(s: &SailMotion, n: &mut Node) -> Json {
    let value = json!({"geometry":s.geometry.map(|v|v.map(f32::to_bits)),"rolling":u8::from(s.rolling),"angle":s.old_wind_angle.to_bits(),
 "transition":s.transition.map(|t|json!({"geometry":t.geometry.map(|v|v.map(f32::to_bits)),"delta":t.delta.to_bits(),"up":u8::from(t.up)})),
 "position":n.position.map(f32::to_bits),"globals":n.globals,"ties":s.ties.map(|v|v.map(|p|p.map(f32::to_bits))),"calls":n.calls});
    n.calls.clear();
    value
}
#[test]
fn rolling_geometry_reversal_turns_and_node_calls_match_native_snapshots() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/sail_motion/cases.json")).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 64);
    enum Action {
        Set(bool),
        Step(u32),
        Turn(f32, f32),
    }
    for id in 0..64 {
        let kind = [
            Kind::Triangle,
            Kind::Square,
            Kind::Trapezoid,
            Kind::Gerbed,
            Kind::Rolling,
            Kind::Special,
        ][id % 6];
        let mut s = SailMotion::new(kind, id % 2 != 0);
        s.old_wind_angle = (id as i32 % 3 - 1) as f32 * 0.125;
        s.minimum_angle = 0.6 + (id % 4) as f32 * 0.125;
        s.maximum_angle = 0.6 - (id % 3) as f32 * 0.2;
        s.geometry = std::array::from_fn(|j| {
            [
                id as f32 * 0.125 + j as f32,
                -(j as f32) * 0.25,
                (id % 5) as f32 * 0.5 + 0.125,
            ]
        });
        s.hard_points = std::array::from_fn(|i| [i as f32, id as f32 * 0.25, -(i as f32)]);
        s.ties = std::array::from_fn(|i| (id & (1 << i) != 0).then_some([-(i as f32), 1.0, 2.0]));
        let mut node = Node {
            position: [3.0 + id as f32, -2.0, 0.25],
            ..Default::default()
        };
        let rows = &data["cases"][id]["snapshots"];
        assert_eq!(snapshot(&s, &mut node), rows[0]);
        let x = (id % 5) as f32 * 0.5 - 1.0;
        let rate = [0.0003, 0.001, 0.0, -0.0001][id % 4];
        let actions = [
            Action::Set(id % 2 == 0),
            Action::Set(id % 2 == 0),
            Action::Step(0),
            Action::Step(1000),
            Action::Turn(x, 0.034),
            Action::Step(1000),
            Action::Set(id % 2 != 0),
            Action::Step(2000),
            Action::Step(2000),
            Action::Turn(1.0 - (id % 5) as f32 * 0.5, 0.1),
            Action::Set(false),
            Action::Step(4000),
            Action::Step(0),
            Action::Step(17),
            Action::Set(true),
            Action::Step(4000),
        ];
        for (i, action) in actions.into_iter().enumerate() {
            match action {
                Action::Set(up) => s.set_rolling(up, 0.002, &mut node).unwrap(),
                Action::Step(ms) => s.advance(ms, rate).unwrap(),
                Action::Turn(x, limit) => s.turn(x, limit, 0.01, &mut node).unwrap(),
            }
            assert_eq!(
                snapshot(&s, &mut node),
                rows[i + 1],
                "profile {id} step {i}"
            );
        }
    }
}
#[test]
fn strict_roll_endpoints_raw_overshoot_and_checked_errors_are_retained() {
    let mut n = Node::default();
    let mut s = SailMotion::new(Kind::Triangle, false);
    s.geometry = [[0.25, -0.5, 1.0]; 10];
    let geometry = s.geometry;
    s.set_rolling(true, 0.002, &mut n).unwrap();
    s.advance(1000, 0.001).unwrap();
    assert!(!s.rolling);
    assert_eq!(s.transition.unwrap().delta, 0.0);
    s.advance(1, 0.001).unwrap();
    assert!(s.rolling);
    assert!(s.transition.is_none());
    assert_eq!(s.geometry, geometry);
    s.set_rolling(false, 0.002, &mut n).unwrap();
    s.advance(1000, 0.001).unwrap();
    assert_eq!(s.transition.unwrap().delta, 1.0);
    s.advance(1, 0.001).unwrap();
    assert!(s.transition.unwrap().delta > 1.0);
    assert_eq!(s.deployment(), 1.0);
    s.advance(0, 0.001).unwrap();
    assert!(!s.rolling);
    assert!(s.transition.is_none());
    assert_eq!(s.geometry, geometry);
    let before = s.clone();
    assert!(s.advance(1, f32::NAN).is_err());
    assert_eq!(s, before);
    assert!(s.turn(f32::NAN, 0.1, 0.01, &mut n).is_err());
    assert_eq!(s, before);
    n.position = [f32::NAN; 3];
    s.rolling = true;
    let before = s.clone();
    assert!(s.set_rolling(false, 0.002, &mut n).is_err());
    assert_eq!(s, before);
}
