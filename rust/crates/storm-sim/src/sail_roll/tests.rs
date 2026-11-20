use super::*;
use crate::sail_topology::{self, Grid};
use serde_json::{Value as Json, json};
fn f(v: &Json) -> f32 {
    f32::from_bits(v.as_u64().unwrap() as u32)
}
fn vector(v: &Json) -> V {
    std::array::from_fn(|i| f(&v[i]))
}
fn snapshot(s: &State, v: &[Vertex]) -> Json {
    json!({"phase":[s.vertical,s.horizontal],"wind_up":u8::from(s.wind_up),"sum":s.sum_wind.to_bits(),"maximum":s.maximum_sum_wind.to_bits(),"points":s.points.map(|v|v.map(f32::to_bits)),"ropes":s.rope_points.map(|v|v.map(f32::to_bits)),"box_center":s.box_center.map(f32::to_bits),"box_size":s.box_size.map(f32::to_bits),"vertices":v.iter().map(|v|v.position.into_iter().chain(v.normal).chain(v.uv.into_iter().flatten()).map(f32::to_bits).collect::<Vec<_>>()).collect::<Vec<_>>()})
}
#[test]
fn rolled_vertices_phase_ropes_and_complete_native_frames_match() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/sail_roll/cases.json")).unwrap();
    assert_eq!(data["rolls"].as_array().unwrap().len(), 60);
    let table = [
        0.0, 0.25, 0.5, 0.75, 1.0, 0.5, 0.25, 0.0, -0.25, -0.75, -1.0, -0.5, -0.25,
    ];
    for c in data["rolls"].as_array().unwrap() {
        let kind = [
            Kind::Triangle,
            Kind::Square,
            Kind::Trapezoid,
            Kind::Gerbed,
            Kind::Rolling,
            Kind::Special,
        ][c["kind"].as_u64().unwrap() as usize];
        let mut m = SailMotion::new(kind, true);
        m.hard_points = std::array::from_fn(|i| vector(&c["hard_points"][i]));
        m.geometry = std::array::from_fn(|i| vector(&c["geometry"][i]));
        let i = &c["initial"];
        let mut s = State {
            vertical: i["phase"][0].as_u64().unwrap() as usize,
            horizontal: i["phase"][1].as_u64().unwrap() as usize,
            wind_up: i["wind_up"] == 1,
            sum_wind: f(&i["sum"]),
            maximum_sum_wind: f(&i["maximum"]),
            points: std::array::from_fn(|n| vector(&i["points"][n])),
            rope_points: std::array::from_fn(|n| vector(&i["ropes"][n])),
            box_center: vector(&i["box_center"]),
            box_size: vector(&i["box_size"]),
        };
        let mut vertices = i["vertices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| Vertex {
                position: std::array::from_fn(|n| f(&v[n])),
                normal: std::array::from_fn(|n| f(&v[n + 3])),
                uv: std::array::from_fn(|a| std::array::from_fn(|b| f(&v[6 + a * 2 + b]))),
            })
            .collect::<Vec<_>>();
        let p = &c["roll_parameters"];
        let parameters = Parameters {
            z: f(&p[0]),
            y: f(&p[1]),
            wind_delta: f(&p[2]),
            triangle_multiplier: f(&p[3]),
            square_form: std::array::from_fn(|n| f(&c["square_form"][n])),
            triangle_form: std::array::from_fn(|n| f(&c["triangle_form"][n])),
            turning: c["turning"] == 1,
            ties: std::array::from_fn(|n| c["ties"][n] == 1),
            height: f(&c["height"]),
        };
        let topology =
            sail_topology::fill(kind, true, Grid::ROLLED, c["mask"].as_u64().unwrap() as u16)
                .unwrap();
        let topology = json!({"normal":topology.normal_count,"hole_start":topology.hole_start,"holes":topology.hole_count,"indices":topology.indices});
        for frame in 0..3 {
            advance(
                &mut s,
                &m,
                &mut vertices,
                parameters,
                &table,
                c["increment"].as_u64().unwrap() as u32,
            )
            .unwrap();
            let actual = snapshot(&s, &vertices);
            let expected = &c["frames"][frame];
            for key in [
                "phase",
                "wind_up",
                "sum",
                "maximum",
                "points",
                "ropes",
                "box_center",
                "box_size",
            ] {
                assert_eq!(
                    actual[key], expected[key],
                    "case {} frame {frame} {key}",
                    c["id"]
                );
            }
            for (n, v) in actual["vertices"].as_array().unwrap().iter().enumerate() {
                assert_eq!(
                    v, &expected["vertices"][n],
                    "case {} frame {frame} vertex {n}",
                    c["id"]
                );
            }
            assert_eq!(topology, expected["topology"], "case {} topology", c["id"]);
        }
    }
}
#[test]
fn invalid_rolled_input_and_overflow_are_transactional() {
    let mut m = SailMotion::new(Kind::Square, true);
    m.hard_points = [
        [0.0, 3.0, 0.0],
        [4.0, 3.0, 0.0],
        [0.0, 0.0, 0.0],
        [4.0, 0.0, 0.0],
    ];
    m.geometry[5] = [0.0, 0.0, 1.0];
    let mut s = State::default();
    let mut vertices = vec![Vertex::default(); 221];
    let before = (s.clone(), vertices.clone());
    for p in [
        Parameters {
            height: f32::NAN,
            ..Default::default()
        },
        Parameters {
            z: f32::MAX,
            height: f32::MAX,
            ..Default::default()
        },
    ] {
        assert!(advance(&mut s, &m, &mut vertices, p, &[0.0, 1.0], 6).is_err());
        assert_eq!((s.clone(), vertices.clone()), before);
    }
    for table in [&[][..], &[f32::INFINITY][..]] {
        assert!(advance(&mut s, &m, &mut vertices, Default::default(), table, 6).is_err());
        assert_eq!((s.clone(), vertices.clone()), before);
    }
    m.rolling = false;
    assert!(advance(&mut s, &m, &mut vertices, Default::default(), &[0.0], 6).is_err());
    assert_eq!((s, vertices), before);
}
