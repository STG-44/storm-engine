//! Retained SHIP strength application and movement, with explicit speed/sea ports.
use anyhow::{Result, bail, ensure};
use storm_core::{AttributeId, AttributeStore, Entity, Message};

pub const MSG_SHIP_ADD_MOVE_IMPULSE: i32 = 50308;
pub const MAX_STRENGTH: usize = 16;
pub const RESERVED_STRENGTH: usize = 1;
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Strength {
    pub used: bool,
    pub inertia: bool,
    pub speed: [f32; 3],
    pub rotation: [f32; 3],
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MotionState {
    pub position: [f32; 3],
    pub angles: [f32; 3],
    /// Native local speed is in knots; yaw rotation is radians/second.
    pub speed: [f32; 3],
    pub rotation: [f32; 3],
    pub inertia_accel: [f32; 3],
    pub inertia_brake: [f32; 3],
    pub mass_inertia: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TouchParams {
    pub speed: [f32; 3],
    pub rotation: [f32; 3],
    pub position: [f32; 3],
    pub angles: [f32; 3],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchPreview {
    pub old: Option<TouchParams>,
    pub new: Option<TouchParams>,
}
/// Mount/sail/weather supply target limits; the sea provider supplies ShipRocking.
pub trait MovementPort {
    // Pin the scalar software implementation: platform sinf/cosf differ in low bits.
    fn sin(&mut self, angle: f32) -> f32 {
        libm::sinf(angle)
    }
    fn cos(&mut self, angle: f32) -> f32 {
        libm::cosf(angle)
    }
    fn max_speed_z(&mut self) -> f32;
    fn max_speed_y(&mut self) -> f32;
    fn rocking(
        &mut self,
        position: [f32; 3],
        angles: [f32; 3],
        delta: f32,
    ) -> Result<(f32, [f32; 3])>;
}
#[derive(Clone, Copy, Debug, Default)]
pub struct FlatMovement {
    pub max_speed_z: f32,
    pub max_speed_y: f32,
}
impl MovementPort for FlatMovement {
    fn max_speed_z(&mut self) -> f32 {
        self.max_speed_z
    }
    fn max_speed_y(&mut self) -> f32 {
        self.max_speed_y
    }
    fn rocking(&mut self, position: [f32; 3], angles: [f32; 3], _: f32) -> Result<(f32, [f32; 3])> {
        Ok((position[1], angles))
    }
}
impl FlatMovement {
    /// Retained GetMaxSpeedZ/Y lookups over the current character attribute tree.
    pub fn from_character(store: &AttributeStore, character: AttributeId) -> Result<Self> {
        ensure!(
            store.get(character).is_some(),
            "stale or foreign motion character binding"
        );
        let value = |path: &str| -> Result<f32> {
            Ok(store
                .lookup_path(character, path)?
                .map_or(0.0, |id| store.as_float(id, None, 0.0)))
        };
        Ok(Self {
            max_speed_z: value("Ship.MaxSpeedZ")?,
            max_speed_y: value("Ship.MaxSpeedY")?,
        })
    }
}
#[derive(Clone, Debug)]
pub struct ShipMotion {
    pub state: MotionState,
    pub strengths: [Strength; MAX_STRENGTH],
    pub speed_accel: [f32; 3],
    pub offsets: [f32; 2],
    pub fixed_speed: Option<f32>,
    pub perk_angle: Option<f32>,
    pub dead: bool,
}
impl Default for ShipMotion {
    fn default() -> Self {
        let mut strengths = [Strength::default(); MAX_STRENGTH];
        strengths[0].inertia = true;
        Self {
            state: MotionState::default(),
            strengths,
            speed_accel: [0.0; 3],
            offsets: [0.0; 2],
            fixed_speed: None,
            perk_angle: None,
            dead: false,
        }
    }
}
impl ShipMotion {
    fn validate(&self) -> Result<()> {
        let s = &self.state;
        ensure!(
            s.position
                .into_iter()
                .chain(s.angles)
                .chain(s.speed)
                .chain(s.rotation)
                .chain(s.inertia_accel)
                .chain(s.inertia_brake)
                .chain(self.speed_accel)
                .chain(self.offsets)
                .chain([s.mass_inertia])
                .chain(self.fixed_speed)
                .chain(self.perk_angle)
                .all(f32::is_finite),
            "non-finite native ship motion state"
        );
        ensure!(
            self.strengths.iter().all(|s| s
                .speed
                .into_iter()
                .chain(s.rotation)
                .all(f32::is_finite)),
            "non-finite ship strength"
        );
        Ok(())
    }
    pub fn add_strength(&mut self, mut strength: Strength) -> Result<Option<usize>> {
        ensure!(
            strength
                .speed
                .into_iter()
                .chain(strength.rotation)
                .all(f32::is_finite),
            "non-finite ship strength"
        );
        let Some(index) = (RESERVED_STRENGTH..MAX_STRENGTH).find(|&i| !self.strengths[i].used)
        else {
            return Ok(None);
        };
        strength.used = true;
        self.strengths[index] = strength;
        Ok(Some(index))
    }
    pub fn del_strength(&mut self, index: i32) -> bool {
        if !(RESERVED_STRENGTH as i32..MAX_STRENGTH as i32).contains(&index) {
            return false;
        }
        self.strengths[index as usize].used = false;
        true
    }
    pub fn calculate_target(&self, port: &mut impl MovementPort) -> Result<([f32; 3], [f32; 3])> {
        let mut speed = [0.0; 3];
        let mut rotation = [0.0; 3];
        // Native reserved slots contribute even when bUse is false.
        for strength in &self.strengths[..RESERVED_STRENGTH] {
            for i in 0..3 {
                speed[i] += strength.speed[i];
                rotation[i] += strength.rotation[i];
            }
        }
        speed[2] *= port.max_speed_z();
        rotation[1] *= port.max_speed_y();
        ensure!(
            speed.into_iter().chain(rotation).all(f32::is_finite),
            "non-finite native target speed"
        );
        Ok((speed, rotation))
    }
    fn apply(&mut self, delta: f32, collision: bool, port: &mut impl MovementPort) -> Result<()> {
        for strength in &mut self.strengths[RESERVED_STRENGTH..] {
            if strength.used && !strength.inertia {
                for i in 0..3 {
                    self.state.speed[i] += strength.speed[i];
                    self.state.rotation[i] += strength.rotation[i];
                }
                if !collision {
                    strength.used = false;
                }
            }
        }
        let (speed, mut rotation) = self.calculate_target(port)?;
        if let Some(angle) = self.perk_angle {
            rotation[1] = (if angle < 0.0 { -1.0 } else { 1.0 }) * port.max_speed_y();
        }
        let small_delta = |x: f32| (x - 0.00001f32).abs() > 0.00001f32;
        // Native x/z movement and y rotation retain their evaluation order.
        for i in 0..3 {
            let k = delta * self.state.mass_inertia;
            if i == 0 || i == 2 {
                if i == 2 {
                    self.speed_accel[2] = self.state.speed[i];
                }
                let force = speed[i] - self.state.speed[i];
                if small_delta(speed[i]) {
                    self.state.speed[i] += k * force * self.state.inertia_accel[i];
                } else {
                    self.state.speed[i] += k * -self.state.speed[i] * self.state.inertia_brake[i];
                }
                if i == 2 {
                    self.speed_accel[2] -= self.state.speed[i];
                }
            }
            if i == 1 {
                let force = rotation[i] - self.state.rotation[i];
                if small_delta(rotation[i]) {
                    self.state.rotation[i] += k * force * self.state.inertia_accel[i];
                } else {
                    self.state.rotation[i] +=
                        k * -self.state.rotation[i] * self.state.inertia_brake[i];
                }
            }
        }
        Ok(())
    }
    /// Execute one native millisecond frame. Invalid results do not replace the live body.
    pub fn advance(
        &mut self,
        delta_ms: u32,
        collision: bool,
        port: &mut impl MovementPort,
    ) -> Result<()> {
        self.validate()?;
        let mut next = self.clone();
        let delta = delta_ms as f32 * 0.001f32;
        next.apply(delta, collision, port)?;
        if !collision && !next.dead {
            let (height, angles) = port.rocking(next.state.position, next.state.angles, delta)?;
            next.state.position[1] = height;
            next.state.angles = angles;
        }
        let knots_to_meters = |v: f32| v * 1853.0f32 / 3600.0f32;
        let mut k = knots_to_meters(next.fixed_speed.unwrap_or(next.state.speed[2])) * delta;
        next.state.position[2] += k * port.cos(next.state.angles[1]);
        next.state.position[0] += k * port.sin(next.state.angles[1]);
        k = knots_to_meters(next.state.speed[0]) * delta;
        let sideways = next.state.angles[1] + std::f32::consts::FRAC_PI_2;
        next.state.position[2] += k * port.cos(sideways);
        next.state.position[0] += k * port.sin(sideways);
        next.state.angles[1] += next.state.rotation[1] * delta;
        next.validate()?;
        *self = next;
        Ok(())
    }
    fn touch_params(&self) -> Result<TouchParams> {
        let mut position = self.state.position;
        position[0] += self.offsets[0];
        position[2] += self.offsets[1];
        ensure!(
            position.into_iter().all(f32::is_finite),
            "ship touch world position overflow"
        );
        Ok(TouchParams {
            position,
            angles: self.state.angles,
            speed: self.state.speed,
            rotation: self.state.rotation,
        })
    }
    pub fn touch_move(
        &mut self,
        delta_ms: u32,
        old: bool,
        new: bool,
        port: &mut impl MovementPort,
    ) -> Result<Option<TouchPreview>> {
        if !old && !new {
            return Ok(None);
        }
        self.validate()?;
        let old_state = self.state;
        let before = if old {
            Some(self.touch_params()?)
        } else {
            None
        };
        self.advance(delta_ms, true, port)?;
        let after = if new {
            self.touch_params().map(Some)
        } else {
            Ok(None)
        };
        // Native TouchMove restores State, while vSpeedAccel retains its update.
        self.state = old_state;
        Ok(Some(TouchPreview {
            old: before,
            new: after?,
        }))
    }
}
impl ShipMotion {
    pub fn process_message_with_port(
        &mut self,
        message: &mut Message,
        port: &mut impl MovementPort,
    ) -> Result<u64> {
        match message.long()? {
            MSG_SHIP_ADD_MOVE_IMPULSE => {
                let inertia = message.long()? != 0;
                let mut speed = [message.float()?, message.float()?, message.float()?];
                let yaw = -self.state.angles[1];
                let cos = port.cos(yaw);
                let sin = port.sin(yaw);
                let x = speed[0] * cos + speed[2] * sin;
                let z = speed[2] * cos - speed[0] * sin;
                speed[0] = x;
                speed[2] = z;
                let rotation = [message.float()?, message.float()?, message.float()?];
                let _ = self.add_strength(Strength {
                    inertia,
                    speed,
                    rotation,
                    ..Default::default()
                })?;
            }
            code => bail!("SHIP motion message {code} is not implemented"),
        }
        Ok(0)
    }
}
impl Entity for ShipMotion {
    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        self.process_message_with_port(message, &mut FlatMovement::default())
    }
}
#[cfg(test)]
mod tests;
