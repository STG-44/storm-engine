use super::*;
use crate::sail_motion::Rolling;
use serde_json::{Value as Json, json};
#[derive(Clone, Default, Debug, PartialEq)]
struct Node {
    position: V,
    globals: u32,
    calls: Vec<Vec<u32>>,
}
impl NodePort for Node {
    fn position(&mut self) -> V {
        self.calls.push(vec![0]);
        self.position
    }
    fn set_position(&mut self, v: V) {
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
fn f(v: &Json) -> f32 {
    f32::from_bits(v.as_u64().unwrap() as u32)
}
fn vector(v: &Json) -> V {
    std::array::from_fn(|i| f(&v[i]))
}
fn ropes(m: &Json) -> [Rope; 3] {
    std::array::from_fn(|i| Rope {
        number: if i == 1 { 2 } else { 1 },
        model: if i == 2 { 88 } else { 77 },
        begin: End {
            point: [i as f32 * 0.25, -0.5, 0.125],
            matrix: std::array::from_fn(|j| f(&m[i * 2][j])),
        },
        end: End {
            point: [3.0 + i as f32, 0.25, 1.0 - i as f32 * 0.125],
            matrix: std::array::from_fn(|j| f(&m[i * 2 + 1][j])),
        },
    })
}
fn snapshot(
    m: &SailMotion,
    links: &Links,
    n: &mut Node,
    sphere: Sphere,
    rebuilds: u32,
    queries: &mut Vec<i32>,
) -> Json {
    let out = json!({"finite":1,"geometry":m.geometry.map(|p|p.map(f32::to_bits)),"hard_points":m.hard_points.map(|p|p.map(f32::to_bits)),"rolling":u8::from(m.rolling),"angle":m.old_wind_angle.to_bits(),"limits":[m.minimum_angle.to_bits(),m.maximum_angle.to_bits()],"transition":m.transition.map(|t|json!({"geometry":t.geometry.map(|p|p.map(f32::to_bits)),"delta":t.delta.to_bits(),"up":u8::from(t.up)})),"sphere":[json!(sphere.center.map(f32::to_bits)),json!(sphere.radius.to_bits())],"position":n.position.map(f32::to_bits),"globals":n.globals,"rebuilds":rebuilds,"ties":m.ties.map(|p|p.map(|v|v.map(f32::to_bits))),"records":links.records.map(|r|r.map(|r|json!({"number":r.number,"tie":r.tie,"r1":r.r1.to_bits(),"r2":r.r2.to_bits(),"b":if m.kind==Kind::Triangle {Json::Null}else{json!(r.b.map(f32::to_bits))}}))),"queries":queries,"calls":n.calls});
    queries.clear();
    n.calls.clear();
    out
}
#[test]
fn rope_registration_endpoint_limits_and_complete_native_turn_snapshots_match() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/sail_rope/cases.json")).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 78);
    let (mut finite, mut rejected) = (0, 0);
    for c in data["cases"].as_array().unwrap() {
        let id = c["id"].as_u64().unwrap() as usize;
        let kind = [
            Kind::Triangle,
            Kind::Square,
            Kind::Trapezoid,
            Kind::Gerbed,
            Kind::Rolling,
            Kind::Special,
        ][id % 6];
        let frames = c["snapshots"].as_array().unwrap();
        let init = &frames[0];
        let mut m = SailMotion::new(kind, id < 72 && id.is_multiple_of(4));
        m.hard_points = std::array::from_fn(|i| vector(&init["hard_points"][i]));
        m.old_wind_angle = (id as i32 % 3 - 1) as f32 * 0.125;
        m.minimum_angle = 0.6 + (id % 4) as f32 * 0.125;
        m.maximum_angle = 0.6 - (id % 3) as f32 * 0.2;
        m.geometry = std::array::from_fn(|i| [i as f32 * 0.125, -(i as f32) * 0.25, 0.5]);
        if id < 72 && !(id / 6).is_multiple_of(2) {
            m.transition = Some(Rolling {
                geometry: m.geometry,
                delta: 0.75,
                up: !id.is_multiple_of(2),
            });
        }
        let p = Parameters {
            deep_z: 0.125,
            deep_h: 0.03,
            deep_vh: 0.01,
            deep_vz: 0.02,
            holes: 0,
            hole_flex: 0.01,
        };
        let mut sphere = sail_geometry::rebuild(&mut m, p).unwrap();
        let mut rebuilds = 1;
        let mut node = Node {
            position: [3.0 + id as f32, -2.0, 0.25],
            ..Default::default()
        };
        let mut links = Links::default();
        let mut queries = Vec::new();
        let mut registry = ropes(&c["matrices"]);
        for k in 0..7 {
            let number = [1, -1, 2, -2, 1, 999, 1][k];
            let model = if k == 4 {
                88
            } else if k == 6 {
                999
            } else {
                77
            };
            let mut point = [1.0, -2.0, 3.0];
            endpoint(&registry, number, model, Some(&mut point)).unwrap();
            assert_eq!(
                point.map(f32::to_bits),
                std::array::from_fn::<_, 3, _>(
                    |i| c["endpoint_checks"][k][i].as_u64().unwrap() as u32
                ),
                "endpoint {id}/{k}"
            );
        }
        assert_eq!(
            snapshot(&m, &links, &mut node, sphere, rebuilds, &mut queries),
            *init,
            "initial {id}"
        );
        finite += 1;
        let original = m.hard_points;
        let positions = [
            original[0],
            original[2],
            original[if kind == Kind::Triangle { 1 } else { 3 }],
            original[2],
        ];
        let turning = c["turning"] == 1;
        let service = c["service"] == 1;
        for k in 0..4 {
            let number = if k == 2 { -2 } else { 1 };
            let opposite =
                if turning && k != 0 && service && links.records.iter().any(Option::is_none) {
                    let mut out = [0.0; 3];
                    queries.push(number);
                    endpoint(&registry, number, 77, Some(&mut out)).unwrap();
                    Some(out)
                } else {
                    None
                };
            register(&mut m, &mut links, turning, positions[k], number, opposite).unwrap();
            assert_eq!(
                snapshot(&m, &links, &mut node, sphere, rebuilds, &mut queries),
                frames[k + 1],
                "registration {id}/{k}"
            );
            finite += 1;
        }
        let input = if kind == Kind::Triangle && turning && service && links.records[0].is_some() {
            let n = links.records[0].unwrap().number;
            let mut out = [0.0; 3];
            queries.push(n);
            endpoint(&registry, n, 77, Some(&mut out)).unwrap();
            Some(Limits {
                endpoint: out,
                sphere_radius: sphere.radius,
                parent_width: f(&c["limit_parameters"][0]),
                deep_h: p.deep_h,
                deep_z: p.deep_z,
                triangle_dependence: f(&c["limit_parameters"][1]),
                max_angle: f(&c["limit_parameters"][2]),
            })
        } else {
            None
        };
        set_limits(&mut m, &links, turning, input).unwrap();
        assert_eq!(
            snapshot(&m, &links, &mut node, sphere, rebuilds, &mut queries),
            frames[5],
            "limits {id}"
        );
        finite += 1;
        if id >= 72 {
            m.rolling = false;
            m.ties[2] = Some(original[2]);
            links.records = [
                Some(Record {
                    number: 1,
                    tie: 2,
                    r1: 0.0,
                    r2: 0.0,
                    b: [0.0; 3],
                }),
                None,
            ];
        }
        for (k, step) in c["steps"].as_array().unwrap().iter().enumerate() {
            registry = ropes(&step["matrices"]);
            let before = (m.clone(), node.clone());
            let result = turn(
                &mut m,
                &links,
                Turning {
                    wind_x: f(&step["wind"]),
                    step: f(&step["step"]),
                    discrete: 0.01,
                    geometry: p,
                },
                &mut node,
                if service || id >= 72 {
                    Some(|_: &Node, _: &SailMotion, n, out: &mut V| {
                        queries.push(n);
                        endpoint(&registry, n, 77, Some(out))?;
                        Ok(())
                    })
                } else {
                    None
                },
            );
            if step["state"]["finite"] == 0 {
                assert!(
                    result.unwrap_err().is::<NativeNonFinite>(),
                    "nonfinite {id}/{k}"
                );
                assert_eq!((m.clone(), node.clone()), before);
                rejected += 1;
                queries.clear();
                continue;
            }
            let out = result.unwrap();
            if let Some(s) = out.rebuilt {
                sphere = s;
                rebuilds += 1;
            }
            assert_eq!(
                snapshot(&m, &links, &mut node, sphere, rebuilds, &mut queries),
                step["state"],
                "turn {id}/{k}"
            );
            finite += 1;
        }
    }
    assert_eq!((finite, rejected), (900, 6));
}
#[test]
fn rope_guards_first_match_retention_and_checked_failure_are_atomic() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/sail_rope/cases.json")).unwrap();
    let mut r = ropes(&data["cases"][0]["matrices"]);
    r[2] = r[0];
    r[2].end.point = [100.0; 3];
    let mut point = [7.0; 3];
    endpoint(&r, 1, 77, Some(&mut point)).unwrap();
    assert_ne!(point, [100.0; 3]);
    let before = point;
    assert!(!endpoint(&r, 1, 999, Some(&mut point)).unwrap());
    assert_eq!(before, point);
    assert!(!endpoint(&r, i32::MIN, 77, None).unwrap());
    assert!(endpoint(&r, i32::MIN, 77, Some(&mut point)).is_err());
    assert_eq!(point, before);
    let mut m = SailMotion::new(Kind::Triangle, false);
    let before = m.clone();
    let mut links = Links::default();
    set_limits(&mut m, &links, true, None).unwrap();
    assert_eq!(m, before);
    register(&mut m, &mut links, true, [100.0; 3], 1, Some([0.0; 3])).unwrap();
    assert!(links.records.iter().all(Option::is_none));
    assert_eq!(m.ties[0], Some([100.0; 3]));
    let snapshot = (m.clone(), links.clone());
    assert!(register(&mut m, &mut links, true, [f32::NAN; 3], 1, None).is_err());
    assert_eq!((m, links), snapshot);
}
