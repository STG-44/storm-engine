use super::*;
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
fn free_vertices_matrix_random_consumption_and_native_nonfinite_outcomes_match() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/sail_free/cases.json")).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 72);
    let (mut finite, mut rejected, mut vertex_count) = (0, 0, 0);
    for c in data["cases"].as_array().unwrap() {
        let kind = [
            Kind::Triangle,
            Kind::Square,
            Kind::Trapezoid,
            Kind::Gerbed,
            Kind::Rolling,
            Kind::Special,
        ][c["kind"].as_u64().unwrap() as usize];
        let mut m = SailMotion::new(kind, false);
        m.hard_points = std::array::from_fn(|i| vector(&c["hard_points"][i]));
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
        let p = &c["parameters"];
        let params = Parameters {
            height: f(&p[0]),
            wind_base: f(&p[1]),
            addition_min: f(&p[2]),
            addition_random: f(&p[3]),
            sail_wind: vector(&c["wind"]),
            axes: std::array::from_fn(|n| vector(&c["axes"][n])),
        };
        for frame in 0..3 {
            let draws = c["draws"][frame]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect::<Vec<_>>();
            let before = snapshot(&s, &vertices);
            let result = advance(
                &mut s,
                &m,
                &mut vertices,
                params,
                Draws {
                    values: &draws,
                    maximum: c["rand_max"].as_u64().unwrap() as u32,
                },
            );
            let expected = &c["frames"][frame];
            assert_eq!(expected["draws_used"], draw_count(kind));
            if expected["finite"] == 0 {
                rejected += 1;
                assert!(result.unwrap_err().is::<NativeNonFinite>());
                assert_eq!(
                    snapshot(&s, &vertices),
                    before,
                    "non-finite native frame must not poison state"
                );
                continue;
            }
            finite += 1;
            vertex_count += vertices.len();
            assert_eq!(result.unwrap(), draw_count(kind));
            let actual = snapshot(&s, &vertices);
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
        }
    }
    assert_eq!((finite, rejected, vertex_count), (180, 36, 37740));
}
#[test]
fn free_input_checks_and_coarse_topology_preserve_atomic_state() {
    let mut m = SailMotion::new(Kind::Square, false);
    m.hard_points[1] = [3.0, 0.0, 0.0];
    let mut s = State {
        points: [[0.0, 1.0, 1.0]; 20],
        ..Default::default()
    };
    let mut v = vec![Vertex::default(); 221];
    let before = (s.clone(), v.clone());
    let samples = [16383; 16];
    for draws in [
        Draws {
            values: &samples[..15],
            maximum: 32767,
        },
        Draws {
            values: &samples,
            maximum: 0,
        },
        Draws {
            values: &[u32::MAX; 16],
            maximum: 32767,
        },
    ] {
        assert!(advance(&mut s, &m, &mut v, Default::default(), draws).is_err());
        assert_eq!((s.clone(), v.clone()), before);
    }
    assert!(
        advance(
            &mut s,
            &m,
            &mut v,
            Parameters {
                height: f32::NAN,
                ..Default::default()
            },
            Draws {
                values: &samples,
                maximum: 32767
            }
        )
        .is_err()
    );
    assert_eq!((s.clone(), v.clone()), before);
    m.rolling = true;
    assert!(
        advance(
            &mut s,
            &m,
            &mut v,
            Default::default(),
            Draws {
                values: &samples,
                maximum: 32767
            }
        )
        .is_err()
    );
    m.rolling = false;
    let tail = [samples.as_slice(), &[u32::MAX; 3]].concat();
    assert_eq!(
        advance(
            &mut s,
            &m,
            &mut v,
            Default::default(),
            Draws {
                values: &tail,
                maximum: 32767
            }
        )
        .unwrap(),
        16
    );
    for kind in [Kind::Triangle, Kind::Square] {
        let grid = crate::sail_topology::Grid::select(0.0, false, true).unwrap();
        let topology = crate::sail_topology::fill(kind, false, grid, 4095).unwrap();
        let indices = topology.packed().unwrap();
        assert_eq!(indices.len(), if kind == Kind::Triangle { 48 } else { 72 });
        let written = if kind == Kind::Triangle {
            (0..5)
                .flat_map(|x| (0..=x).map(move |y| x * (x * 8 + 2) + y * 4))
                .collect::<Vec<_>>()
        } else {
            (0..4)
                .flat_map(|x| (0..5).map(move |y| x * 68 + y * 4))
                .collect::<Vec<_>>()
        };
        assert!(indices.iter().all(|v| written.contains(v)));
    }
}
