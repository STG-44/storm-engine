use super::*;
use crate::sail_motion::Rolling;
use serde_json::{Value as Json, json};
fn f(v: &Json) -> f32 {
    f32::from_bits(v.as_u64().unwrap() as u32)
}
fn vector(v: &Json) -> V {
    std::array::from_fn(|i| f(&v[i]))
}
fn geometry(v: &Json) -> Geometry {
    std::array::from_fn(|i| vector(&v[i]))
}
fn rolling(v: &Json) -> Option<Rolling> {
    (!v.is_null()).then(|| Rolling {
        geometry: geometry(&v["geometry"]),
        delta: f(&v["delta"]),
        up: v["up"] == 1,
    })
}
#[test]
fn geometry_saved_targets_full_vertices_and_all_uv_sets_match_native() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/sail_geometry/cases.json")).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 48);
    for c in data["cases"].as_array().unwrap() {
        let kind = [
            Kind::Triangle,
            Kind::Square,
            Kind::Trapezoid,
            Kind::Gerbed,
            Kind::Rolling,
            Kind::Special,
        ][c["kind"].as_u64().unwrap() as usize];
        let mut s = SailMotion::new(kind, false);
        s.hard_points = std::array::from_fn(|i| vector(&c["hard_points"][i]));
        s.geometry = geometry(&c["initial_geometry"]);
        s.transition = rolling(&c["initial_transition"]);
        let p = &c["parameters"];
        let sphere = rebuild(
            &mut s,
            Parameters {
                deep_z: f(&p[0]),
                deep_h: f(&p[1]),
                deep_vh: f(&p[2]),
                deep_vz: f(&p[3]),
                hole_flex: f(&p[4]),
                holes: c["holes"].as_i64().unwrap() as i32,
            },
        )
        .unwrap();
        for step in 0..2 {
            if step == 1 {
                s.advance(2000, 0.0003).unwrap();
            }
            let vertices = fill(&s, sphere, [f(&c["gerald"][0]), f(&c["gerald"][1])]).unwrap();
            let actual = json!({"geometry":s.geometry.map(|v|v.map(f32::to_bits)),"transition":s.transition.map(|t|json!({"geometry":t.geometry.map(|v|v.map(f32::to_bits)),"delta":t.delta.to_bits(),"up":u8::from(t.up)})),"rolled":u8::from(s.rolling),"sphere":[sphere.center[0].to_bits(),sphere.center[1].to_bits(),sphere.center[2].to_bits(),sphere.radius.to_bits()],"vertices":vertices.iter().map(|v|v.position.into_iter().chain(v.normal).chain(v.uv.into_iter().flatten()).map(f32::to_bits).collect::<Vec<_>>()).collect::<Vec<_>>()});
            assert_eq!(
                actual, c["snapshots"][step],
                "profile {} step {step}",
                c["id"]
            );
        }
    }
}
#[test]
fn invalid_geometry_is_transactional_and_native_grid_anchors_are_retained() {
    let mut s = SailMotion::new(Kind::Square, false);
    s.hard_points = [
        [0.0, 3.0, 0.0],
        [4.0, 3.0, 0.0],
        [0.0, 0.0, 0.0],
        [4.0, 0.0, 0.0],
    ];
    let p = preview_parameters(s.kind, s.hard_points, false, 0, 0.01).unwrap();
    let sphere = rebuild(&mut s, p).unwrap();
    let vertices = fill(&s, sphere, [1.0; 2]).unwrap();
    assert_eq!(vertices.len(), 221);
    for (i, h) in [(0, 0), (204, 1), (16, 2), (220, 3)] {
        assert_eq!(vertices[i].position, s.hard_points[h]);
    }
    let before = s.clone();
    assert!(
        rebuild(
            &mut s,
            Parameters {
                deep_z: f32::NAN,
                ..p
            }
        )
        .is_err()
    );
    assert_eq!(s, before);
    assert!(
        fill(
            &s,
            Sphere {
                radius: f32::INFINITY,
                ..sphere
            },
            [1.0; 2]
        )
        .is_err()
    );
    let mut overflowing = s.clone();
    overflowing.hard_points = [[f32::MAX; 3]; 4];
    let before = overflowing.clone();
    assert!(rebuild(&mut overflowing, p).is_err());
    assert_eq!(overflowing, before);
    assert!(preview_parameters(Kind::Square, [[0.0; 3]; 4], false, 0, 0.01).is_err());
}
