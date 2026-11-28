//! Retained emitter transform state and Quaternion-based BlendMatrix arithmetic.
use super::particle_motion::Transform;
use anyhow::{Result, ensure};

impl Transform {
    pub fn normal(self, v: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|i| self.0[0][i] * v[0] + self.0[1][i] * v[1] + self.0[2][i] * v[2])
    }
    pub fn from_euler(angles: [f32; 3], position: [f32; 3]) -> Result<Self> {
        ensure!(
            angles.into_iter().chain(position).all(f32::is_finite),
            "invalid emitter transform"
        );
        let [x, y, z] = angles;
        let (sx, cx) = x.sin_cos();
        let (sy, cy) = y.sin_cos();
        let (sz, cz) = z.sin_cos();
        Ok(Self([
            [cz * cy + sz * sx * sy, sz * cx, cz * -sy + sz * sx * cy],
            [-sz * cy + cz * sx * sy, cz * cx, -sz * -sy + cz * sx * cy],
            [cx * sy, -sx, cx * cy],
            position,
        ]))
    }
    fn quaternion(self) -> [f32; 4] {
        let m = self.0;
        let tr = m[0][0] + m[1][1] + m[2][2];
        let mut q = [0.0; 4];
        if tr > 0.0 {
            // C++ sqrt(float) is evaluated at float precision before promotion.
            let mut s = f64::from((tr + 1.0).sqrt());
            q[3] = (s * 0.5) as f32;
            s = 0.5 / s;
            q[0] = (f64::from(m[1][2] - m[2][1]) * s) as f32;
            q[1] = (f64::from(m[2][0] - m[0][2]) * s) as f32;
            q[2] = (f64::from(m[0][1] - m[1][0]) * s) as f32;
        } else {
            let (i, j, k) = if m[1][1] > m[0][0] {
                if m[2][2] > m[1][1] {
                    (2, 0, 1)
                } else {
                    (1, 2, 0)
                }
            } else if m[2][2] > m[0][0] {
                (2, 0, 1)
            } else {
                (0, 1, 2)
            };
            let mut s = (f64::from(m[i][i] - (m[j][j] + m[k][k])) + 1.0).sqrt();
            q[i] = (s * 0.5) as f32;
            if s != 0.0 {
                s = 0.5 / s;
            }
            q[j] = (f64::from(m[i][j] + m[j][i]) * s) as f32;
            q[k] = (f64::from(m[i][k] + m[k][i]) * s) as f32;
            q[3] = (f64::from(m[j][k] - m[k][j]) * s) as f32;
        }
        q
    }
    pub fn blend(self, other: Self, k: f32) -> Result<Self> {
        ensure!(
            self.0
                .into_iter()
                .flatten()
                .chain(other.0.into_iter().flatten())
                .chain([k])
                .all(f32::is_finite)
                && (0.0..=1.0).contains(&k),
            "invalid emitter matrix blend"
        );
        let a = self.quaternion();
        let b = other.quaternion();
        let mut cosine = f64::from(a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]);
        let mut sign = 1.0;
        if cosine < 0.0 {
            cosine = -cosine;
            sign = -1.0;
        }
        cosine = cosine.min(1.0);
        let (mut k0, k1) = if 1.0 - cosine > 0.1 {
            let omega = cosine.acos();
            let inv = 1.0 / omega.sin();
            (
                (omega * (1.0 - f64::from(k))).sin() * inv,
                (omega * f64::from(k)).sin() * inv,
            )
        } else {
            (1.0 - f64::from(k), f64::from(k))
        };
        k0 *= sign;
        let [x, y, z, w] = std::array::from_fn::<_, 4, _>(|i| {
            (f64::from(a[i]) * k0 + f64::from(b[i]) * k1) as f32
        });
        let (xx, xy, xz) = (x * x * 2.0, x * y * 2.0, x * z * 2.0);
        let (yy, yz, zz) = (y * y * 2.0, y * z * 2.0, z * z * 2.0);
        let (wx, wy, wz) = (w * x * 2.0, w * y * 2.0, w * z * 2.0);
        let result = Self([
            [1.0 - (yy + zz), xy + wz, xz - wy],
            [xy - wz, 1.0 - (xx + zz), yz + wx],
            [xz + wy, yz - wx, 1.0 - (xx + yy)],
            std::array::from_fn(|i| self.0[3][i] + (other.0[3][i] - self.0[3][i]) * k),
        ]);
        ensure!(
            result.0.into_iter().flatten().all(f32::is_finite),
            "emitter blend overflow"
        );
        Ok(result)
    }
}
#[derive(Clone, Debug, Default)]
pub struct EmitterTransform {
    pub old: Option<Transform>,
    pub new: Transform,
    old_initialized: bool,
}
impl EmitterTransform {
    pub fn set(&mut self, value: Transform) {
        if !self.old_initialized {
            self.old = Some(value);
            self.old_initialized = true;
        }
        self.new = value;
    }
    pub fn teleport(&mut self, value: Transform) {
        self.old = Some(value);
        self.new = value;
    }
    pub fn birth(&self, k: f32, profile_position: [f32; 3]) -> Result<Transform> {
        let mut value = self.old.unwrap_or_default().blend(self.new, k)?;
        value.0[3] = value.point(profile_position);
        ensure!(
            value.0[3].into_iter().all(f32::is_finite),
            "emitter position overflow"
        );
        Ok(value)
    }
    pub fn finish(&mut self) {
        self.old = Some(self.new);
    }
}
#[cfg(test)]
mod tests;
