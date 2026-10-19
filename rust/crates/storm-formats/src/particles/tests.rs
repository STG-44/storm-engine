use super::*;
const PROFILE: &[u8] = include_bytes!("../../reference/particles/synthetic.xps");
fn reference() -> Json {
    serde_json::from_str(include_str!("../../reference/particles/cases.json")).unwrap()
}
#[test]
fn all_seven_field_loaders_and_complete_state_match_retained_cpp() {
    let cases = reference();
    for case in cases["fields"].as_array().unwrap() {
        let data = case["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect::<Vec<_>>();
        let mut bytes = 1u32.to_le_bytes().to_vec();
        bytes.extend((case["kind"].as_u64().unwrap() as u32).to_le_bytes());
        bytes.extend(&data);
        let mut reader = Reader {
            bytes: &bytes,
            offset: 0,
            budget: 262_144,
        };
        let fields = reader.fields().unwrap();
        assert_eq!(reader.offset, bytes.len());
        let actual = Profile {
            emitters: vec![Emitter {
                fields,
                particles: vec![],
            }],
        }
        .report()["emitters"][0]["fields"][0]
            .clone();
        let mut expected = case["expected"].clone();
        expected.as_object_mut().unwrap().remove("samples");
        expected.as_object_mut().unwrap().remove("range_samples");
        assert_eq!(
            expected
                .as_object_mut()
                .unwrap()
                .remove("consumed")
                .unwrap(),
            data.len()
        );
        assert_eq!(actual, expected);
    }
}
#[test]
fn original_graph_samples_include_relative_time_short_segments_backtracking_and_rgb_only() {
    let fixture = reference();
    let mut count = 0;
    for case in fixture["fields"].as_array().unwrap() {
        let Some(samples) = case["expected"].get("samples") else {
            continue;
        };
        let mut bytes = 1u32.to_le_bytes().to_vec();
        bytes.extend((case["kind"].as_u64().unwrap() as u32).to_le_bytes());
        bytes.extend(
            case["bytes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u8),
        );
        let fields = Reader {
            bytes: &bytes,
            offset: 0,
            budget: 262_144,
        }
        .fields()
        .unwrap();
        for sample in samples.as_array().unwrap() {
            let f = |i: usize| f32::from_bits(sample[i].as_u64().unwrap() as u32);
            let actual = match &fields.0[0].value {
                Value::Graph(g) => json!(g.sample(f(0), f(1), f(2)).to_bits()),
                Value::Color(g) => json!(g.sample(f(0), f(1), f(2)).map(f32::to_bits)),
                _ => panic!("not a graph"),
            };
            assert_eq!(actual, sample[3], "{sample}");
            count += 1;
        }
    }
    assert_eq!(count, 282);
}
#[test]
fn container_preserves_emitter_particle_kinds_and_all_field_data() {
    let profile = Profile::read(PROFILE).unwrap();
    assert_eq!(profile.emitters.len(), 1);
    assert_eq!(
        profile.emitters[0]
            .particles
            .iter()
            .map(|p| p.kind)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(profile.emitters[0].fields.float("LIFE TIME"), Some(2.0));
    assert_eq!(
        profile.emitters[0].fields.text("name").unwrap().display(),
        "emitter"
    );
    let uv = profile.emitters[0].particles[0]
        .fields
        .get("texture")
        .unwrap();
    assert_eq!(
        uv,
        &Value::Uv(vec![[0.25, 0.5, 0.5, 0.625], [0.0, 0.0, 1.0, 1.0]])
    );
    assert_eq!(Text(vec![0xef, 0xe0, 0xf0, 0xf3, 0xf1]).display(), "парус");
    let fields = Fields(vec![
        Field {
            name: Text(b"count".to_vec()),
            value: Value::Bool(true),
        },
        Field {
            name: Text(b"Count".to_vec()),
            value: Value::Float(7.0),
        },
    ]);
    assert_eq!(fields.float("COUNT"), Some(7.0));
}
#[test]
fn every_truncated_prefix_is_rejected_without_panics() {
    for end in 0..PROFILE.len() {
        assert!(Profile::read(&PROFILE[..end]).is_err(), "{end}");
    }
    assert!(Profile::read(PROFILE).is_ok());
}
#[test]
fn rejects_unknown_version_types_counts_and_trailing_bytes() {
    for (offset, value) in [
        (0, 0),
        (4, u32::from_le_bytes(*b"v3.4")),
        (8, u32::MAX),
        (12, 1),
        (16, u32::MAX),
        (20, 0),
    ] {
        let mut data = PROFILE.to_vec();
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(Profile::read(&data).is_err(), "{offset}");
    }
    let mut data = PROFILE.to_vec();
    data.push(0);
    assert!(Profile::read(&data).is_err());
    assert!(Profile::read(&vec![0; 16 * 1024 * 1024 + 1]).is_err());
}
#[test]
fn rejects_unterminated_names_nonfinite_values_and_overflowing_uv_extents() {
    let fields = [
        (2u32, vec![f32::NAN.to_bits(), 2, 0]),
        (1, vec![1, 128]),
        (6, vec![1, f32::MAX.to_bits(), 0, f32::MAX.to_bits(), 0]),
    ];
    for (kind, words) in fields {
        let mut bytes = 1u32.to_le_bytes().to_vec();
        bytes.extend(kind.to_le_bytes());
        for n in words {
            bytes.extend(n.to_le_bytes());
        }
        assert!(
            Reader {
                bytes: &bytes,
                offset: 0,
                budget: 100
            }
            .fields()
            .is_err()
        );
    }
    let bytes = [
        1u32.to_le_bytes().as_slice(),
        1u32.to_le_bytes().as_slice(),
        1u32.to_le_bytes().as_slice(),
        1u32.to_le_bytes().as_slice(),
        b"x",
    ]
    .concat();
    assert!(
        Reader {
            bytes: &bytes,
            offset: 0,
            budget: 100
        }
        .fields()
        .is_err()
    );
}
#[test]
fn sparse_graphs_and_undefined_native_division_paths_are_safe() {
    let empty = Graph {
        negative: false,
        relative: true,
        min: vec![],
        max: vec![],
    };
    assert_eq!(empty.sample(0.0, 0.0, 0.5), 0.0);
    assert_eq!(ColorGraph(vec![]).sample(0.0, 0.0, 0.5), [1.0; 4]);
}
#[test]
fn native_random_ranges_swap_crossing_bounds_and_use_the_thousand_step_fraction() {
    let fixture = reference();
    let mut count = 0;
    for case in fixture["fields"].as_array().unwrap() {
        let Some(samples) = case["expected"].get("range_samples") else {
            continue;
        };
        let mut bytes = 1u32.to_le_bytes().to_vec();
        bytes.extend(3u32.to_le_bytes());
        bytes.extend(
            case["bytes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u8),
        );
        let fields = Reader {
            bytes: &bytes,
            offset: 0,
            budget: 262_144,
        }
        .fields()
        .unwrap();
        let Value::Graph(graph) = &fields.0[0].value else {
            panic!("graph expected")
        };
        for sample in samples.as_array().unwrap() {
            let f = |n: usize| f32::from_bits(sample[n].as_u64().unwrap() as u32);
            assert_eq!(
                graph
                    .sample_range(f(0), f(1), sample[2].as_u64().unwrap() as u32)
                    .to_bits(),
                sample[3].as_u64().unwrap() as u32
            );
            count += 1;
        }
    }
    assert_eq!(count, 240);
}
