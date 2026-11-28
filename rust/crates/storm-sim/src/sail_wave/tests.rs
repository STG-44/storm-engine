use super::*;
use crate::sail_motion::Rolling;
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
fn attached_wave_lod_rope_points_and_all_complete_native_frames_match() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/sail_wave/cases.json")).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 60);
    let table = [
        0.0, 0.25, 0.5, 0.75, 1.0, 0.5, 0.25, 0.0, -0.25, -0.75, -1.0, -0.5, -0.25,
    ];
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
        m.geometry = std::array::from_fn(|i| vector(&c["geometry"][i]));
        if !c["rolling_delta"].is_null() {
            m.transition = Some(Rolling {
                geometry: m.geometry,
                delta: f(&c["rolling_delta"]),
                up: false,
            });
        }
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
        let input = Input {
            parameters: Parameters {
                deep_z: f(&p[0]),
                deep_h: f(&p[1]),
                deep_vh: f(&p[2]),
                deep_vz: f(&p[3]),
                holes: c["holes"].as_i64().unwrap() as i32,
                hole_flex: 0.01,
            },
            sphere: Sphere {
                center: std::array::from_fn(|i| f(&c["sphere"][i])),
                radius: f(&c["sphere"][3]),
            },
            height: f(&c["height"]),
            local_wind: vector(&c["wind"]),
            wind_base: f(&c["base"]),
            limit_point: c["limit"] == 1,
            rows: c["lod"][0].as_u64().unwrap() as u32,
            columns: c["lod"][1].as_u64().unwrap() as u32,
            ties: std::array::from_fn(|i| c["ties"][i] == 1),
        };
        for (index, dt) in [0, 17, 1001].into_iter().enumerate() {
            advance(
                &mut s,
                &m,
                &mut vertices,
                input,
                &table,
                Settings::default(),
                dt,
            )
            .unwrap();
            let actual = snapshot(&s, &vertices);
            // Report the first field/index rather than dumping entire grids.
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
                    actual[key], c["frames"][index][key],
                    "case {} frame {index} {key}",
                    c["id"]
                );
            }
            for (n, v) in actual["vertices"].as_array().unwrap().iter().enumerate() {
                assert_eq!(
                    v, &c["frames"][index]["vertices"][n],
                    "case {} frame {index} vertex {n}",
                    c["id"]
                );
            }
        }
    }
}
#[test]
fn invalid_wave_inputs_and_overflow_leave_all_output_untouched() {
    let mut m = SailMotion::new(Kind::Square, false);
    m.hard_points = [
        [0.0, 3.0, 0.0],
        [4.0, 3.0, 0.0],
        [0.0, 0.0, 0.0],
        [4.0, 0.0, 0.0],
    ];
    let parameters =
        crate::sail_geometry::preview_parameters(m.kind, m.hard_points, false, 0, 0.01).unwrap();
    let sphere = crate::sail_geometry::rebuild(&mut m, parameters).unwrap();
    let mut vertices = crate::sail_geometry::fill(&m, sphere, [1.0; 2]).unwrap();
    let mut s = State {
        maximum_sum_wind: 0.1,
        ..Default::default()
    };
    let input = Input {
        parameters,
        sphere,
        height: 3.0,
        local_wind: [0.0, 0.0, 1.0],
        wind_base: 0.4,
        limit_point: false,
        rows: 17,
        columns: 13,
        ties: [false; 4],
    };
    let old = (s.clone(), vertices.clone());
    for bad in [
        Input { rows: 0, ..input },
        Input {
            columns: 1,
            ..input
        },
        Input {
            wind_base: f32::NAN,
            ..input
        },
        Input {
            parameters: Parameters {
                deep_z: f32::MAX,
                deep_h: f32::MAX,
                ..parameters
            },
            ..input
        },
    ] {
        assert!(
            advance(
                &mut s,
                &m,
                &mut vertices,
                bad,
                &[0.0, 1.0],
                Settings::default(),
                17
            )
            .is_err()
        );
        assert_eq!((s.clone(), vertices.clone()), old);
    }
    for table in [&[][..], &[f32::NAN][..]] {
        assert!(
            advance(
                &mut s,
                &m,
                &mut vertices,
                input,
                table,
                Settings::default(),
                17
            )
            .is_err()
        );
        assert_eq!((s.clone(), vertices.clone()), old);
    }
    assert!(
        advance(
            &mut s,
            &m,
            &mut vertices,
            input,
            &[0.0, 1.0],
            Settings {
                square_add: u32::MAX,
                ..Default::default()
            },
            u32::MAX
        )
        .is_err()
    );
    assert_eq!((s.clone(), vertices.clone()), old);
    advance(
        &mut s,
        &m,
        &mut vertices,
        input,
        &[0.0, 1.0],
        Settings::default(),
        17,
    )
    .unwrap();
    assert_ne!(vertices, old.1);
    for (a, b) in vertices.iter().zip(&old.1) {
        assert_eq!((a.normal, a.uv), (b.normal, b.uv));
    }
    m.rolling = true;
    assert!(
        advance(
            &mut s,
            &m,
            &mut vertices,
            input,
            &[0.0, 1.0],
            Settings::default(),
            17
        )
        .is_err()
    );
}
