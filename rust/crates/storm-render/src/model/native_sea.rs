//! Shared immutable CPU surface uploaded to the preview water sampler.
use storm_sim::sea::Surface;

#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Uniform {
    pub layers: [[f32; 4]; 2],
    pub camera: [f32; 4],
    pub bounds: [f32; 4],
}
impl From<&Surface> for Uniform {
    fn from(s: &Surface) -> Self {
        Self {
            layers: std::array::from_fn(|i| {
                [
                    s.parameters.amplitude[i],
                    s.parameters.scale[i],
                    s.animation.movement[i][0],
                    s.animation.movement[i][1],
                ]
            }),
            camera: [s.camera[0], s.camera[1], s.parameters.max_distance, 1.0],
            bounds: [
                s.parameters.amplitude.into_iter().map(f32::abs).sum(),
                0.0,
                0.0,
                0.0,
            ],
        }
    }
}

#[cfg(test)]
mod tests;
