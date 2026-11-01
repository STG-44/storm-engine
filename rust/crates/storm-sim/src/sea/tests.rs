use super::*;
use crate::ship_rocking::SeaSurface;
use serde_json::Value;
fn f(v: &Value) -> f32 {
    f32::from_bits(v.as_u64().unwrap() as u32)
}
fn hash(bytes: impl Iterator<Item = u8>) -> String {
    bytes
        .fold(14695981039346656037u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(1099511628211)
        })
        .to_string()
}
#[test]
fn native_normal_packing_prepared_maps_animation_and_queries_match_cpp() {
    let data: Value = serde_json::from_str(include_str!("../../reference/sea/cases.json")).unwrap();
    let frames = Arc::new(Frames::fixture());
    for (i, normals) in frames.normals.iter().enumerate() {
        assert_eq!(
            hash(normals.iter().flat_map(|v| v.to_le_bytes())),
            data["normal_hashes"][i].as_str().unwrap()
        );
    }
    for case in data["cases"].as_array().unwrap() {
        let mut sea = Sea::new(
            frames.clone(),
            Parameters {
                amplitude: [f(&case["amplitude"][0]), f(&case["amplitude"][1])],
                scale: [f(&case["scale"][0]), f(&case["scale"][1])],
                animation_speed: [
                    f(&case["animation_speed"][0]),
                    f(&case["animation_speed"][1]),
                ],
                ..Default::default()
            },
        )
        .unwrap();
        sea.set_camera([7.0, -9.0]).unwrap();
        for step in case["frames"].as_array().unwrap() {
            sea.advance(step["delta_ms"].as_u64().unwrap() as u32)
                .unwrap();
            let s = sea.surface();
            let a = s.animation;
            let actual = a
                .frame
                .into_iter()
                .chain(a.movement.into_iter().flatten())
                .map(f32::to_bits)
                .collect::<Vec<_>>();
            assert_eq!(serde_json::json!(actual), step["animation"]);
            assert_eq!(
                hash(s.maps().iter().flat_map(|v| v.to_bits().to_le_bytes())),
                step["maps_hash"].as_str().unwrap()
            );
            let mut provider = s.clone();
            for q in step["queries"].as_array().unwrap() {
                let sample = s.sample(f(&q[0]), f(&q[1])).unwrap();
                assert_eq!(sample.height.to_bits(), q[2].as_u64().unwrap() as u32);
                assert_eq!(
                    provider.wave_xz(f(&q[0]), f(&q[1])).unwrap().to_bits(),
                    q[3].as_u64().unwrap() as u32
                );
                assert_eq!(serde_json::json!(sample.normal.map(f32::to_bits)), q[4]);
            }
        }
    }
}
#[test]
fn invalid_updates_are_atomic_and_snapshots_are_immutable() {
    assert!(Frames::new(vec![vec![0; CELLS]; 63]).is_err());
    let mut sea = Sea::new(Arc::new(Frames::fixture()), Parameters::default()).unwrap();
    let saved = sea.surface().clone();
    assert!(sea.advance(1001).is_err());
    assert!(sea.set_camera([f32::NAN, 0.0]).is_err());
    assert_eq!(saved.maps(), sea.surface().maps());
    assert_eq!(saved.animation, sea.surface().animation);
    assert!(saved.sample(f32::NAN, 0.0).is_err());
    let old = saved.sample(-128.5, 127.5).unwrap();
    sea.advance(1000).unwrap();
    assert_eq!(saved.sample(-128.5, 127.5).unwrap(), old);
    assert_ne!(saved.maps(), sea.surface().maps());
}
