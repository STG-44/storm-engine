//! World positions for the native ship-fire emitter; its Euler angles stay (0,1,0).
use super::ModelPose;
use anyhow::{Result, ensure};
use glam::Vec3;
use std::sync::Arc;
use storm_sim::{
    particle_motion::Transform,
    pose_history::{PoseHistory, PoseSample},
};

impl From<PoseSample> for ModelPose {
    fn from(s: PoseSample) -> Self {
        Self {
            translation: s.translation,
            yaw: s.yaw,
            pitch: s.pitch,
            roll: s.roll,
        }
    }
}
impl ModelPose {
    pub fn sample(self, seconds: f64) -> PoseSample {
        PoseSample {
            seconds,
            translation: self.translation,
            yaw: self.yaw,
            pitch: self.pitch,
            roll: self.roll,
        }
    }
}
#[derive(Clone, Debug)]
pub struct WorldEmitter {
    pub history: Arc<PoseHistory>,
    /// Locator in renderer ship-local coordinates, as loaded from the model.
    pub point: [f32; 3],
}
impl WorldEmitter {
    pub(super) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.history, &other.history)
            && self.point.map(f32::to_bits) == other.point.map(f32::to_bits)
    }
    pub fn at(&self, seconds: f64) -> Result<Transform> {
        ensure!(
            seconds.is_finite() && self.point.into_iter().all(f32::is_finite),
            "invalid world emitter sample"
        );
        let frames = self.history.read()?;
        ensure!(!frames.is_empty(), "empty particle pose history");
        let end = frames.partition_point(|s| s.seconds < seconds);
        let (a, b) = if end == 0 {
            (frames[0], frames[0])
        } else if end == frames.len() {
            (frames[end - 1], frames[end - 1])
        } else {
            (frames[end - 1], frames[end])
        };
        let point = Vec3::from_array(self.point);
        let first = ModelPose::from(a).matrix()?.transform_point3(point);
        let second = ModelPose::from(b).matrix()?.transform_point3(point);
        let k = if a.seconds == b.seconds {
            0.0
        } else {
            ((seconds - a.seconds) / (b.seconds - a.seconds)).clamp(0.0, 1.0) as f32
        };
        let p = first + (second - first) * k;
        // FirePlace passes these as Euler RADIANS to PARTICLE_SYSTEM::SetEmitter.
        Transform::from_euler([0.0, 1.0, 0.0], [p.x, p.y, -p.z])
    }
}
