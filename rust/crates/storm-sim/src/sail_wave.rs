//! Retained attached GoVWave/GoTWave and their goWave phase/flex/bounds path.
//! The wind table, local wind, LOD and rope ties are explicit inputs. Free and
//! fully rolled sails are separate processors, not approximated by this module.
use crate::{
    sail_drive::Kind,
    sail_geometry::{COLUMNS, Parameters, ROWS, Sphere, Vertex},
    sail_motion::SailMotion,
};
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub square_dependence: f32,
    pub triangle_dependence: f32,
    pub wind_adding: f32,
    pub triangle_hole_flex: f32,
    pub flex_speed: f32,
    pub square_increment: u32,
    pub triangle_increment: u32,
    pub square_add: u32,
    pub triangle_add: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            square_dependence: 0.05,
            triangle_dependence: 0.5,
            wind_adding: 0.3,
            triangle_hole_flex: 0.01,
            flex_speed: 0.001,
            square_increment: 6,
            triangle_increment: 3,
            square_add: 3,
            triangle_add: 3,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Input {
    pub parameters: Parameters,
    pub sphere: Sphere,
    pub height: f32,
    pub local_wind: V,
    pub wind_base: f32,
    pub limit_point: bool,
    pub rows: u32,
    pub columns: u32,
    pub ties: [bool; 4],
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub vertical: usize,
    pub horizontal: usize,
    pub wind_up: bool,
    pub sum_wind: f32,
    pub maximum_sum_wind: f32,
    pub points: [V; 20],
    pub rope_points: [V; 4],
    pub box_center: V,
    pub box_size: V,
}
struct Frame<'a> {
    motion: &'a SailMotion,
    input: Input,
    settings: Settings,
    table: &'a [f32],
    increment: usize,
    phase_add: usize,
}
impl Frame<'_> {
    fn triangle(&self, s: &mut State, vertices: &mut [Vertex]) {
        let h = self.motion.hard_points;
        let g = self.motion.geometry;
        let p = self.input.parameters;
        let w = self.input.local_wind;
        let k = w[0] * g[5][0] + w[1] * g[5][1] + w[2] * g[5][2];
        let mut center_flex = mul(mul(g[5], k), if k < 0.0 { p.deep_h } else { p.deep_z });
        let mut start = h[0];
        let start_delta = div(sub(h[1], h[0]), (ROWS - 1) as f32);
        let mut amplitude = self.settings.triangle_dependence
            * self.input.sphere.radius
            * (self.input.wind_base + 0.3)
            / (ROWS - 1) as f32
            / (ROWS - 1) as f32;
        let retained = 1.0 - p.holes as f32 * self.settings.triangle_hole_flex;
        amplitude *= retained;
        center_flex = mul(center_flex, retained);
        if self.input.ties[0] {
            start = add(
                start,
                mul(
                    mul(mul(g[5], amplitude), self.table[s.vertical]),
                    ROWS as f32,
                ),
            );
            s.rope_points[0] = start;
        }
        let mut sv = 0;
        let mut ix = 0;
        let mut last = [0.0; 3];
        let mut last_dv = [0.0; 3];
        while ix < ROWS {
            let mut position = start;
            let mut dv = add(
                g[0],
                mul(add(g[3], mul(g[4], (ix + 1) as f32 * 0.5)), ix as f32),
            );
            let wind_add = add(
                mul(g[5], self.table[s.vertical] * amplitude * ix as f32),
                center_flex,
            );
            dv = add(dv, wind_add);
            let ddv = div(
                mul(
                    wind_add.map(|x| -x),
                    if self.input.ties[2] { 1.8 } else { 2.0 },
                ),
                ROWS as f32,
            );
            if ix == ROWS - 1 && self.input.ties[1] {
                s.rope_points[1] = position;
            }
            let first = ix * (ix + 1) / 2;
            for iy in 0..=ix {
                vertices[first + iy].position = position;
                if ix % 4 == 0 && iy % 4 == 0 {
                    s.points[sv] = position;
                    sv += 1;
                }
                dv = add(dv, ddv);
                position = add(position, dv);
            }
            last = position;
            last_dv = dv;
            s.vertical = (s.vertical + self.increment) % self.table.len();
            match self.input.rows {
                17 => {
                    ix += 1;
                    start = add(start, start_delta);
                }
                13 => {
                    ix += 1;
                    start = add(start, start_delta);
                    if ix & 3 == 2 {
                        ix += 1;
                        start = add(start, start_delta);
                    }
                }
                9 => {
                    ix += 2;
                    start = add(start, add(start_delta, start_delta));
                }
                5 => {
                    ix += 4;
                    start = add(start, mul(start_delta, 4.0));
                }
                // Preserve the source's four-step start increment at row LOD 3.
                3 => {
                    ix += 8;
                    start = add(start, mul(start_delta, 4.0));
                }
                _ => unreachable!(),
            }
        }
        if self.input.ties[2] {
            s.rope_points[2] = sub(last, last_dv);
        }
        s.horizontal = (s.horizontal + self.phase_add) % self.table.len();
        s.vertical = s.horizontal;
    }
    fn square(&self, s: &mut State, vertices: &mut [Vertex]) {
        let h = self.motion.hard_points;
        let g = self.motion.geometry;
        let p = self.input.parameters;
        let base = if self.input.wind_base > 1.0 {
            1.0
        } else {
            self.input.wind_base
        };
        let main = base * 0.5 + 0.1;
        let trapezoid = self.motion.kind == Kind::Trapezoid;
        let wind = if trapezoid {
            [main, 0.0, main * 0.2]
        } else {
            [main * 0.2, 0.0, main]
        };
        let amplitude = if self.input.limit_point {
            (p.deep_h + p.deep_z) * 0.15
        } else {
            self.settings.square_dependence * self.input.height * (base + self.settings.wind_adding)
                / (self.settings.wind_adding + 1.0)
                / (COLUMNS / 2 + 2) as f32
        };
        let mut flex =
            0.2 * (1.0 - base * (1.0 + self.input.local_wind[if trapezoid { 2 } else { 0 }]) * 0.5);
        if let Some(t) = self.motion.transition {
            flex *= t.delta;
        }
        let mut down = [0.0, -flex, -flex * (p.deep_z + p.deep_h)];
        let mut dh = mul(div(sub(h[1], h[3]), (ROWS + 1) as f32), p.deep_vh);
        let ddh = add(g[4], div(mul(dh, 2.0), (COLUMNS - 1) as f32));
        dh = sub(g[3], dh);
        let mut start = h[0];
        let mut start_delta = div(sub(h[1], h[0]), (COLUMNS - 1) as f32);
        if self.input.columns == 7 {
            start_delta = mul(start_delta, 2.0);
        } else if self.input.columns == 4 {
            start_delta = mul(start_delta, 4.0);
        }
        down[1] -= s.sum_wind * self.motion.transition.map_or(1.0, |t| t.delta);
        if self.input.ties[0] {
            s.rope_points[0] = start;
        }
        let mut sv = 0;
        let mut ix = 0;
        let mut trigger = false;
        let mut last = [0.0; 3];
        let mut last_dv = [0.0; 3];
        while ix < COLUMNS {
            let mut k = if ix <= COLUMNS / 2 {
                ix
            } else {
                COLUMNS - ix - 1
            } as f32
                / COLUMNS as f32;
            if self.input.ties[2] {
                k += 0.2;
            }
            let wind_add = mul(
                wind,
                self.table[s.vertical] * amplitude * (k + 1.0 / COLUMNS as f32),
            );
            let mut position = start;
            let mut dv = add(
                add(
                    add(g[0], mul(add(dh, mul(ddh, ix as f32 * 0.5)), ix as f32)),
                    wind_add,
                ),
                down,
            );
            let ddv = sub(
                sub(g[1], div(mul(wind_add, 2.0 - k), ROWS as f32)),
                div(mul(down, 2.0), ROWS as f32),
            );
            for iy in 0..ROWS {
                vertices[ix * ROWS + iy].position = position;
                if ix % 4 == 0 && iy % 4 == 0 {
                    s.points[sv] = position;
                    sv += 1;
                }
                dv = add(dv, ddv);
                position = add(position, dv);
            }
            if ix == 0 && self.input.ties[2] {
                s.rope_points[2] = sub(position, dv);
            }
            last = position;
            last_dv = dv;
            if trigger {
                s.vertical = (s.vertical + self.increment) % self.table.len();
            }
            trigger = !trigger;
            start = add(start, start_delta);
            match self.input.columns {
                13 => ix += 1,
                10 => {
                    ix += 1;
                    if ix & 3 == 2 {
                        ix += 1;
                        start = add(start, start_delta);
                    }
                }
                7 => ix += 2,
                4 => ix += 4,
                _ => unreachable!(),
            }
        }
        if self.input.ties[3] {
            s.rope_points[3] = sub(last, last_dv);
        }
    }
}
fn values(s: &State) -> impl Iterator<Item = f32> + '_ {
    [s.sum_wind, s.maximum_sum_wind]
        .into_iter()
        .chain(s.points.into_iter().flatten())
        .chain(s.rope_points.into_iter().flatten())
        .chain(s.box_center)
        .chain(s.box_size)
}
/// Apply one native attached-wave frame atomically. Vertices skipped by the
/// original LOD loops retain their previous values, as do normals and all UVs.
pub fn advance(
    s: &mut State,
    motion: &SailMotion,
    vertices: &mut [Vertex],
    input: Input,
    table: &[f32],
    settings: Settings,
    delta_ms: u32,
) -> Result<()> {
    let triangle = motion.kind == Kind::Triangle;
    ensure!(
        !motion.rolling,
        "rolled sail needs its native rolled-vertex processor"
    );
    ensure!(
        vertices.len()
            == if triangle {
                ROWS * (ROWS + 1) / 2
            } else {
                ROWS * COLUMNS
            },
        "invalid wave vertex count"
    );
    ensure!(
        matches!(input.rows, 17 | 13 | 9 | 5 | 3) && matches!(input.columns, 13 | 10 | 7 | 4),
        "invalid native sail LOD"
    );
    ensure!(
        !table.is_empty()
            && table.len() <= 65536
            && s.vertical < table.len()
            && s.horizontal < table.len(),
        "invalid sail wind table or phase"
    );
    let p = input.parameters;
    ensure!(
        table
            .iter()
            .copied()
            .chain(values(s))
            .chain(motion.geometry.into_iter().flatten())
            .chain(motion.hard_points.into_iter().flatten())
            .chain(input.local_wind)
            .chain([
                input.height,
                input.wind_base,
                input.sphere.radius,
                p.deep_z,
                p.deep_h,
                p.deep_vh,
                settings.square_dependence,
                settings.triangle_dependence,
                settings.wind_adding,
                settings.triangle_hole_flex,
                settings.flex_speed
            ])
            .all(f32::is_finite),
        "non-finite sail wave input"
    );
    ensure!(
        motion.transition.is_none_or(|t| t.delta.is_finite()),
        "invalid wave rolling delta"
    );
    ensure!(settings.wind_adding != -1.0, "singular sail wind amplitude");
    let phase = if triangle {
        settings.triangle_add
    } else {
        settings.square_add
    } as f32
        * delta_ms as f32
        * 0.02;
    ensure!(
        phase.is_finite() && (0.0..2147483648.0).contains(&phase),
        "sail phase increment out of native int range"
    );
    let f = Frame {
        motion,
        input,
        settings,
        table,
        increment: if triangle {
            settings.triangle_increment
        } else {
            settings.square_increment
        } as usize
            % table.len(),
        phase_add: phase as usize % table.len(),
    };
    let mut next = s.clone();
    let mut updated = vertices.to_vec();
    if triangle {
        f.triangle(&mut next, &mut updated);
    } else {
        f.square(&mut next, &mut updated);
        next.horizontal = (next.horizontal + f.phase_add) % table.len();
        next.vertical = next.horizontal;
        if motion.transition.is_none() {
            let delta = delta_ms as f32 * settings.flex_speed;
            if next.wind_up {
                next.sum_wind -= delta;
                if next.sum_wind < -next.maximum_sum_wind {
                    next.sum_wind = -next.maximum_sum_wind;
                    next.wind_up = false;
                }
            } else {
                next.sum_wind += delta;
                if next.sum_wind > next.maximum_sum_wind {
                    next.sum_wind = next.maximum_sum_wind;
                    next.wind_up = true;
                }
            }
        }
    }
    let n = if triangle { 15 } else { 20 };
    let mut min = next.points[0];
    let mut max = min;
    for p in &next.points[1..n] {
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
    ensure!(
        values(&next)
            .chain(updated.iter().flat_map(|v| {
                v.position
                    .into_iter()
                    .chain(v.normal)
                    .chain(v.uv.into_iter().flatten())
            }))
            .all(f32::is_finite),
        "sail wave overflow"
    );
    *s = next;
    vertices.copy_from_slice(&updated);
    Ok(())
}

#[cfg(test)]
mod tests;
