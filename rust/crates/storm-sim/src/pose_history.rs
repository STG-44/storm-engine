//! Bounded history for the approximate sailing preview, not native ship physics.
use crate::{Result, SimError};
use std::sync::{RwLock, RwLockReadGuard};

pub const MAX_SAMPLES: usize = 864_001;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoseSample {
    pub seconds: f64,
    pub translation: [f32; 3],
    /// Renderer coordinates: T * Ry * Rx * Rz.
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_records_every_fixed_step_independently_of_frame_partition() {
        let new = || {
            crate::Sailing::new(
                crate::ShipConfig::default(),
                crate::Wind {
                    speed_mps: 16.0,
                    direction: 0.3,
                },
                0.0,
            )
            .unwrap()
        };
        let mut a = new();
        let mut b = new();
        let ha = a.enable_pose_history().unwrap();
        let hb = b.enable_pose_history().unwrap();
        a.advance(2.0).unwrap();
        for _ in 0..120 {
            b.advance(1.0 / 60.0).unwrap();
        }
        assert_eq!(*ha.read().unwrap(), *hb.read().unwrap());
        assert_eq!(ha.read().unwrap().len(), 241);
        assert_eq!(*ha.read().unwrap().last().unwrap(), a.pose_sample());
    }
    #[test]
    fn history_rejects_edits_invalid_samples_and_excessive_intervals() {
        let h = PoseHistory::default();
        let sample = PoseSample {
            seconds: 0.0,
            translation: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
        };
        h.record(sample).unwrap();
        h.record(sample).unwrap();
        assert_eq!(h.read().unwrap().len(), 1);
        assert!(
            h.record(PoseSample {
                translation: [1.0; 3],
                ..sample
            })
            .is_err()
        );
        assert!(
            h.record(PoseSample {
                seconds: f64::NAN,
                ..sample
            })
            .is_err()
        );
        assert!(
            h.record(PoseSample {
                seconds: 7200.1,
                ..sample
            })
            .is_err()
        );
    }
}
#[derive(Debug, Default)]
pub struct PoseHistory {
    samples: RwLock<Vec<PoseSample>>,
}
impl PoseHistory {
    pub fn read(&self) -> Result<RwLockReadGuard<'_, Vec<PoseSample>>> {
        self.samples
            .read()
            .map_err(|_| SimError("pose history lock poisoned"))
    }
    pub fn record(&self, sample: PoseSample) -> Result<()> {
        if !sample.seconds.is_finite()
            || !(0.0..=7200.0).contains(&sample.seconds)
            || !sample
                .translation
                .into_iter()
                .chain([sample.yaw, sample.pitch, sample.roll])
                .all(f32::is_finite)
        {
            return Err(SimError("invalid pose history sample"));
        }
        let mut samples = self
            .samples
            .write()
            .map_err(|_| SimError("pose history lock poisoned"))?;
        if samples.last().is_some_and(|s| *s == sample) {
            return Ok(());
        }
        if samples.last().is_some_and(|s| s.seconds >= sample.seconds)
            || samples.len() >= MAX_SAMPLES
        {
            return Err(SimError(
                "pose history must append increasing times within its sample budget",
            ));
        }
        samples.push(sample);
        Ok(())
    }
}
