//! Full retained GetGrid; world matrices and camera ownership are explicit ports.
use crate::sail_topology::Grid;
use anyhow::{Result, ensure};
pub type V = [f32; 3];
pub const VERTICAL_FOV: f32 = std::f32::consts::FRAC_PI_4;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub eye: V,
    pub perspective: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selection {
    pub grid: Grid,
    pub world_center: V,
}
impl Default for Selection {
    fn default() -> Self {
        Self {
            grid: Grid {
                rows: 0,
                columns: 0,
            },
            world_center: [0.0; 3],
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Change {
    pub changed: bool,
    pub distance: Option<f32>,
}
impl Selection {
    pub fn update(
        &mut self,
        center: V,
        matrix: [f32; 16],
        camera: Camera,
        rolled: bool,
        free: bool,
    ) -> Result<Change> {
        let mut next = *self;
        let distance;
        if rolled {
            next.grid = Grid::ROLLED;
            distance = None;
        } else if free {
            next.grid = Grid {
                rows: 5,
                columns: 4,
            };
            distance = None;
        } else {
            ensure!(
                center
                    .into_iter()
                    .chain(matrix)
                    .chain(camera.eye)
                    .chain([camera.perspective])
                    .all(f32::is_finite),
                "non-finite GetGrid input"
            );
            let pos = std::array::from_fn::<_, 3, _>(|i| {
                matrix[i] * center[0]
                    + matrix[i + 4] * center[1]
                    + matrix[i + 8] * center[2]
                    + matrix[i + 12]
            });
            let d = std::array::from_fn::<_, 3, _>(|i| pos[i] - camera.eye[i]);
            let projected = libm::sqrtf(d[0] * d[0] + d[1] * d[1] + d[2] * d[2])
                * libm::tanf(camera.perspective * 0.5);
            ensure!(
                pos.into_iter().chain([projected]).all(f32::is_finite),
                "GetGrid overflow"
            );
            next.world_center = pos;
            next.grid = if projected < 0.0 {
                Grid::NEAR
            } else {
                Grid::select(projected, false, false)?
            };
            distance = Some(projected);
        }
        let changed = next.grid != self.grid;
        *self = next;
        Ok(Change { changed, distance })
    }
}
/// Pinned viewer orbit construction, shared by the GPU view and CPU LOD input.
/// This is an adapter, not the native camera entity's orientation construction.
pub fn orbit_eye(target: V, yaw: f32, pitch: f32, distance: f32) -> Result<V> {
    ensure!(
        target
            .into_iter()
            .chain([yaw, pitch, distance])
            .all(f32::is_finite)
            && distance > 0.0,
        "invalid sail camera"
    );
    let direction = [
        libm::sinf(yaw) * libm::cosf(pitch),
        libm::sinf(pitch),
        libm::cosf(yaw) * libm::cosf(pitch),
    ];
    let eye = std::array::from_fn(|i| target[i] + direction[i] * distance);
    ensure!(eye.into_iter().all(f32::is_finite), "sail camera overflow");
    Ok(eye)
}
#[cfg(test)]
mod tests;
