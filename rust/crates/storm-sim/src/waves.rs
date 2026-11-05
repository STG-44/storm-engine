//! Two deep-water sine components, shared with the renderer via explicit coefficients.
//! This is a new diagnostic sea, not the original Storm water or a fluid solver.
use crate::{Result, SimError};
use glam::DVec2;

#[derive(Clone, Copy, Debug, Default)]
pub struct Waves {
    /// Maximum crest-to-trough envelope in metres (not statistical significant height).
    pub height_m: f64,
    /// Travel direction, 0 towards +Z, pi/2 towards +X.
    pub direction: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct WaveComponent {
    pub amplitude: f64,
    pub k: DVec2,
    pub omega: f64,
    pub phase: f64,
}

impl Waves {
    pub fn validate(self) -> Result<()> {
        if !self.height_m.is_finite()
            || !(0.0..=4.0).contains(&self.height_m)
            || !self.direction.is_finite()
        {
            return Err(SimError(
                "waves must have finite direction and height 0..=4 m",
            ));
        }
        Ok(())
    }

    pub fn components(self) -> [WaveComponent; 2] {
        [(0.7, 64.0, 0.0, 0.0), (0.3, 26.0, 0.65, 1.1)].map(
            |(weight, wavelength, offset, phase)| {
                let k = std::f64::consts::TAU / wavelength;
                let direction = self.direction + offset;
                WaveComponent {
                    amplitude: self.height_m * 0.5 * weight,
                    k: DVec2::new(direction.sin(), direction.cos()) * k,
                    omega: (9.81 * k).sqrt(),
                    phase,
                }
            },
        )
    }

    /// Height and analytic X/Z gradient at world position/time.
    pub fn sample(self, position: DVec2, seconds: f64) -> (f64, DVec2) {
        let mut height = 0.0;
        let mut gradient = DVec2::ZERO;
        for c in self.components() {
            let phase = c.k.dot(position) - c.omega * seconds + c.phase;
            height += c.amplitude * phase.sin();
            gradient += c.k * (c.amplitude * phase.cos());
        }
        (height, gradient)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HullMotion {
    pub heave_m: f64,
    /// Bow up, radians, relative to the mean water plane.
    pub pitch: f64,
    /// Starboard up, radians.
    pub roll: f64,
    /// Heave (m/s), pitch and roll (rad/s).
    pub rates: [f64; 3],
}

impl HullMotion {
    pub(crate) fn step(&mut self, target: [f64; 3]) {
        let mut value = [self.heave_m, self.pitch, self.roll];
        // Damped spring response to a five-point surface fit, fixed at 120 Hz.
        for (i, frequency) in [1.2_f64, 1.0, 1.4].into_iter().enumerate() {
            let acceleration =
                frequency.powi(2) * (target[i] - value[i]) - 2.0 * 0.85 * frequency * self.rates[i];
            self.rates[i] += acceleration * crate::STEP_SECONDS;
            value[i] += self.rates[i] * crate::STEP_SECONDS;
        }
        [self.heave_m, self.pitch, self.roll] = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_gradient_matches_finite_difference_and_height_is_bounded() {
        let waves = Waves {
            height_m: 4.0,
            direction: 1.2,
        };
        for i in 0..100 {
            let p = DVec2::new(i as f64 * 0.71, i as f64 * -0.37);
            let time = i as f64 * 0.29;
            let (height, gradient) = waves.sample(p, time);
            assert!(height.abs() <= 2.0);
            for axis in [DVec2::X, DVec2::Y] {
                let epsilon = 1e-4;
                let finite = (waves.sample(p + axis * epsilon, time).0
                    - waves.sample(p - axis * epsilon, time).0)
                    / (2.0 * epsilon);
                assert!((finite - gradient.dot(axis)).abs() < 1e-8);
            }
        }
        assert_eq!(
            Waves::default().sample(DVec2::ZERO, 12.0),
            (0.0, DVec2::ZERO)
        );
        assert!(
            Waves {
                height_m: f64::NAN,
                ..waves
            }
            .validate()
            .is_err()
        );
        assert!(
            Waves {
                height_m: 4.1,
                ..waves
            }
            .validate()
            .is_err()
        );
    }
}
