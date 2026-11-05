//! Retained rope-tie arithmetic, SetTurnLimits and TurnSail's constrained path.
//! Registration order, matrix ownership and geometry parameters are caller ports.
use crate::{
    sail_drive::Kind,
    sail_geometry::{self, Parameters, Sphere},
    sail_motion::{NodePort, SailMotion},
};
use anyhow::{Result, ensure};
pub type V = [f32; 3];
fn add(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] + b[i])
}
fn sub(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] - b[i])
}
fn mul(a: V, b: f32) -> V {
    a.map(|v| v * b)
}
fn square(v: V) -> f32 {
    v[0] * v[0] + v[1] * v[1] + v[2] * v[2]
}
fn normalized(v: V) -> V {
    let n = f64::from(square(v));
    if n == 0.0 {
        return [0.0; 3];
    }
    let r = 1.0 / libm::sqrt(n);
    v.map(|x| (f64::from(x) * r) as f32)
}
fn div(v: V, x: f32) -> V {
    let r = 1.0 / f64::from(x);
    v.map(|v| (f64::from(v) * r) as f32)
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record {
    pub number: i32,
    pub tie: usize,
    pub r1: f32,
    pub r2: f32,
    pub b: V,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Links {
    pub records: [Option<Record>; 2],
}
#[derive(Clone, Copy, Debug)]
pub struct End {
    pub point: V,
    pub matrix: [f32; 16],
}
impl End {
    pub fn world(self) -> Result<V> {
        ensure!(
            self.point
                .into_iter()
                .chain(self.matrix)
                .all(f32::is_finite),
            "invalid rope frame"
        );
        let p = transform(self);
        ensure!(p.into_iter().all(f32::is_finite), "rope frame overflow");
        Ok(p)
    }
    pub fn local(self, point: V) -> Result<V> {
        ensure!(
            point.into_iter().chain(self.matrix).all(f32::is_finite),
            "invalid rope frame"
        );
        let p = inverse_point(self.matrix, point);
        ensure!(p.into_iter().all(f32::is_finite), "rope frame overflow");
        Ok(p)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Rope {
    pub number: i32,
    pub model: u64,
    pub begin: End,
    pub end: End,
}
fn transform(e: End) -> V {
    let m = e.matrix;
    let v = e.point;
    std::array::from_fn(|i| m[i] * v[0] + m[i + 4] * v[1] + m[i + 8] * v[2] + m[i + 12])
}
fn inverse_point(m: [f32; 16], v: V) -> V {
    std::array::from_fn(|i| {
        (v[0] - m[12]) * m[i * 4] + (v[1] - m[13]) * m[i * 4 + 1] + (v[2] - m[14]) * m[i * 4 + 2]
    })
}
/// ROPE::GetEndPoint: first number/model match; a miss retains the output.
/// Negative numbers reverse the selected endpoint. MulToInv uses a transpose,
/// including for non-orthonormal matrices, exactly as the native method.
pub fn endpoint(ropes: &[Rope], number: i32, model: u64, out: Option<&mut V>) -> Result<bool> {
    let Some(out) = out else { return Ok(false) };
    let key = number
        .checked_abs()
        .ok_or_else(|| anyhow::anyhow!("invalid native rope number"))?;
    let Some(r) = ropes.iter().find(|r| r.number == key && r.model == model) else {
        return Ok(false);
    };
    ensure!(
        r.begin
            .point
            .into_iter()
            .chain(r.end.point)
            .chain(r.begin.matrix)
            .chain(r.end.matrix)
            .all(f32::is_finite),
        "non-finite rope endpoint input"
    );
    let v = if number < 0 {
        inverse_point(r.end.matrix, transform(r.begin))
    } else {
        inverse_point(r.begin.matrix, transform(r.end))
    };
    ensure!(v.into_iter().all(f32::is_finite), "rope endpoint overflow");
    *out = v;
    Ok(true)
}
/// Numeric MSG_SAIL_ROPE_TIE path after node/group matching. A duplicate tie can
/// consume the second record; later records are ignored while ties still update.
pub fn register(
    m: &mut SailMotion,
    links: &mut Links,
    turning: bool,
    position: V,
    number: i32,
    opposite: Option<V>,
) -> Result<usize> {
    ensure!(
        m.hard_points
            .into_iter()
            .flatten()
            .chain(position)
            .chain(opposite.into_iter().flatten())
            .all(f32::is_finite),
        "non-finite rope tie input"
    );
    let mut closest = 0;
    let mut distance = 1000.0;
    for i in 0..if m.kind == Kind::Triangle { 3 } else { 4 } {
        let d = square(sub(m.hard_points[i], position));
        if d < distance {
            distance = d;
            closest = i;
        }
    }
    let mut next = links.clone();
    if turning
        && closest != 0
        && let Some(end) = opposite
        && let Some(slot) = next.records.iter_mut().find(|r| r.is_none())
    {
        let begin = m.hard_points[if closest == 3 { 1 } else { 0 }];
        let r1 = libm::sqrtf(square(sub(position, begin)));
        let r2 = libm::sqrtf(square(sub(position, end)));
        let b = if m.kind == Kind::Triangle {
            [0.0; 3]
        } else {
            sub(
                position,
                mul(normalized(add(m.geometry[0], mul(m.geometry[1], 17.0))), r1),
            )
        };
        ensure!(
            [r1, r2].into_iter().chain(b).all(f32::is_finite),
            "rope registration overflow"
        );
        *slot = Some(Record {
            number,
            tie: closest,
            r1,
            r2,
            b,
        });
    }
    m.ties[closest] = Some(position);
    *links = next;
    Ok(closest)
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub endpoint: V,
    pub sphere_radius: f32,
    pub parent_width: f32,
    pub deep_h: f32,
    pub deep_z: f32,
    pub triangle_dependence: f32,
    pub max_angle: f32,
}
/// None represents the absent entity/pointer guards before the native search.
pub fn set_limits(
    m: &mut SailMotion,
    links: &Links,
    turning: bool,
    input: Option<Limits>,
) -> Result<()> {
    if m.kind != Kind::Triangle || !turning || links.records[0].is_none() {
        return Ok(());
    }
    let Some(p) = input else { return Ok(()) };
    ensure!(
        m.hard_points[0]
            .into_iter()
            .chain(p.endpoint)
            .chain([
                p.sphere_radius,
                p.parent_width,
                p.deep_h,
                p.deep_z,
                p.triangle_dependence,
                p.max_angle,
                m.minimum_angle,
                m.maximum_angle
            ])
            .all(f32::is_finite)
            && (-16.0..=16.0).contains(&p.max_angle),
        "invalid rope turn-limit input"
    );
    let kx = p.endpoint[2];
    let ky = p.endpoint[0];
    let kc = 0.0 * kx - 0.0 * ky;
    let x0 = m.hard_points[0][0];
    let y0 = m.hard_points[0][2];
    let side = (x0 * kx - y0 * ky + kc) >= 0.0;
    let adding = libm::sqrtf(kx * kx + ky * ky)
        * (p.triangle_dependence * p.sphere_radius * 1.3 + p.parent_width + p.deep_h + p.deep_z);
    ensure!(adding.is_finite(), "rope limit overflow");
    let (mut min, mut max) = (-1.0, -1.0);
    let mut angle = p.max_angle;
    while angle > 0.0 {
        let c = libm::cosf(angle);
        let s = libm::sinf(angle);
        if min < 0.0 {
            let x = x0 * c + y0 * s;
            let y = -x0 * s + y0 * c;
            let v = x * kx - y * ky + kc;
            if if side { v > adding } else { v < -adding } {
                min = angle;
            }
        }
        if max < 0.0 {
            let x = x0 * c - y0 * s;
            let y = x0 * s + y0 * c;
            let v = x * kx - y * ky + kc;
            if if side { v > adding } else { v < -adding } {
                max = angle;
            }
        }
        angle -= 0.01;
    }
    if min >= 0.0 {
        m.minimum_angle = min;
    }
    if max >= 0.0 {
        m.maximum_angle = max;
    }
    Ok(())
}
#[derive(Debug)]
pub struct NativeNonFinite;
impl std::fmt::Display for NativeNonFinite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native rope constraint produced non-finite coordinates")
    }
}
impl std::error::Error for NativeNonFinite {}
#[derive(Clone, Copy, Debug, Default)]
pub struct Turn {
    pub moved: bool,
    pub rebuilt: Option<Sphere>,
}
#[derive(Clone, Copy, Debug)]
pub struct Turning {
    pub wind_x: f32,
    pub step: f32,
    pub discrete: f32,
    pub geometry: Parameters,
}
/// Complete TurnSail dispatch. The endpoint provider sees the updated node.
/// Checked failures retain motion and node; external provider observations remain.
pub fn turn<N: NodePort + Clone>(
    m: &mut SailMotion,
    links: &Links,
    input: Turning,
    node: &mut N,
    mut provider: Option<impl FnMut(&N, &SailMotion, i32, &mut V) -> Result<()>>,
) -> Result<Turn> {
    let mut next = m.clone();
    let mut yard = node.clone();
    if !next.turn_node(input.wind_x, input.step, input.discrete, &mut yard)? {
        return Ok(Turn::default());
    }
    let mut rebuilt = None;
    if next.rolling {
        for (slot, point) in [(2, 0), (3, 1)] {
            if next.ties[slot].is_some() {
                next.ties[slot] = Some(next.hard_points[point]);
            }
        }
    } else if let Some(provider) = &mut provider {
        let mut change = false;
        for r in links.records.iter().flatten() {
            ensure!(
                (1..if next.kind == Kind::Triangle { 3 } else { 4 }).contains(&r.tie)
                    && next.ties[r.tie].is_some(),
                "invalid rope tie record"
            );
            ensure!(
                [r.r1, r.r2].into_iter().all(f32::is_finite)
                    && (next.kind == Kind::Triangle || r.b.into_iter().all(f32::is_finite)),
                "non-finite rope record"
            );
            let mut end = [0.0; 3];
            provider(&yard, &next, r.number, &mut end)?;
            ensure!(
                end.into_iter().all(f32::is_finite),
                "non-finite endpoint provider"
            );
            let mut point = next.hard_points[r.tie];
            let begin = next.hard_points[if r.tie == 3 { 1 } else { 0 }];
            if next.kind != Kind::Triangle {
                point = div(add(mul(end, r.r1), mul(r.b, r.r2)), r.r1 + r.r2);
            } else {
                for i in [0, 2] {
                    point[i] = (end[i] * r.r1 + begin[i] * r.r2) / (r.r1 + r.r2);
                }
            }
            if !point.into_iter().all(f32::is_finite) {
                return Err(NativeNonFinite.into());
            }
            next.ties[r.tie] = Some(point);
            next.hard_points[r.tie] = point;
            change = true;
        }
        if change {
            rebuilt = Some(sail_geometry::rebuild(&mut next, input.geometry)?);
        }
    }
    *m = next;
    *node = yard;
    Ok(Turn {
        moved: true,
        rebuilt,
    })
}
#[cfg(test)]
mod tests;
