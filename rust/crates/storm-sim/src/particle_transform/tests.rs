use super::*;
use serde_json::{Value, json};
fn transform(value: &Value) -> Transform {
    Transform(std::array::from_fn(|i| {
        std::array::from_fn(|j| f32::from_bits(value[i][j].as_u64().unwrap() as u32))
    }))
}
#[test]
fn retained_matrix_blend_and_birth_coordinates_match_cpp() {
    let data: Value = serde_json::from_str(include_str!(
        "../../reference/particle_transform/cases.json"
    ))
    .unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 49);
    for case in data["cases"].as_array().unwrap() {
        let old = transform(&case["old"]);
        let new = transform(&case["new"]);
        let mut e = EmitterTransform::default();
        e.set(old);
        e.set(new);
        assert_eq!(e.old, Some(old));
        for (step, expected) in case["births"].as_array().unwrap().iter().enumerate() {
            let m = e.birth(step as f32 / 32.0, [0.5, 1.25, -0.75]).unwrap();
            let actual = json!({"matrix":m.0.map(|r|r.map(f32::to_bits)),"point":m.point([-0.25,0.75,2.0]).map(f32::to_bits),"normal":m.normal([1.25,-0.5,2.0]).map(f32::to_bits)});
            // Transcendental SLerp implementations may differ in their last bits.
            assert_eq!(actual, *expected, "step={step}, old={old:?}, new={new:?}");
        }
        e.finish();
        assert_eq!(e.old, Some(new));
        e.teleport(old);
        let mut fresh = EmitterTransform::default();
        fresh.teleport(old);
        fresh.set(new);
        assert_eq!(
            fresh.old,
            Some(transform(&case["first_set_after_teleport"]))
        );
        assert_eq!(e.old, Some(transform(&case["teleport_old"])));
        assert_eq!(e.new, transform(&case["teleport_new"]));
    }
}
#[test]
fn invalid_transform_inputs_fail_and_native_fire_angles_are_radians() {
    assert!(Transform::from_euler([f32::NAN, 0.0, 0.0], [0.0; 3]).is_err());
    let a = Transform::from_euler([0.0, 1.0, 0.0], [2.0, 0.0, 0.0]).unwrap();
    assert!((a.normal([1.0, 0.0, 0.0])[2] + 1.0f32.sin()).abs() < 1e-6);
    assert!(a.blend(a, -0.1).is_err());
    assert!(a.blend(a, f32::INFINITY).is_err());
}
