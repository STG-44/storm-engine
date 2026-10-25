//! Retained SHIP immersion lookup and model-root placement expression.
use anyhow::{Result, ensure};
use storm_core::{AttributeId, AttributeStore};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShipPlacement {
    pub immersion: f32,
    pub waterline: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RootPose {
    pub position: [f32; 3],
    pub angles: [f32; 3],
}
fn attribute(store: &AttributeStore, root: AttributeId, path: &str) -> Result<f32> {
    ensure!(
        store.get(root).is_some(),
        "stale or foreign ship placement binding"
    );
    let value = store
        .lookup_path(root, path)?
        .map_or(0.0, |id| store.as_float(id, None, 0.0));
    ensure!(value.is_finite(), "non-finite ship placement attribute");
    Ok(value)
}
impl ShipPlacement {
    /// Native LoadShipParameters reads WaterLine from the ship-type record.
    pub fn load_waterline(&mut self, store: &AttributeStore, ship_type: AttributeId) -> Result<()> {
        self.waterline = attribute(store, ship_type, "WaterLine")?;
        Ok(())
    }
    /// Native CalculateImmersion refreshes Ship.Immersion from the character.
    pub fn calculate_immersion(
        &mut self,
        store: &AttributeStore,
        character: AttributeId,
    ) -> Result<()> {
        self.immersion = attribute(store, character, "Ship.Immersion")?;
        Ok(())
    }
    /// Inputs are the native presentation vPos/vAng, independently supplied by
    /// the caller. This does not port Execute's presentation lag or sinking.
    pub fn root(
        self,
        position: [f32; 3],
        angles: [f32; 3],
        offsets: [f32; 2],
        dead_tilt: [f32; 2],
    ) -> Result<RootPose> {
        ensure!(
            position
                .into_iter()
                .chain(angles)
                .chain(offsets)
                .chain(dead_tilt)
                .chain([self.immersion, self.waterline])
                .all(f32::is_finite),
            "non-finite ship root input"
        );
        let result = RootPose {
            position: [
                position[0] + offsets[0],
                position[1] - self.immersion - self.waterline,
                position[2] + offsets[1],
            ],
            angles: [
                angles[0] + dead_tilt[0],
                angles[1],
                angles[2] + dead_tilt[1],
            ],
        };
        ensure!(
            result
                .position
                .into_iter()
                .chain(result.angles)
                .all(f32::is_finite),
            "ship root overflow"
        );
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value as Json, json};
    #[test]
    fn immersion_and_complete_root_components_match_native_snapshots() {
        let reference: Json =
            serde_json::from_str(include_str!("../reference/ship_placement/cases.json")).unwrap();
        assert_eq!(reference["cases"].as_array().unwrap().len(), 64);
        for case in reference["cases"].as_array().unwrap() {
            let f = |k: &str| case[k].as_f64().unwrap() as f32;
            let mut store = AttributeStore::default();
            let character = store.create_root().unwrap();
            let ship_type = store.create_root().unwrap();
            let waterline = store.create_path(ship_type, "WaterLine").unwrap();
            store.set_float(waterline, None, f("waterline")).unwrap();
            let mut placement = ShipPlacement::default();
            placement.load_waterline(&store, ship_type).unwrap();
            for (step, row) in case["snapshots"].as_array().unwrap().iter().enumerate() {
                if step == 1 || (step == 0 && case["present"].as_bool().unwrap()) {
                    let id = store.create_path(character, "Ship.Immersion").unwrap();
                    store
                        .set_float(
                            id,
                            None,
                            if step == 1 {
                                -f("immersion")
                            } else {
                                f("immersion")
                            },
                        )
                        .unwrap();
                }
                if step == 2 {
                    let id = store
                        .lookup_path(character, "Ship.Immersion")
                        .unwrap()
                        .unwrap();
                    store.delete_class(character, id).unwrap();
                }
                placement.calculate_immersion(&store, character).unwrap();
                let root = placement
                    .root(
                        [f("x"), f("y"), f("z")],
                        [0.125, f("yaw"), -0.25],
                        [f("offsetx"), f("offsetz")],
                        [f("tiltx"), f("tiltz")],
                    )
                    .unwrap();
                assert_eq!(
                    json!({"immersion":placement.immersion.to_bits(),"position":root.position.map(f32::to_bits),"angles":root.angles.map(f32::to_bits)}),
                    *row,
                    "case {} step {step}",
                    case["id"]
                );
            }
        }
    }
    #[test]
    fn live_character_defaults_and_mounted_waterline_have_separate_lifetimes() {
        let mut store = AttributeStore::default();
        let character = store.create_root().unwrap();
        let ship = store.create_root().unwrap();
        let mut p = ShipPlacement::default();
        p.load_waterline(&store, ship).unwrap();
        p.calculate_immersion(&store, character).unwrap();
        assert_eq!(p, ShipPlacement::default());
        let line = store.create_path(ship, "waterline").unwrap();
        store.set_value(line, Some("0.75")).unwrap();
        p.load_waterline(&store, ship).unwrap();
        store.set_value(line, Some("3.5")).unwrap();
        let immersion = store.create_path(character, "sHiP.iMmErSiOn").unwrap();
        store.set_value(immersion, Some("-1.25")).unwrap();
        p.calculate_immersion(&store, character).unwrap();
        assert_eq!(
            p,
            ShipPlacement {
                immersion: -1.25,
                waterline: 0.75
            }
        );
        store.set_value(immersion, Some("2.5")).unwrap();
        p.calculate_immersion(&store, character).unwrap();
        assert_eq!(p.immersion, 2.5);
        store.delete_class(character, immersion).unwrap();
        p.calculate_immersion(&store, character).unwrap();
        assert_eq!(p.immersion, 0.0);
        assert!(
            p.calculate_immersion(&AttributeStore::default(), character)
                .is_err()
        );
        store.drop_root(ship).unwrap();
        assert!(p.load_waterline(&store, ship).is_err());
    }
    #[test]
    fn invalid_attributes_and_root_overflow_leave_existing_parameters_intact() {
        let mut store = AttributeStore::default();
        let root = store.create_root().unwrap();
        let i = store.create_path(root, "Ship.Immersion").unwrap();
        let w = store.create_path(root, "WaterLine").unwrap();
        let mut p = ShipPlacement {
            immersion: 0.5,
            waterline: 1.0,
        };
        let saved = p;
        for bad in ["nan", "inf", "-inf"] {
            store.set_value(i, Some(bad)).unwrap();
            store.set_value(w, Some(bad)).unwrap();
            assert!(p.calculate_immersion(&store, root).is_err());
            assert!(p.load_waterline(&store, root).is_err());
            assert_eq!(p, saved);
        }
        assert!(
            p.root([f32::MAX, 0.0, 0.0], [0.0; 3], [f32::MAX, 0.0], [0.0; 2])
                .is_err()
        );
        assert!(
            p.root([0.0; 3], [f32::NAN, 0.0, 0.0], [0.0; 2], [0.0; 2])
                .is_err()
        );
    }
}
