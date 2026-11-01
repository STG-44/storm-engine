//! Retained SEA bump-frame preparation, animation and scalar WaveXZ queries.
//! Texture loading, weather attributes and native entity scheduling are providers.
use anyhow::{Result, ensure};
use std::sync::Arc;

pub const SIDE: usize = 128;
pub const CELLS: usize = SIDE * SIDE;
pub const FRAMES: usize = 64;

#[derive(Debug)]
pub struct Frames {
    heights: Vec<Vec<u8>>,
    normals: Vec<Vec<u32>>,
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    // CVECTOR: float squared sum, then double reciprocal sqrt and products.
    let length = f64::from(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    if length == 0.0 {
        return [0.0; 3];
    }
    let inverse = 1.0 / libm::sqrt(length);
    v.map(|x| (f64::from(x) * inverse) as f32)
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
impl Frames {
    /// Blue bytes in original file order. Init's five-frame filter is dead code:
    /// the actual assignment copies the current raw frame unchanged.
    pub fn new(heights: Vec<Vec<u8>>) -> Result<Self> {
        ensure!(
            heights.len() == FRAMES && heights.iter().all(|f| f.len() == CELLS),
            "SEA requires 64 128x128 frames"
        );
        let normals = heights
            .iter()
            .map(|frame| {
                (0..CELLS)
                    .map(|index| {
                        let x = index % SIDE;
                        let y = index / SIDE;
                        let get = |dx: usize, dy: usize| {
                            f32::from(
                                frame[((x.wrapping_add(dx)) & 127)
                                    + ((y.wrapping_add(dy)) & 127) * SIDE],
                            ) / 255.0
                        };
                        let center = get(0, 0);
                        let d0 = [-1.0, get(usize::MAX, 0) - center, 0.0];
                        let d1 = [0.0, get(0, usize::MAX) - center, -1.0];
                        let d2 = [1.0, get(1, 0) - center, 0.0];
                        let d3 = [0.0, get(0, 1) - center, 1.0];
                        let a = cross(d1, d0);
                        let b = cross(d3, d2);
                        let c = cross(d0, d3);
                        let d = cross(d2, d1);
                        let n = normalize(std::array::from_fn(|i| ((a[i] + b[i]) + c[i]) + d[i]));
                        let nx = (n[0] * 32767.5) as i16 as u16;
                        let nz = (n[2] * 32767.5) as i16 as u16;
                        u32::from(nx) | (u32::from(nz) << 16)
                    })
                    .collect()
            })
            .collect();
        Ok(Self { heights, normals })
    }

    /// Resource-free, intentionally synthetic inputs for numeric/render checks.
    pub fn fixture() -> Self {
        Self::new(
            (0..FRAMES)
                .map(|f| {
                    (0..CELLS)
                        .map(|i| {
                            let x = i % SIDE;
                            let z = i / SIDE;
                            ((x * 3 + z * 5 + f * 7 + (x * z + f) % 19) % 256) as u8
                        })
                        .collect()
                })
                .collect(),
        )
        .unwrap()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parameters {
    pub amplitude: [f32; 2],
    pub scale: [f32; 2],
    pub animation_speed: [f32; 2],
    pub move_speed: [[f32; 2]; 2],
    pub max_distance: f32,
}
impl Default for Parameters {
    /// Original EditMode_Update values, not game weather attributes.
    fn default() -> Self {
        Self {
            amplitude: [8.0, 1.0],
            scale: [0.5, 2.0],
            animation_speed: [6.0, 3.0],
            move_speed: [[5.0, 0.0], [0.0, 2.0]],
            max_distance: 1600.0,
        }
    }
}
impl Parameters {
    fn validate(self) -> Result<()> {
        ensure!(
            self.amplitude
                .into_iter()
                .map(f32::abs)
                .sum::<f32>()
                .is_finite(),
            "SEA envelope overflow"
        );
        ensure!(
            self.amplitude
                .into_iter()
                .chain(self.scale)
                .chain(self.animation_speed)
                .chain(self.move_speed.into_iter().flatten())
                .chain([self.max_distance, self.max_distance * self.max_distance])
                .all(f32::is_finite)
                && self.animation_speed.into_iter().all(|v| v >= 0.0)
                && self.max_distance >= 0.0,
            "invalid SEA parameters"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Animation {
    pub frame: [f32; 2],
    pub movement: [[f32; 2]; 2],
}
impl Default for Animation {
    fn default() -> Self {
        Self {
            frame: [0.0, 10.0],
            movement: [[0.0; 2]; 2],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Surface {
    pub parameters: Parameters,
    pub animation: Animation,
    /// Native X/Z query camera (not renderer Z).
    pub camera: [f32; 2],
    // Per layer: height[CELLS], then interleaved normal X/Z[2*CELLS].
    maps: Arc<Vec<f32>>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub height: f32,
    pub normal: [f32; 3],
}

impl Surface {
    pub fn maps(&self) -> &[f32] {
        &self.maps
    }
    pub fn validate(&self) -> Result<()> {
        self.parameters.validate()?;
        ensure!(
            self.camera
                .into_iter()
                .chain(self.animation.movement.into_iter().flatten())
                .all(f32::is_finite),
            "invalid SEA camera/movement"
        );
        Ok(())
    }
    fn query(&self, x: f32, z: f32, normal: bool) -> Result<Sample> {
        self.validate()?;
        ensure!(x.is_finite() && z.is_finite(), "non-finite SEA query");
        let dx = x - self.camera[0];
        let dz = z - self.camera[1];
        if dx * dx + dz * dz > self.parameters.max_distance * self.parameters.max_distance {
            return Ok(Sample {
                height: 0.0,
                normal: [0.0, 1.0, 0.0],
            });
        }
        let mut height = 0.0;
        let mut ns = [[0.0; 2]; 2];
        for (layer, n) in ns.iter_mut().enumerate() {
            let scale = self.parameters.scale[layer];
            let xx = (x + self.animation.movement[layer][0]) * scale;
            let zz = (z + self.animation.movement[layer][1]) * scale;
            // Native ffloor is SSE round-to-nearest-even(x - .5), NOT floorf.
            let ix = (xx - 0.5).round_ties_even();
            let iz = (zz - 0.5).round_ties_even();
            ensure!(
                [ix, iz]
                    .into_iter()
                    .all(|v| v.is_finite() && (-2_147_483_648.0..2_147_483_520.0).contains(&v)),
                "SEA sample coordinate out of native integer range"
            );
            let fx = xx - ix;
            let fz = zz - iz;
            let ix = ix as i32;
            let iz = iz as i32;
            let indices = [(ix, iz), (ix + 1, iz), (ix, iz + 1), (ix + 1, iz + 1)]
                .map(|(x, z)| (x & 127) as usize + (z & 127) as usize * SIDE);
            let interpolate = |offset: usize, stride: usize| {
                let [a, b, c, d] =
                    indices.map(|i| self.maps[layer * 3 * CELLS + offset + i * stride]);
                a + fx * (b - a) + fz * (c - a) + fx * fz * (d + a - b - c)
            };
            let value = self.parameters.amplitude[layer] * interpolate(0, 1);
            if layer == 0 {
                height = value;
            } else {
                height += value;
            }
            if normal {
                *n = [interpolate(CELLS, 2), interpolate(CELLS + 1, 2)];
            }
        }
        let normal = if normal {
            let y = ns.map(|n| libm::sqrtf(1.0 - (n[0] * n[0] + n[1] * n[1])));
            let p = self.parameters;
            normalize([
                p.scale[0] * p.amplitude[0] * ns[0][0] + p.scale[1] * p.amplitude[1] * ns[1][0],
                y[0] + y[1],
                p.scale[0] * p.amplitude[0] * ns[0][1] + p.scale[1] * p.amplitude[1] * ns[1][1],
            ])
        } else {
            [0.0; 3]
        };
        ensure!(
            height.is_finite() && normal.into_iter().all(f32::is_finite),
            "SEA sample overflow/invalid normal"
        );
        Ok(Sample { height, normal })
    }
    pub fn sample(&self, x: f32, z: f32) -> Result<Sample> {
        self.query(x, z, true)
    }
}
impl crate::ship_rocking::SeaSurface for Surface {
    fn wave_xz(&mut self, x: f32, z: f32) -> Result<f32> {
        Ok(self.query(x, z, false)?.height)
    }
}

#[derive(Clone, Debug)]
pub struct Sea {
    frames: Arc<Frames>,
    surface: Surface,
}
impl Sea {
    pub fn new(frames: Arc<Frames>, parameters: Parameters) -> Result<Self> {
        parameters.validate()?;
        let mut sea = Self {
            frames,
            surface: Surface {
                parameters,
                animation: Animation::default(),
                camera: [0.0; 2],
                maps: Arc::new(Vec::new()),
            },
        };
        sea.advance(0)?;
        Ok(sea)
    }
    pub fn surface(&self) -> &Surface {
        &self.surface
    }
    pub fn set_camera(&mut self, camera: [f32; 2]) -> Result<()> {
        ensure!(camera.into_iter().all(f32::is_finite), "invalid SEA camera");
        self.surface.camera = camera;
        Ok(())
    }
    /// Retained Realize animation arithmetic, bounded to one second per call.
    /// Failed updates leave the published surface unchanged; pausing calls no step.
    pub fn advance(&mut self, delta_ms: u32) -> Result<()> {
        ensure!(delta_ms <= 1000, "SEA update exceeds one second");
        let p = self.surface.parameters;
        p.validate()?;
        let delta = delta_ms as f32 * 0.001;
        let mut a = self.surface.animation;
        for l in 0..2 {
            for i in 0..2 {
                a.movement[l][i] += delta * p.move_speed[l][i];
            }
            a.frame[l] += delta * p.animation_speed[l];
            ensure!(
                a.frame[l].is_finite()
                    && a.frame[l] < 1_000_000.0
                    && a.movement[l].into_iter().all(f32::is_finite),
                "SEA animation overflow"
            );
            while a.frame[l] >= FRAMES as f32 {
                a.frame[l] -= FRAMES as f32;
            }
        }
        let mut maps = vec![0.0; 6 * CELLS];
        for l in 0..2 {
            let first = a.frame[l] as usize % FRAMES;
            let second = (first + 1) % FRAMES;
            let fraction = a.frame[l] - first as f32;
            for i in 0..CELLS {
                let h1 = f32::from(self.frames.heights[first][i]);
                let h2 = f32::from(self.frames.heights[second][i]);
                maps[l * 3 * CELLS + i] = (1.0 / 255.0) * (h1 + (h2 - h1) * fraction);
                for c in 0..2 {
                    let unpack = |v: u32| ((v >> (c * 16)) as i16) as f32 / 32767.5;
                    let n1 = unpack(self.frames.normals[first][i]);
                    let n2 = unpack(self.frames.normals[second][i]);
                    // CalculateNormalMap's amplitude argument is unused in C++.
                    maps[l * 3 * CELLS + CELLS + i * 2 + c] = n1 + (n2 - n1) * fraction;
                }
            }
        }
        self.surface.animation = a;
        self.surface.maps = Arc::new(maps);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
