use super::*;
use serde_json::{Value as Json, json};
fn f(v: &Json) -> f32 {
    f32::from_bits(v.as_u64().unwrap() as u32)
}
#[test]
fn complete_native_camera_transform_perspective_grid_and_retained_centers_match() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/sail_lod/cases.json")).unwrap();
    assert_eq!(data["cases"].as_array().unwrap().len(), 96);
    for c in data["cases"].as_array().unwrap() {
        let mut state = Selection {
            world_center: [7.0, -8.0, 9.0],
            ..Default::default()
        };
        for frame in c["frames"].as_array().unwrap() {
            let flags = frame["flags"].as_u64().unwrap();
            let change = state
                .update(
                    std::array::from_fn(|i| f(&c["center"][i])),
                    std::array::from_fn(|i| f(&c["matrix"][i])),
                    Camera {
                        eye: std::array::from_fn(|i| f(&frame["eye"][i])),
                        perspective: f(&frame["fov"]),
                    },
                    flags & 1 != 0,
                    flags & 2 != 0,
                )
                .unwrap();
            assert_eq!(json!([state.grid.rows, state.grid.columns]), frame["grid"]);
            assert_eq!(u8::from(change.changed), frame["changed"]);
            assert_eq!(
                json!(state.world_center.map(f32::to_bits)),
                frame["world_center"]
            );
            assert_eq!(
                json!(change.distance.map(f32::to_bits)),
                frame["distance"],
                "profile {}",
                c["id"]
            );
        }
    }
}
#[test]
fn fixed_grids_ignore_unused_camera_inputs_and_failed_distance_retains_state() {
    let mut state = Selection {
        grid: Grid::NEAR,
        world_center: [1.0, 2.0, 3.0],
    };
    let bad = Camera {
        eye: [f32::NAN; 3],
        perspective: f32::NAN,
    };
    let before = state;
    assert!(
        state
            .update([0.0; 3], [0.0; 16], bad, false, false)
            .is_err()
    );
    assert_eq!(state, before);
    state
        .update([f32::NAN; 3], [f32::NAN; 16], bad, true, true)
        .unwrap();
    assert_eq!(state.grid, Grid::ROLLED);
    assert_eq!(state.world_center, before.world_center);
    state
        .update([f32::NAN; 3], [f32::NAN; 16], bad, false, true)
        .unwrap();
    assert_eq!(
        state.grid,
        Grid {
            rows: 5,
            columns: 4
        }
    );
    assert_eq!(state.world_center, before.world_center);
    let before = state;
    assert!(
        state
            .update(
                [f32::MAX; 3],
                [f32::MAX; 16],
                Camera {
                    eye: [0.0; 3],
                    perspective: VERTICAL_FOV
                },
                false,
                false
            )
            .is_err()
    );
    assert_eq!(state, before);
}
