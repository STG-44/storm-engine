//! Retained DoSRollSail/DoTRollSail and the rolled goWave state/bounds path.
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
fn div(a: V, b: f32) -> V {
    let r = 1.0 / f64::from(b);
    a.map(|x| (f64::from(x) * r) as f32)
}
#[derive(Clone, Copy, Debug)]
pub struct Parameters {
    pub z: f32,
    pub y: f32,
    pub wind_delta: f32,
    pub triangle_multiplier: f32,
    pub square_form: [f32; 13],
    pub triangle_form: [f32; 5],
    pub turning: bool,
    pub ties: [bool; 4],
    pub height: f32,
}
impl Default for Parameters {
    fn default() -> Self {
        Self {
            z: 0.01,
            y: 0.04,
            wind_delta: 0.001,
            triangle_multiplier: 2.0,
            square_form: [
                0.2, 0.8, 1.0, 0.8, 0.4, 1.0, 1.3, 1.0, 0.4, 0.8, 1.0, 0.8, 0.2,
            ],
            triangle_form: [0.1, 0.6, 0.3, 0.8, 0.2],
            turning: false,
            ties: [false; 4],
            height: 1.0,
        }
    }
}
/// Native rolled grid (5 rows / 13 columns). Unwritten vertices, normals/UVs,
/// sample points, horizontal phase and SumWind remain unchanged.
pub fn advance(
    s: &mut State,
    m: &SailMotion,
    vertices: &mut [Vertex],
    p: Parameters,
    table: &[f32],
    increment: u32,
) -> Result<()> {
    let tri = m.kind == Kind::Triangle;
    ensure!(m.rolling, "attached sail needs its native wave processor");
    ensure!(
        vertices.len() == if tri { 153 } else { 221 },
        "invalid rolled sail vertex count"
    );
    ensure!(
        !table.is_empty()
            && table.len() <= 65536
            && s.vertical < table.len()
            && s.horizontal < table.len(),
        "invalid rolled wind table or phase"
    );
    ensure!(
        m.geometry
            .into_iter()
            .flatten()
            .chain(m.hard_points.into_iter().flatten())
            .chain(table.iter().copied())
            .chain(p.square_form)
            .chain(p.triangle_form)
            .chain([
                p.z,
                p.y,
                p.wind_delta,
                p.triangle_multiplier,
                p.height,
                s.sum_wind,
                s.maximum_sum_wind
            ])
            .chain(s.points.into_iter().flatten())
            .chain(s.rope_points.into_iter().flatten())
            .all(f32::is_finite),
        "non-finite rolled sail input"
    );
    let mut next = s.clone();
    let mut grid = vertices.to_vec();
    let h = m.hard_points;
    let mut position = h[0];
    let step = if tri {
        mul(sub(h[1], h[0]), 0.25)
    } else {
        div(sub(h[1], h[0]), 12.0)
    };
    let mut dz = p.z * p.height;
    let mut dy = p.y * p.height;
    if tri && p.turning {
        dz *= p.triangle_multiplier;
        dy *= p.triangle_multiplier;
    }
    let mut d1 = mul(m.geometry[5], 0.5 * dz);
    d1[1] -= dy;
    let d2 = [0.0, -dy * 1.2, 0.0];
    let mut d3 = mul(m.geometry[5], -dz);
    d3[1] -= dy;
    let d4 = mul(m.geometry[5], -0.5 * dz);
    let incr = increment as usize % table.len();
    if tri {
        for (index, d) in [
            (6, [0.0; 3]),
            (7, mul(d1, p.triangle_form[0])),
            (8, mul(d3, p.triangle_form[0])),
            (9, mul(d4, p.triangle_form[0])),
        ] {
            grid[index].position = if index == 6 {
                position
            } else {
                add(position, d)
            };
        }
        position = add(position, step);
        let mut index = 10;
        for i in 1..5 {
            let wind = 1.0 - table[next.vertical] * p.wind_delta;
            grid[index].position = position;
            for d in [d1, d2, d3, d4] {
                index += i;
                grid[index].position = add(position, mul(d, p.triangle_form[i] * wind));
            }
            next.vertical = (next.vertical + incr) % table.len();
            position = add(position, step);
            index += i * 12 + 10;
        }
        if p.ties[2] {
            next.rope_points[2] = h[1];
        }
    } else {
        for i in 0..13 {
            let wind = 1.0 - table[next.vertical] * p.wind_delta;
            grid[i * 17].position = position;
            for (offset, d) in [(4, d1), (8, d2), (12, d3), (16, d4)] {
                grid[i * 17 + offset].position = add(position, mul(d, p.square_form[i] * wind));
            }
            next.vertical = (next.vertical + incr) % table.len();
            position = add(position, step);
        }
        if p.ties[2] {
            next.rope_points[2] = h[0];
        }
        if p.ties[3] {
            next.rope_points[3] = h[1];
        }
    }
    let mut min = next.points[0];
    let mut max = min;
    for point in &next.points[1..if tri { 15 } else { 20 }] {
        for i in 0..3 {
            if point[i] < min[i] {
                min[i] = point[i];
            }
            if point[i] > max[i] {
                max[i] = point[i];
            }
        }
    }
    next.box_center = mul(add(min, max), 0.5);
    next.box_size = sub(max, next.box_center);
    ensure!(
        grid.iter()
            .flat_map(|v| v
                .position
                .into_iter()
                .chain(v.normal)
                .chain(v.uv.into_iter().flatten()))
            .chain(next.rope_points.into_iter().flatten())
            .chain(next.box_center)
            .chain(next.box_size)
            .all(f32::is_finite),
        "rolled sail overflow"
    );
    *s = next;
    vertices.copy_from_slice(&grid);
    Ok(())
}
#[cfg(test)]
mod tests;
