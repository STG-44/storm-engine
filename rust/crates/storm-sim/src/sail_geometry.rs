//! Retained SetGeometry, FillVertex and SetTexGrid. Oscillation and LOD are separate.
use crate::{
    sail_drive::Kind,
    sail_motion::{Geometry, SailMotion},
};
use anyhow::{Result, ensure};

pub const ROWS: usize = 17;
pub const COLUMNS: usize = 13;
type V = [f32; 3];
fn add(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] + b[i])
}
fn sub(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] - b[i])
}
fn mul(a: V, b: f32) -> V {
    a.map(|v| v * b)
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn square(a: V) -> f32 {
    a[0] * a[0] + a[1] * a[1] + a[2] * a[2]
}
fn normalized(a: V) -> V {
    // CVECTOR first sums in float, then uses a double reciprocal/sqrt.
    let len = f64::from(square(a));
    if len == 0.0 {
        return [0.0; 3];
    }
    let reciprocal = 1.0 / libm::sqrt(len);
    a.map(|v| (f64::from(v) * reciprocal) as f32)
}
fn finite(g: Geometry) -> bool {
    g.into_iter().flatten().all(f32::is_finite)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sphere {
    pub center: V,
    pub radius: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Parameters {
    pub deep_z: f32,
    pub deep_h: f32,
    pub deep_vh: f32,
    pub deep_vz: f32,
    pub holes: i32,
    pub hole_flex: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex {
    pub position: V,
    pub normal: V,
    pub uv: [[f32; 2]; 3],
}

/// Checked SetGeometry; an active rolling block receives the new saved geometry.
/// Unwritten triangle fields and dopV retain their preceding native values.
pub fn rebuild(state: &mut SailMotion, p: Parameters) -> Result<Sphere> {
    ensure!(
        state
            .hard_points
            .into_iter()
            .flatten()
            .chain([p.deep_z, p.deep_h, p.deep_vh, p.deep_vz, p.hole_flex])
            .all(f32::is_finite),
        "non-finite sail geometry input"
    );
    ensure!(
        finite(state.geometry) && state.transition.is_none_or(|t| finite(t.geometry)),
        "non-finite preceding sail geometry"
    );
    let h = state.hard_points;
    let mut g = state.transition.map_or(state.geometry, |t| t.geometry);
    let col = 1.0 / (COLUMNS - 1) as f32;
    let row = 1.0 / (ROWS - 1) as f32;
    let center;
    if state.kind == Kind::Triangle {
        let sum = add(add(h[0], h[1]), h[2]);
        let reciprocal = 1.0_f64 / 3.0_f64;
        center = sum.map(|v| (f64::from(v) * reciprocal) as f32);
        let p01 = sub(h[1], h[0]);
        let p13 = sub(h[2], h[1]);
        g[5] = normalized(cross(p13, p01));
        g[0] = mul(p13, row);
        g[3] = mul(p13, -p.deep_vh * row);
        g[4] = mul(g[3], -2.0 / ROWS as f32);
    } else {
        center = mul(add(add(add(h[0], h[1]), h[2]), h[3]), 0.25);
        let p02 = sub(h[2], h[0]);
        let p13 = sub(h[3], h[1]);
        let p01 = sub(h[1], h[0]);
        let p23 = sub(h[3], h[2]);
        g[5] = normalized(cross(p02, p01));
        g[7] = normalized(cross(p13, p01));
        let norm_ld = normalized(cross(p02, p23));
        g[6] = mul(sub(norm_ld, g[5]), row);
        g[8] = mul(sub(normalized(cross(p13, p23)), g[7]), row);
        g[2] = [0.0; 3];
        let retained = 1.0 - p.holes as f32 * p.hole_flex;
        g[0] = add(mul(mul(g[5], p.deep_z), retained), g[9]);
        g[1] = mul(g[0], -2.0 / ROWS as f32);
        g[0] = add(g[0], mul(p02, row));
        g[3] = mul(
            mul(
                add(
                    sub(mul(norm_ld, p.deep_h), mul(p13, p.deep_vh)),
                    mul(g[7], p.deep_vz),
                ),
                row,
            ),
            retained,
        );
        g[4] = mul(g[3], -2.0 * col);
        g[3] = add(g[3], mul(mul(sub(p13, p02), col), row));
    }
    let sphere = Sphere {
        center,
        radius: libm::sqrtf(square(sub(h[0], center))),
    };
    ensure!(
        finite(g)
            && sphere
                .center
                .into_iter()
                .chain([sphere.radius])
                .all(f32::is_finite),
        "sail geometry overflow"
    );
    if let Some(t) = &mut state.transition {
        t.geometry = g;
        state.geometry[9] = g[9];
    } else {
        state.geometry = g;
    }
    Ok(sphere)
}

/// No-limit-point SetSail depth expressions with explicit unit samples.
/// This numeric preview provider does not implement native RNG or SetSail lifecycle.
pub fn preview_parameters(
    kind: Kind,
    h: [V; 4],
    turning: bool,
    holes: i32,
    hole_flex: f32,
) -> Result<Parameters> {
    ensure!(
        h.into_iter().flatten().all(f32::is_finite),
        "non-finite sail hard points"
    );
    let (width, height) = if kind == Kind::Triangle {
        (
            libm::sqrtf(square(sub(h[2], h[0]))),
            libm::sqrtf(square(sub(h[1], h[2]))),
        )
    } else {
        (
            0.5 * libm::sqrtf(square(sub(sub(add(h[0], h[2]), h[1]), h[3]))),
            0.5 * libm::sqrtf(square(sub(sub(add(h[0], h[1]), h[2]), h[3]))),
        )
    };
    ensure!(
        [width, height]
            .into_iter()
            .all(|v| v.is_finite() && v >= 0.1),
        "degenerate sail dimensions"
    );
    let mut p = Parameters {
        holes,
        hole_flex,
        ..Default::default()
    };
    match kind {
        Kind::Triangle => {
            p.deep_z = 0.04 + 0.01 * 0.5 * height;
            p.deep_h = p.deep_z;
            p.deep_vh = if turning {
                0.0
            } else {
                0.005 + 0.005 * 0.5 * width
            };
        }
        Kind::Trapezoid => {
            p.deep_z = 0.01 + 0.01 * 0.5 * height;
            p.deep_h = 0.01 + 0.01 * 0.5 * width;
        }
        _ => {
            p.deep_z = (0.06 + 0.02 * 0.5) * height;
            p.deep_h = (0.06 + 0.02 * 0.5) * width * 0.6;
            p.deep_vh = p.deep_z * 0.1;
            p.deep_vz = p.deep_z * 0.1;
        }
    }
    Ok(p)
}

/// Full native fixed grid. All three UV sets retain SetTexGrid's coordinates.
pub fn fill(state: &SailMotion, sphere: Sphere, gerald: [f32; 2]) -> Result<Vec<Vertex>> {
    ensure!(
        finite(state.geometry)
            && state
                .hard_points
                .into_iter()
                .flatten()
                .chain(sphere.center)
                .chain([sphere.radius])
                .chain(gerald)
                .all(f32::is_finite),
        "non-finite sail vertex input"
    );
    let h = state.hard_points;
    let g = state.geometry;
    let tri = state.kind == Kind::Triangle;
    let row = 1.0 / (ROWS - 1) as f32;
    let col = 1.0 / (COLUMNS - 1) as f32;
    let mut start = h[0];
    let mut dv_start = g[0];
    let mut dh_start = g[3];
    let step = mul(sub(h[1], start), if tri { row } else { col });
    let normal_origin = sub(
        sphere.center,
        mul(
            if tri || state.kind == Kind::Trapezoid {
                g[5]
            } else {
                g[6]
            },
            sphere.radius,
        ),
    );
    let mut vertices = Vec::with_capacity(if tri {
        ROWS * (ROWS + 1) / 2
    } else {
        ROWS * COLUMNS
    });
    for ix in 0..if tri { ROWS } else { COLUMNS } {
        let mut position = start;
        let mut dv = dv_start;
        let mut ddv = g[1];
        for _ in 0..if tri { ix + 1 } else { ROWS } {
            vertices.push(Vertex {
                position,
                normal: normalized(sub(position, normal_origin)),
                ..Default::default()
            });
            if !tri {
                ddv = add(ddv, g[2]);
                dv = add(dv, ddv);
            }
            position = add(position, dv);
        }
        start = add(start, step);
        dh_start = add(dh_start, g[4]);
        dv_start = add(dv_start, dh_start);
    }
    let n = vertices.len();
    vertices[n - ROWS].position = h[1];
    vertices[n - 1].position = h[if tri { 2 } else { 3 }];
    if !tri {
        vertices[ROWS - 1].position = h[2];
    }
    let mut idx = 0;
    for ix in 0..if tri { ROWS } else { COLUMNS } {
        let tu = ix as f32 * if tri { row } else { col };
        let tu3 = ix as f32
            * if tri {
                0.5 / (ROWS - 1) as f32
            } else {
                0.5 / (COLUMNS - 1) as f32
            }
            + 1.0;
        for iy in if tri { ROWS - ix - 1 } else { 0 }..ROWS {
            let tv = iy as f32 * row;
            let center = if tri { 0.75 } else { 0.5 };
            vertices[idx].uv = [
                [tu, tv],
                [
                    (tu - center) * gerald[0] + 0.5,
                    (tv - center) * gerald[1] + 0.5,
                ],
                [
                    tu3,
                    if tri { 0.5 } else { 0.0 } + iy as f32 * (0.5 / (ROWS - 1) as f32),
                ],
            ];
            idx += 1;
        }
    }
    ensure!(
        vertices.iter().all(|v| v
            .position
            .into_iter()
            .chain(v.normal)
            .chain(v.uv.into_iter().flatten())
            .all(f32::is_finite)),
        "sail vertex overflow"
    );
    Ok(vertices)
}

#[cfg(test)]
mod tests;
