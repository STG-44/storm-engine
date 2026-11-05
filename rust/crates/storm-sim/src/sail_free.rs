//! Retained DoSFreeSail/DoTFreeSail with explicit matrix axes and rand draws.
//! Native overshoot is preserved. Non-finite native results are rejected atomically.
use crate::{sail_drive::Kind, sail_geometry::Vertex, sail_motion::SailMotion, sail_wave::State};
use anyhow::{Result, ensure};
type V = [f32; 3];
fn add(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] + b[i])
}
fn sub(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] - b[i])
}
fn mul(a: V, b: f32) -> V {
    a.map(|x| x * b)
}
fn square(a: V) -> f32 {
    a[0] * a[0] + a[1] * a[1] + a[2] * a[2]
}
fn normalized(a: V) -> V {
    let len = f64::from(square(a));
    if len == 0.0 {
        return [0.0; 3];
    }
    let r = 1.0 / libm::sqrt(len);
    a.map(|x| (f64::from(x) * r) as f32)
}
#[derive(Clone, Copy, Debug)]
pub struct Parameters {
    pub height: f32,
    pub sail_wind: V,
    pub wind_base: f32,
    /// The three native matrix columns consumed by MulToInvNorm.
    pub axes: [V; 3],
    pub addition_min: f32,
    pub addition_random: f32,
}
impl Default for Parameters {
    fn default() -> Self {
        Self {
            height: 1.0,
            sail_wind: [0.0; 3],
            wind_base: 0.4,
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            addition_min: 0.2,
            addition_random: 0.2,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Draws<'a> {
    pub values: &'a [u32],
    pub maximum: u32,
}
#[derive(Debug)]
pub struct NativeNonFinite;
impl std::fmt::Display for NativeNonFinite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native free sail produced non-finite coordinates")
    }
}
impl std::error::Error for NativeNonFinite {}
pub fn draw_count(kind: Kind) -> usize {
    if kind == Kind::Triangle { 10 } else { 16 }
}
/// Pinned construction for the diagnostic mast transform, not native mast physics.
pub fn preview_rotation_z(angle: f32) -> [f32; 16] {
    let (s, c) = (libm::sinf(angle), libm::cosf(angle));
    [
        c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}
/// The original normalization followed by MulToInvNorm's column dot products.
pub fn local_direction(p: Parameters) -> Result<V> {
    ensure!(
        p.axes
            .into_iter()
            .flatten()
            .chain(p.sail_wind)
            .chain([p.wind_base])
            .all(f32::is_finite),
        "non-finite free sail direction input"
    );
    let n = normalized(add([0.0, -1.0, 0.0], mul(p.sail_wind, p.wind_base)));
    let direction = p.axes.map(|a| n[0] * a[0] + n[1] * a[1] + n[2] * a[2]);
    if !direction.into_iter().all(f32::is_finite) {
        return Err(NativeNonFinite.into());
    }
    Ok(direction)
}
/// One original free frame, including goWave's sample-based box update.
/// Returns the consumed draw count; matrices/RNG ownership are caller ports.
pub fn advance(
    s: &mut State,
    m: &SailMotion,
    vertices: &mut [Vertex],
    p: Parameters,
    draws: Draws<'_>,
) -> Result<usize> {
    let tri = m.kind == Kind::Triangle;
    let count = draw_count(m.kind);
    ensure!(!m.rolling, "rolled state takes precedence over free cloth");
    ensure!(
        vertices.len() == if tri { 153 } else { 221 },
        "invalid free sail grid"
    );
    ensure!(
        (1..=i32::MAX as u32).contains(&draws.maximum) && draws.values.len() >= count,
        "invalid free sail random stream"
    );
    ensure!(
        draws.values[..count].iter().all(|v| *v <= draws.maximum),
        "free sail draw exceeds RAND_MAX"
    );
    ensure!(
        m.hard_points
            .into_iter()
            .flatten()
            .chain(s.points.into_iter().flatten())
            .chain(s.rope_points.into_iter().flatten())
            .chain(p.axes.into_iter().flatten())
            .chain(p.sail_wind)
            .chain([
                p.height,
                p.wind_base,
                p.addition_min,
                p.addition_random,
                s.sum_wind,
                s.maximum_sum_wind
            ])
            .all(f32::is_finite)
            && vertices
                .iter()
                .flat_map(|v| v
                    .position
                    .into_iter()
                    .chain(v.normal)
                    .chain(v.uv.into_iter().flatten()))
                .all(f32::is_finite),
        "non-finite free sail input"
    );
    let direction = local_direction(p)?;
    let dv = mul(direction, p.height * 0.25);
    let mut next = s.clone();
    let mut grid = vertices.to_vec();
    let mut point = 0;
    let mut used = 0;
    for ix in 0..if tri { 5 } else { 4 } {
        let fraction = if tri {
            ix as f32 * 0.25
        } else {
            ix as f32 / 3.0
        };
        let mut target = add(
            m.hard_points[0],
            mul(sub(m.hard_points[1], m.hard_points[0]), fraction),
        );
        let mut index = if tri { ix * (ix * 8 + 2) } else { 17 * ix * 4 };
        for iy in 0..if tri { ix + 1 } else { 5 } {
            let delta = sub(target, next.points[point]);
            let factor = if iy == 0 {
                1.0
            } else {
                let sample = draws.values[used] as f32 / draws.maximum as f32;
                used += 1;
                (p.addition_min + sample * p.addition_random) / libm::sqrtf(square(delta))
            };
            next.points[point] = add(next.points[point], mul(delta, factor));
            grid[index].position = next.points[point];
            point += 1;
            index += 4;
            target = add(target, dv);
        }
    }
    let mut min = next.points[0];
    let mut max = min;
    for p in &next.points[1..if tri { 15 } else { 20 }] {
        for i in 0..3 {
            if p[i] < min[i] {
                min[i] = p[i];
            }
            if p[i] > max[i] {
                max[i] = p[i];
            }
        }
    }
    next.box_center = mul(add(min, max), 0.5);
    next.box_size = sub(max, next.box_center);
    if !next
        .points
        .into_iter()
        .flatten()
        .chain(next.box_center)
        .chain(next.box_size)
        .chain(grid.iter().flat_map(|v| v.position))
        .all(f32::is_finite)
    {
        return Err(NativeNonFinite.into());
    }
    *s = next;
    vertices.copy_from_slice(&grid);
    Ok(used)
}
#[cfg(test)]
mod tests;
