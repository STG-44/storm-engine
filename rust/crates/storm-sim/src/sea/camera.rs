//! Retained CMatrix::BuildViewMatrix and rigid Transposition for SEA::Realize.
use anyhow::{Result, ensure};
pub type V = [f32; 3];
pub type Matrix = [f32; 16];
const IDENTITY: Matrix = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];
fn dot(a: V, b: V) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalized(v: V, length: f32) -> V {
    let k = 1.0 / libm::sqrtf(length);
    v.map(|x| x * k)
}
/// Returns the original success flag, including degenerate identity fallbacks.
pub fn build_view(from: V, to: V, mut up: V) -> Result<(Matrix, bool)> {
    ensure!(
        from.into_iter().chain(to).chain(up).all(f32::is_finite),
        "invalid view input"
    );
    let mut m = IDENTITY;
    let mut look = std::array::from_fn(|i| to[i] - from[i]);
    let length = dot(look, look);
    if length == 0.0 {
        m[12..15].copy_from_slice(&from.map(|x| -x));
        return Ok((m, false));
    }
    look = normalized(look, length);
    let projection = dot(look, up);
    up = std::array::from_fn(|i| up[i] - look[i] * projection);
    let length = dot(up, up);
    if length == 0.0 {
        up[1] = 1.0;
    } else {
        up = normalized(up, length);
    }
    let right = cross(up, look);
    let length = dot(right, right);
    if length != 0.0 {
        let right = normalized(right, length);
        // Native m[column][row]; the basis first assigned by C++ is overwritten.
        for i in 0..3 {
            m[i * 4] = right[i];
            m[i * 4 + 1] = up[i];
            m[i * 4 + 2] = look[i];
        }
    }
    for i in 0..3 {
        m[12 + i] = -(m[i] * from[0] + m[4 + i] * from[1] + m[8 + i] * from[2]);
    }
    ensure!(
        m.into_iter().all(f32::is_finite),
        "view construction overflow"
    );
    Ok((m, true))
}
/// This is the native rigid transform transpose, not a general matrix inverse.
pub fn transpose(mut m: Matrix) -> Result<Matrix> {
    ensure!(m.into_iter().all(f32::is_finite), "non-finite native view");
    let p = [m[12], m[13], m[14]];
    let t = std::array::from_fn::<_, 3, _>(|i| -dot(p, [m[i * 4], m[i * 4 + 1], m[i * 4 + 2]]));
    m[12..15].copy_from_slice(&t);
    m.swap(1, 4);
    m.swap(2, 8);
    m.swap(6, 9);
    ensure!(
        m.into_iter().all(f32::is_finite),
        "native view transpose overflow"
    );
    Ok(m)
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub view: Matrix,
    pub eye: V,
    pub world_offset: V,
}
impl Camera {
    pub fn from_view(view: Matrix) -> Result<Self> {
        let inverse = transpose(view)?;
        let eye = [inverse[12], inverse[13], inverse[14]];
        Ok(Self {
            view,
            eye,
            world_offset: [eye[0], 0.0, eye[2]],
        })
    }
}
/// Boundary between renderer RH coordinates and Storm's native coordinates.
pub fn reflect_view(m: Matrix) -> Matrix {
    std::array::from_fn(|i| {
        let mut x = m[i];
        if i % 4 == 2 {
            x = -x;
        }
        if i / 4 == 2 {
            x = -x;
        }
        x
    })
}
#[cfg(test)]
mod tests;
