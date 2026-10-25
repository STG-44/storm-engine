//! Retained SHIP::ShipRocking: a 6-by-6 water sample grid and native smoothing.
use anyhow::{Result, ensure};

pub const GRID_SIDE: usize = 6;

/// SEA_BASE::WaveXZ is an external dependency, not a water simulation here.
pub trait SeaSurface {
    fn wave_xz(&mut self, x: f32, z: f32) -> Result<f32>;
    fn sin(&mut self, angle: f32) -> f32 {
        libm::sinf(angle)
    }
    fn cos(&mut self, angle: f32) -> f32 {
        libm::cosf(angle)
    }
    fn atan(&mut self, value: f32) -> f32 {
        libm::atanf(value)
    }
}

/// Float coefficients shared with the diagnostic water shader. This is not SEA.
#[derive(Clone, Debug)]
pub struct DiagnosticSea {
    pub coefficients: [[f32; 5]; 2],
    pub seconds: f32,
}
impl DiagnosticSea {
    pub fn new(waves: crate::Waves) -> Result<Self> {
        waves.validate()?;
        Ok(Self {
            coefficients: waves.components().map(|c| {
                [
                    c.k.x as f32,
                    c.k.y as f32,
                    c.omega as f32,
                    c.amplitude as f32,
                    c.phase as f32,
                ]
            }),
            seconds: 0.0,
        })
    }
}
impl SeaSurface for DiagnosticSea {
    fn wave_xz(&mut self, x: f32, z: f32) -> Result<f32> {
        ensure!(
            x.is_finite() && z.is_finite() && self.seconds.is_finite(),
            "invalid diagnostic sea query"
        );
        let mut height = 0.0;
        for [kx, kz, omega, amplitude, phase] in self.coefficients {
            let phase = (kx * x + kz * z) - omega * self.seconds + phase;
            height += amplitude * libm::sinf(phase);
        }
        ensure!(height.is_finite(), "diagnostic sea overflow");
        Ok(height)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RockingParameters {
    /// Native State.vBoxSize X/Z from the root GM, not the assembled rig bounds.
    pub size: [f32; 2],
    pub offsets: [f32; 2],
    /// Native fRockingY and fRockingAZ; both start at one.
    pub height_factor: f32,
    pub angle_factor: f32,
}
impl RockingParameters {
    pub fn new(size: [f32; 2]) -> Self {
        Self {
            size,
            offsets: [0.0; 2],
            height_factor: 1.0,
            angle_factor: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RockingInput {
    pub position: [f32; 3],
    pub angles: [f32; 3],
    /// Native vAng, independently captured by the caller before Move.
    pub previous_angles: [f32; 3],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RockingOutput {
    pub height: f32,
    pub angles: [f32; 3],
}
#[derive(Clone, Debug, PartialEq)]
pub struct ShipRocking {
    pub parameters: RockingParameters,
    /// Native ShipPoints[ix][iz].fY. Other ship_point_t fields are unused here.
    pub heights: [[f32; GRID_SIDE]; GRID_SIDE],
}
impl ShipRocking {
    pub fn new(size: [f32; 2]) -> Self {
        Self {
            parameters: RockingParameters::new(size),
            heights: [[0.0; GRID_SIDE]; GRID_SIDE],
        }
    }
    /// Invalid input/provider output leaves the stored grid unchanged.
    /// External sea queries cannot be rolled back.
    pub fn advance(
        &mut self,
        input: RockingInput,
        seconds: f32,
        sea: Option<&mut dyn SeaSurface>,
    ) -> Result<RockingOutput> {
        let p = self.parameters;
        ensure!(
            input
                .position
                .into_iter()
                .chain(input.angles)
                .chain(input.previous_angles)
                .chain(p.size)
                .chain(p.offsets)
                .chain([p.height_factor, p.angle_factor, seconds])
                .all(f32::is_finite)
                && seconds >= 0.0,
            "non-finite or negative ship rocking input"
        );
        let Some(sea) = sea else {
            return Ok(RockingOutput {
                height: input.position[1],
                angles: input.previous_angles,
            });
        };
        ensure!(
            p.size.into_iter().all(|v| v > 0.0),
            "ship rocking requires positive root dimensions"
        );
        let delta = seconds.min(0.1) / 0.025;
        let cos = sea.cos(input.angles[1]);
        let sin = sea.sin(input.angles[1]);
        ensure!(
            cos.is_finite() && sin.is_finite(),
            "non-finite rocking rotation"
        );
        let mut heights = [[0.0; GRID_SIDE]; GRID_SIDE];
        let mut total = 0.0;
        for (ix, row) in heights.iter_mut().enumerate() {
            let x = ix as f32 * p.size[0] * 0.2 - 0.5 * p.size[0];
            for (iz, height) in row.iter_mut().enumerate() {
                let z = iz as f32 * p.size[1] * 0.2 - 0.5 * p.size[1];
                // Retained RotateAroundY, including operation and offset order.
                let xx = x * cos + z * sin;
                let zz = z * cos - x * sin;
                let x = xx + input.position[0] + p.offsets[0];
                let z = zz + input.position[2] + p.offsets[1];
                ensure!(
                    x.is_finite() && z.is_finite(),
                    "rocking sample position overflow"
                );
                *height = sea.wave_xz(x, z)?;
                ensure!(height.is_finite(), "non-finite rocking water sample");
                total += *height;
            }
        }
        let height = input.position[1]
            + (delta * p.height_factor).min(1.0) * (total / 36.0 - input.position[1]);
        let mut target = input.angles;
        let mut angle = 0.0;
        for iz in 0..3 {
            let mut a = 0.0;
            let mut b = 0.0;
            for row in &heights {
                a += row[iz];
                b += row[5 - iz];
            }
            a /= 6.0;
            b /= 6.0;
            let z = 1.0 / (0.5 * p.size[1] - iz as f32 * p.size[1] * 0.2);
            angle += sea.atan((a - b) / 2.0 * z);
        }
        target[0] = angle / 3.0;
        angle = 0.0;
        for ix in 0..3 {
            let mut a = 0.0;
            let mut b = 0.0;
            for (&left, &right) in heights[ix].iter().zip(&heights[5 - ix]) {
                a += left;
                b += right;
            }
            a /= 6.0;
            b /= 6.0;
            let x = 1.0 / (0.5 * p.size[0] - ix as f32 * p.size[0] * 0.2);
            angle += sea.atan((a - b) / 2.0 * x);
        }
        target[2] = -angle / 3.0;
        let angles = std::array::from_fn(|i| {
            input.previous_angles[i]
                + (p.angle_factor * delta).min(1.0) * (target[i] - input.previous_angles[i])
        });
        ensure!(
            height.is_finite() && angles.into_iter().all(f32::is_finite),
            "ship rocking result overflow"
        );
        self.heights = heights;
        Ok(RockingOutput { height, angles })
    }
}

#[cfg(test)]
mod tests;
