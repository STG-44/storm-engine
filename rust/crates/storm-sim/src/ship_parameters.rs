//! Retained LoadShipParameters and the numeric portion of SHIP mounting.
use crate::{ship_motion::MotionState, ship_placement::ShipPlacement};
use anyhow::{Result, ensure};
use storm_core::{AttributeId, AttributeStore};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShipParameters {
    pub class: i32,
    pub speed_rate: f32,
    pub turn_rate: f32,
    pub crew_min: i32,
    pub crew_max: i32,
    pub capacity: i32,
    pub weight: f32,
    pub speed_depend_weight: f32,
    pub sub_sea_depend_weight: f32,
    pub turn_depend_weight: f32,
    pub turn_depend_speed: f32,
    pub waterline: f32,
    pub inertia_accel: [f32; 3],
    pub inertia_brake: [f32; 3],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MountedParameters {
    pub gravity: f32,
    pub length: f32,
    pub width: f32,
    pub load: f32,
    pub nominal_speed_z: f32,
    pub nominal_speed_y: f32,
    pub inertia_factor: f32,
    pub mass_inertia: f32,
    pub weight: f32,
}
impl ShipParameters {
    pub fn load(store: &AttributeStore, ship_type: AttributeId) -> Result<Self> {
        ensure!(
            store.get(ship_type).is_some(),
            "stale or foreign ship parameter binding"
        );
        let f = |name: &str| store.as_float(ship_type, Some(name), 0.0);
        let d = |name: &str| store.as_dword(ship_type, Some(name), 0) as i32;
        let mut p = Self {
            class: d("Class"),
            speed_rate: f("SpeedRate"),
            turn_rate: f("TurnRate"),
            crew_min: d("MinCrew"),
            crew_max: d("MaxCrew"),
            capacity: d("Capacity"),
            weight: f("Weight"),
            speed_depend_weight: f("SpeedDependWeight"),
            sub_sea_depend_weight: f("SubSeaDependWeight"),
            turn_depend_weight: f("TurnDependWeight"),
            turn_depend_speed: f("TurnDependSpeed"),
            waterline: f("WaterLine"),
            ..Default::default()
        };
        // Keep the original Y/X/Z acceleration/braking lookup order.
        for (axis, suffix) in [(1, "Y"), (0, "X"), (2, "Z")] {
            p.inertia_accel[axis] = f(&format!("InertiaAcceleration{suffix}"));
            p.inertia_brake[axis] = f(&format!("InertiaBraking{suffix}"));
        }
        p.validate()?;
        Ok(p)
    }
    fn validate(self) -> Result<()> {
        ensure!(
            [
                self.speed_rate,
                self.turn_rate,
                self.weight,
                self.speed_depend_weight,
                self.sub_sea_depend_weight,
                self.turn_depend_weight,
                self.turn_depend_speed,
                self.waterline
            ]
            .into_iter()
            .chain(self.inertia_accel)
            .chain(self.inertia_brake)
            .all(f32::is_finite),
            "non-finite ship parameter"
        );
        Ok(())
    }
    /// LoadShipParameters itself only replaces represented inertia components.
    pub fn apply_inertia(self, state: &mut MotionState) -> Result<()> {
        self.validate()?;
        state.inertia_accel = self.inertia_accel;
        state.inertia_brake = self.inertia_brake;
        Ok(())
    }
    /// Numeric Mount assignments; no geometry/services/force scripts are mounted.
    pub fn mounted(self, box_size: [f32; 3]) -> Result<MountedParameters> {
        self.validate()?;
        ensure!(
            box_size.into_iter().all(f32::is_finite),
            "non-finite mounted bounds"
        );
        Ok(MountedParameters {
            gravity: 9.81,
            length: box_size[2],
            width: box_size[0],
            load: 0.0,
            nominal_speed_z: self.speed_rate,
            nominal_speed_y: self.turn_rate / 444.0,
            inertia_factor: self.capacity as f32 / 10000.0,
            mass_inertia: 0.10,
            weight: self.weight,
        })
    }
    /// Apply only represented numeric mounting state, preserving motion/pose.
    pub fn configure(
        self,
        state: &mut MotionState,
        placement: &mut ShipPlacement,
        box_size: [f32; 3],
    ) -> Result<MountedParameters> {
        let mounted = self.mounted(box_size)?;
        state.inertia_accel = self.inertia_accel;
        state.inertia_brake = self.inertia_brake;
        state.mass_inertia = mounted.mass_inertia;
        placement.waterline = self.waterline;
        Ok(mounted)
    }
}

#[cfg(test)]
mod tests;
