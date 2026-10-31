//! Retained sail rolling state and yard-turn decisions, with an explicit node port.
//! Rope-constrained turning and native cloth construction are separate providers.
use crate::sail_drive::Kind;
use anyhow::{Result, ensure};

pub type Geometry = [[f32; 3]; 10];
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rolling {
    pub geometry: Geometry,
    pub delta: f32,
    pub up: bool,
}
pub trait NodePort {
    fn position(&mut self) -> [f32; 3];
    fn set_position(&mut self, value: [f32; 3]);
    fn rotate_y(&mut self, angle: f32);
    fn update_global(&mut self);
}
#[derive(Clone, Debug, PartialEq)]
pub struct SailMotion {
    pub kind: Kind,
    pub geometry: Geometry,
    pub rolling: bool,
    pub transition: Option<Rolling>,
    pub old_wind_angle: f32,
    pub minimum_angle: f32,
    pub maximum_angle: f32,
    pub hard_points: [[f32; 3]; 4],
    pub ties: [Option<[f32; 3]>; 4],
}
impl SailMotion {
    pub fn new(kind: Kind, rolling: bool) -> Self {
        Self {
            kind,
            rolling,
            geometry: [[0.0; 3]; 10],
            transition: None,
            old_wind_angle: 0.0,
            minimum_angle: 0.6,
            maximum_angle: 0.6,
            hard_points: [[0.0; 3]; 4],
            ties: [None; 4],
        }
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.geometry
                .into_iter()
                .flatten()
                .chain(self.hard_points.into_iter().flatten())
                .chain(self.ties.into_iter().flatten().flatten())
                .chain([self.old_wind_angle, self.minimum_angle, self.maximum_angle])
                .all(f32::is_finite),
            "non-finite sail motion"
        );
        if let Some(t) = self.transition {
            ensure!(
                t.geometry
                    .into_iter()
                    .flatten()
                    .chain([t.delta])
                    .all(f32::is_finite),
                "non-finite sail transition"
            );
        }
        Ok(())
    }
    fn orientation(&self) -> f32 {
        if matches!(self.kind, Kind::Triangle | Kind::Trapezoid | Kind::Special) {
            1.0
        } else {
            -1.0
        }
    }
    pub fn set_rolling(
        &mut self,
        up: bool,
        forced_step: f32,
        node: &mut impl NodePort,
    ) -> Result<()> {
        self.validate()?;
        ensure!(forced_step.is_finite(), "non-finite forced yard step");
        if self.transition.map_or(up == self.rolling, |t| t.up == up) {
            return Ok(());
        }
        let mut next = self.clone();
        next.rolling = false;
        if !up {
            let step = if next.old_wind_angle < 0.0 {
                forced_step
            } else {
                -forced_step
            };
            next.old_wind_angle += step;
            ensure!(next.old_wind_angle.is_finite(), "yard angle overflow");
            let position = node.position();
            ensure!(
                position.into_iter().all(f32::is_finite),
                "invalid yard position"
            );
            node.set_position([0.0; 3]);
            node.rotate_y(step * next.orientation());
            node.set_position(position);
        }
        if let Some(t) = &mut next.transition {
            t.up = up;
        } else {
            next.transition = Some(Rolling {
                geometry: next.geometry,
                delta: if up { 1.0 } else { 0.0 },
                up,
            });
        }
        *self = next;
        Ok(())
    }
    pub fn advance(&mut self, delta_ms: u32, rate: f32) -> Result<()> {
        self.validate()?;
        ensure!(rate.is_finite(), "non-finite sail rolling rate");
        let Some(t) = self.transition else {
            return Ok(());
        };
        let mut next = self.clone();
        let mut delta = t.delta;
        if t.up {
            delta -= delta_ms as f32 * rate;
            if delta < 0.0 {
                next.rolling = true;
                next.geometry = t.geometry;
                next.transition = None;
                *self = next;
                return Ok(());
            }
        } else {
            delta += delta_ms as f32 * rate;
            // Native checks the preceding delta, so one overshooting frame is retained.
            if t.delta > 1.0 {
                next.rolling = false;
                next.geometry = t.geometry;
                next.transition = None;
                *self = next;
                return Ok(());
            }
        }
        ensure!(delta.is_finite(), "sail transition overflow");
        for i in [2, 4, 1, 3, 0, 5, 7, 6, 8] {
            next.geometry[i] = t.geometry[i].map(|v| v * delta);
        }
        next.transition.as_mut().unwrap().delta = delta;
        next.validate()?;
        *self = next;
        Ok(())
    }
    /// TurnSail's path with no rope service. Folded ties 2/3 retain native endpoints.
    pub fn turn(
        &mut self,
        wind_x: f32,
        step: f32,
        discrete: f32,
        node: &mut impl NodePort,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            [wind_x, step, discrete].into_iter().all(f32::is_finite),
            "non-finite yard turn input"
        );
        let mut angle = -wind_x;
        angle *= if angle < 0.0 {
            self.minimum_angle
        } else {
            self.maximum_angle
        };
        angle -= self.old_wind_angle;
        if angle < -discrete || angle > discrete {
            if angle > step {
                angle = step;
            }
            if angle < -step {
                angle = -step;
            }
            let next = self.old_wind_angle + angle;
            ensure!(next.is_finite(), "yard turn overflow");
            let position = node.position();
            ensure!(
                position.into_iter().all(f32::is_finite),
                "invalid yard position"
            );
            self.old_wind_angle = next;
            node.set_position([0.0; 3]);
            node.rotate_y(angle * self.orientation());
            node.set_position(position);
            node.update_global();
            if self.rolling {
                for (slot, point) in [(2, 0), (3, 1)] {
                    if self.ties[slot].is_some() {
                        self.ties[slot] = Some(self.hard_points[point]);
                    }
                }
            }
        }
        Ok(())
    }
    /// Renderers bound visible deployment; the raw native delta remains available for drive.
    pub fn deployment(&self) -> f32 {
        if self.rolling {
            0.0
        } else {
            self.transition.map_or(1.0, |t| t.delta).clamp(0.0, 1.0)
        }
    }
}

#[cfg(test)]
mod tests;
