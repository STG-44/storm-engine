//! Retained billboard Process / SolvePhysic arithmetic, independent of graph storage.
use anyhow::{Result, ensure};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform(pub [[f32; 3]; 4]);
impl Default for Transform {
    fn default() -> Self {
        Self([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0; 3]])
    }
}
impl Transform {
    pub fn point(self, v: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|i| {
            self.0[0][i] * v[0] + self.0[1][i] * v[1] + self.0[2][i] * v[2] + self.0[3][i]
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    Drag,
    Gravity,
    SpinDrag,
    TrackX,
    TrackY,
    TrackZ,
    Blend,
}
pub trait MotionPort {
    fn sample(&mut self, curve: Curve, age: f32, lifetime: f32) -> f32;
}
#[derive(Clone, Debug, PartialEq)]
pub struct Billboard {
    pub age: f32,
    pub lifetime: f32,
    pub mass: f32,
    pub spin: f32,
    pub angle: f32,
    pub render_angle: f32,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub force: [f32; 3],
    pub render_position: [f32; 3],
    pub old_render_position: [f32; 3],
    pub transform: Transform,
}
impl Billboard {
    pub fn new(
        position: [f32; 3],
        velocity: [f32; 3],
        lifetime: f32,
        mass: f32,
        spin: f32,
        transform: Transform,
    ) -> Self {
        Self {
            age: 0.0,
            lifetime,
            mass,
            spin,
            angle: 0.0,
            render_angle: 0.0,
            position,
            velocity,
            force: [0.0; 3],
            render_position: position,
            old_render_position: position,
            transform,
        }
    }
    fn finite(&self) -> bool {
        [
            self.age,
            self.lifetime,
            self.mass,
            self.spin,
            self.angle,
            self.render_angle,
        ]
        .into_iter()
        .chain(self.position)
        .chain(self.velocity)
        .chain(self.force)
        .chain(self.render_position)
        .chain(self.old_render_position)
        .chain(self.transform.0.into_iter().flatten())
        .all(f32::is_finite)
    }
    /// Returns false after strict expiry. On error the body is unchanged; graph
    /// callbacks already made are not rolled back. Finite negative deltas follow C++.
    pub fn process(&mut self, delta: f32, port: &mut impl MotionPort) -> Result<bool> {
        ensure!(
            delta.is_finite() && self.finite(),
            "invalid particle motion input"
        );
        let mut next = self.clone();
        next.age += delta;
        ensure!(next.age.is_finite(), "particle age overflow");
        if next.age > next.lifetime {
            *self = next;
            return Ok(false);
        }
        let mut sample = |curve| -> Result<f32> {
            let v = port.sample(curve, next.age, next.lifetime);
            ensure!(v.is_finite(), "non-finite particle motion graph");
            Ok(v)
        };
        let drag = (1.0 - sample(Curve::Drag)? * 0.01).clamp(0.0, 1.0);
        let gravity = (sample(Curve::Gravity)? * 0.01).clamp(0.0, 1.0);
        next.force[1] += -9.8 * next.mass * gravity;
        let mass = next.mass.abs();
        // Native Vector::operator/=(float) uses a DOUBLE reciprocal/product.
        let acceleration = if mass != 0.0 {
            let reciprocal = 1.0 / f64::from(mass);
            next.force.map(|f| (f64::from(f) * reciprocal) as f32)
        } else {
            [0.0; 3]
        };
        for (i, a) in acceleration.into_iter().enumerate() {
            next.velocity[i] += a * delta;
            next.position[i] += (next.velocity[i] * drag) * delta;
        }
        next.force = [0.0; 3];
        let spin_drag = (1.0 - sample(Curve::SpinDrag)? * 0.01).clamp(0.0, 1.0);
        next.angle += (next.spin * spin_drag) * delta;
        let track = next.transform.point([
            sample(Curve::TrackX)?,
            sample(Curve::TrackY)?,
            sample(Curve::TrackZ)?,
        ]);
        let blend = (1.0 - sample(Curve::Blend)? * delta).clamp(0.0, 1.0);
        next.old_render_position = next.render_position;
        next.render_position =
            std::array::from_fn(|i| track[i] + (next.position[i] - track[i]) * blend);
        next.position = next.render_position;
        next.render_angle = next.angle;
        ensure!(next.finite(), "particle motion overflow");
        *self = next;
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
