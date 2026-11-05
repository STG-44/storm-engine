use super::*;
use serde_json::{Value as Json, json};
#[test]
fn complete_native_index_allocations_and_projected_grid_selections_match() {
    let data: Json =
        serde_json::from_str(include_str!("../../reference/sail_roll/cases.json")).unwrap();
    assert_eq!(data["topologies"].as_array().unwrap().len(), 525);
    for c in data["topologies"].as_array().unwrap() {
        let kind = [
            Kind::Triangle,
            Kind::Square,
            Kind::Trapezoid,
            Kind::Gerbed,
            Kind::Rolling,
            Kind::Special,
        ][c["kind"].as_u64().unwrap() as usize];
        let t = fill(
            kind,
            false,
            Grid {
                rows: c["grid"][0].as_u64().unwrap() as u32,
                columns: c["grid"][1].as_u64().unwrap() as u32,
            },
            c["mask"].as_u64().unwrap() as u16,
        )
        .unwrap();
        assert_eq!(
            json!({"normal":t.normal_count,"hole_start":t.hole_start,"holes":t.hole_count,"indices":t.indices}),
            c["topology"],
            "kind {kind:?} grid {} mask {}",
            c["grid"],
            c["mask"]
        );
    }
    assert_eq!(data["selection"].as_array().unwrap().len(), 108);
    let mut previous = Grid {
        rows: 0,
        columns: 0,
    };
    let mut previous_flags = 0;
    for c in data["selection"].as_array().unwrap() {
        let flags = c["flags"].as_u64().unwrap();
        if flags != previous_flags {
            previous = Grid {
                rows: 0,
                columns: 0,
            };
            previous_flags = flags;
        }
        let grid = Grid::select(
            f32::from_bits(c["distance"].as_u64().unwrap() as u32),
            flags & 1 != 0,
            flags & 2 != 0,
        )
        .unwrap();
        assert_eq!(json!([grid.rows, grid.columns]), c["grid"]);
        assert_eq!(u8::from(grid != previous), c["changed"]);
        previous = grid;
    }
}
#[test]
fn rolled_indices_only_reference_written_vertices_and_invalid_ranges_fail() {
    for kind in [Kind::Triangle, Kind::Square] {
        let t = fill(kind, true, Grid::ROLLED, 4095).unwrap();
        let indices = t.packed().unwrap();
        assert_eq!(indices.len(), if kind == Kind::Triangle { 93 } else { 288 });
        let written = if kind == Kind::Triangle {
            vec![
                6, 7, 8, 9, 10, 11, 12, 13, 14, 36, 38, 40, 42, 44, 78, 81, 84, 87, 90, 136, 140,
                144, 148, 152,
            ]
        } else {
            (0..13)
                .flat_map(|i| [0, 4, 8, 12, 16].map(|j| i * 17 + j))
                .collect()
        };
        assert!(indices.iter().all(|v| written.contains(v)));
        let mut bad = t.clone();
        bad.normal_count = u32::MAX;
        assert!(bad.packed().is_err());
        assert!(fill(kind, true, Grid::NEAR, 0).is_err());
    }
    assert!(Grid::select(f32::NAN, false, false).is_err());
    assert!(Grid::select(-1.0, false, false).is_err());
    assert!(
        fill(
            Kind::Square,
            false,
            Grid {
                rows: 3,
                columns: 13
            },
            0
        )
        .is_err()
    );
}
