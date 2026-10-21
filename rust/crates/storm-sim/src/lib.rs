//! Approximate sailing and wave response, not Storm gameplay parity.
//! Units: metres, seconds, kilograms, radians. Heading 0 points +Z, +pi/2 points +X.
use glam::DVec2;
use std::{error::Error, fmt};
pub mod fire;
mod flags;
pub mod particle_emitter;
pub mod particle_motion;
pub mod particle_transform;
pub mod pose_history;
mod rig;
pub mod ship_queries;
pub mod ship_sails;
pub use flags::{FlagCell, FlagNation, FlagSelection, FlagTexture, ShipFlagRole, ShipFlagState};
mod waves;
pub use rig::{RigLayout, RigState, SailMode, SailTier};
pub use waves::{HullMotion, WaveComponent, Waves};

pub const STEP_SECONDS: f64 = 1.0 / 120.0;
/// Fraction of full area per second: four seconds for a complete manoeuvre.
pub const SAIL_DEPLOY_RATE: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimError(pub &'static str);
impl fmt::Display for SimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl Error for SimError {}
type Result<T> = std::result::Result<T, SimError>;

#[derive(Clone, Copy, Debug)]
pub struct Wind {
    pub speed_mps: f64,
    /// Direction wind travels towards; same heading convention as the ship.
    pub direction: f64,
}
impl Wind {
    pub fn validate(self) -> Result<()> {
        if !self.speed_mps.is_finite()
            || !(0.0..=40.0).contains(&self.speed_mps)
            || !self.direction.is_finite()
        {
            return Err(SimError("wind must be finite and 0..=40 m/s"));
        }
        Ok(())
    }
    pub fn velocity(self) -> DVec2 {
        DVec2::new(self.direction.sin(), self.direction.cos()) * self.speed_mps
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Helm {
    /// Original sail-set request: 0 furled, 0.5 normal, 1 full; original thresholds apply.
    pub sail: f64,
    /// -1 port, +1 starboard. Rudder force depends on forward water speed.
    pub rudder: f64,
}
impl Default for Helm {
    fn default() -> Self {
        Self {
            sail: 1.0,
            rudder: 0.0,
        }
    }
}
impl Helm {
    pub fn validate(self) -> Result<()> {
        if !self.sail.is_finite()
            || !(0.0..=1.0).contains(&self.sail)
            || !self.rudder.is_finite()
            || !(-1.0..=1.0).contains(&self.rudder)
        {
            return Err(SimError("invalid sail/rudder setting"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ShipConfig {
    pub mass_kg: f64,
    pub length_m: f64,
    pub sail_area_m2: f64,
}
impl Default for ShipConfig {
    fn default() -> Self {
        Self {
            mass_kg: 120_000.0,
            length_m: 35.0,
            sail_area_m2: 350.0,
        }
    }
}
impl ShipConfig {
    fn validate(self) -> Result<()> {
        if !self.mass_kg.is_finite()
            || !(1_000.0..=1e9).contains(&self.mass_kg)
            || !self.length_m.is_finite()
            || !(2.0..=500.0).contains(&self.length_m)
            || !self.sail_area_m2.is_finite()
            || !(0.0..=1e5).contains(&self.sail_area_m2)
        {
            return Err(SimError("invalid ship configuration"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShipState {
    /// X/Z position and velocity on the water plane.
    pub position: DVec2,
    pub velocity: DVec2,
    pub heading: f64,
    pub yaw_rate: f64,
}
impl ShipState {
    pub fn forward(self) -> DVec2 {
        DVec2::new(self.heading.sin(), self.heading.cos())
    }
    pub fn speed_knots(self) -> f64 {
        self.velocity.length() * 1.943_844_492_440_6
    }
    pub fn apparent_wind(self, wind: Wind) -> DVec2 {
        wind.velocity() - self.velocity
    }
    fn validate(self) -> Result<()> {
        if !self.position.is_finite()
            || !self.velocity.is_finite()
            || !self.heading.is_finite()
            || !self.yaw_rate.is_finite()
            || self.velocity.length() > 200.0
            || self.yaw_rate.abs() > 10.0
        {
            return Err(SimError(
                "ship state is non-finite or outside simulation limits",
            ));
        }
        Ok(())
    }
}

pub struct Sailing {
    state: ShipState,
    motion: HullMotion,
    waves: Waves,
    config: ShipConfig,
    wind: Wind,
    helm: Helm,
    rig: RigState,
    rig_layout: RigLayout,
    sail_mode: SailMode,
    sail_request: ship_sails::ShipSailState,
    remainder: f64,
    ticks: u64,
    pose_history: Option<std::sync::Arc<pose_history::PoseHistory>>,
}
impl Sailing {
    /// Start with the requested sails already set; later helm changes take time.
    pub fn new_with_helm(config: ShipConfig, wind: Wind, heading: f64, helm: Helm) -> Result<Self> {
        helm.validate()?;
        let mut sim = Self::new(config, wind, heading)?;
        sim.set_helm(helm)?;
        sim.rig = RigState::for_mode(sim.sail_mode);
        Ok(sim)
    }
    pub fn new(config: ShipConfig, wind: Wind, heading: f64) -> Result<Self> {
        Self::with_state(
            config,
            wind,
            ShipState {
                position: DVec2::ZERO,
                velocity: DVec2::ZERO,
                heading,
                yaw_rate: 0.0,
            },
        )
    }
    pub fn with_state(config: ShipConfig, wind: Wind, mut state: ShipState) -> Result<Self> {
        config.validate()?;
        wind.validate()?;
        state.validate()?;
        state.heading = wrap(state.heading);
        Ok(Self {
            state,
            motion: HullMotion::default(),
            waves: Waves::default(),
            config,
            wind,
            helm: Helm::default(),
            rig: RigState::for_mode(SailMode::Full),
            rig_layout: RigLayout::default(),
            sail_mode: SailMode::Full,
            sail_request: {
                let mut state = ship_sails::ShipSailState::default();
                state.set(1.0);
                state
            },
            remainder: 0.0,
            ticks: 0,
            pose_history: None,
        })
    }
    pub fn state(&self) -> ShipState {
        self.state
    }
    pub fn motion(&self) -> HullMotion {
        self.motion
    }
    pub fn waves(&self) -> Waves {
        self.waves
    }
    pub fn set_waves(&mut self, waves: Waves) -> Result<()> {
        waves.validate()?;
        self.waves = waves;
        Ok(())
    }
    pub fn helm(&self) -> Helm {
        self.helm
    }
    /// Raw original SHIP command; applied by the preview on advance, even at zero dt.
    pub fn sail_request(&self) -> f32 {
        self.sail_request.get()
    }
    pub fn sail_deployed(&self) -> f64 {
        self.rig.fraction(self.rig_layout)
    }
    pub fn rig(&self) -> RigState {
        self.rig
    }
    pub fn rig_layout(&self) -> RigLayout {
        self.rig_layout
    }
    pub fn sail_mode(&self) -> SailMode {
        self.sail_mode
    }
    pub fn set_rig_layout(&mut self, layout: RigLayout) -> Result<()> {
        layout.validate()?;
        self.rig_layout = layout;
        Ok(())
    }
    pub fn config(&self) -> ShipConfig {
        self.config
    }
    pub fn wind(&self) -> Wind {
        self.wind
    }
    pub fn time(&self) -> f64 {
        self.ticks as f64 * STEP_SECONDS
    }
    pub fn pose_sample(&self) -> pose_history::PoseSample {
        pose_history::PoseSample {
            seconds: self.time(),
            translation: [
                self.state.position.x as f32,
                self.motion.heave_m as f32,
                self.state.position.y as f32,
            ],
            yaw: (self.state.heading + std::f64::consts::PI) as f32,
            pitch: self.motion.pitch as f32,
            roll: -self.motion.roll as f32,
        }
    }
    pub fn enable_pose_history(&mut self) -> Result<std::sync::Arc<pose_history::PoseHistory>> {
        if let Some(history) = &self.pose_history {
            return Ok(history.clone());
        }
        let history = std::sync::Arc::new(pose_history::PoseHistory::default());
        history.record(self.pose_sample())?;
        self.pose_history = Some(history.clone());
        Ok(history)
    }
    pub fn pose_history(&self) -> Option<&std::sync::Arc<pose_history::PoseHistory>> {
        self.pose_history.as_ref()
    }
    pub fn set_wind(&mut self, wind: Wind) -> Result<()> {
        wind.validate()?;
        self.wind = wind;
        Ok(())
    }
    pub fn set_helm(&mut self, mut helm: Helm) -> Result<()> {
        helm.validate()?;
        let request = helm.sail as f32;
        self.sail_mode = SailMode::from_setting(f64::from(request))?;
        self.sail_request.set(request);
        helm.sail = self.sail_mode.setting();
        self.helm = helm;
        Ok(())
    }
    pub fn set_rudder(&mut self, rudder: f64) -> Result<()> {
        Helm {
            rudder,
            ..self.helm
        }
        .validate()?;
        self.helm.rudder = rudder;
        Ok(())
    }

    /// Inputs are held constant during this interval. Live callers may cap catch-up time.
    pub fn advance(&mut self, seconds: f64) -> Result<()> {
        if !seconds.is_finite() || !(0.0..=3600.0).contains(&seconds) {
            return Err(SimError(
                "simulation interval must be finite and 0..=3600 seconds",
            ));
        }
        // Native SHIP keeps every float bit pattern. The approximate solver has
        // a narrower domain; reject invalid commands here without rewriting them.
        self.sail_mode = SailMode::from_setting(f64::from(self.sail_request.get()))?;
        self.helm.sail = self.sail_mode.setting();
        let total = self.remainder + seconds;
        let steps = ((total + STEP_SECONDS * 1e-9) / STEP_SECONDS).floor() as u64;
        if let Some(history) = &self.pose_history
            && (history.read()?.len() as u64 + steps > pose_history::MAX_SAMPLES as u64
                || (self.ticks + steps) as f64 * STEP_SECONDS > 7200.0)
        {
            return Err(SimError("sailing pose history budget exceeded"));
        }
        for _ in 0..steps {
            self.step()?;
            self.ticks += 1;
            if let Some(history) = &self.pose_history {
                history.record(self.pose_sample())?;
            }
        }
        self.remainder = (total - steps as f64 * STEP_SECONDS).max(0.0);
        Ok(())
    }

    fn step(&mut self) -> Result<()> {
        let mut rig = self.rig;
        rig.advance(self.sail_mode);
        let sail_deployed = rig.fraction(self.rig_layout);
        let mut next = self.state;
        let forward = next.forward();
        let right = DVec2::new(forward.y, -forward.x);
        let surge = next.velocity.dot(forward);
        let sway = next.velocity.dot(right);
        let apparent = next.apparent_wind(self.wind);
        let alignment = apparent.normalize_or_zero().dot(forward);
        // No-go zone within 40 degrees of directly into the apparent wind;
        // smoothly restore drive over the following 25 degrees. New preview polar.
        let reach = ((alignment + 0.766_044_443) / (0.766_044_443 - 0.422_618_262)).clamp(0.0, 1.0);
        let polar = reach * reach * (3.0 - 2.0 * reach) * (1.0 - 0.3 * alignment * alignment);
        let pressure =
            0.5 * 1.225 * self.config.sail_area_m2 * sail_deployed * apparent.length_squared();
        let drive = pressure * 0.9 * polar;
        let leeway = pressure * 0.06 * apparent.normalize_or_zero().dot(right);
        let drag_forward = 2_000.0 * surge + 800.0 * surge.abs() * surge;
        let drag_side = 18_000.0 * sway + 9_000.0 * sway.abs() * sway;
        let rudder_force = 0.5 * 1025.0 * 4.0 * surge.abs() * surge * 0.8 * self.helm.rudder;
        let force = forward * (drive - drag_forward) + right * (leeway - drag_side - rudder_force);
        let inertia = self.config.mass_kg * self.config.length_m.powi(2);
        let torque = rudder_force * self.config.length_m * 0.4
            - inertia * (0.025 * next.yaw_rate + 0.25 * next.yaw_rate.abs() * next.yaw_rate);
        next.velocity += force / self.config.mass_kg * STEP_SECONDS;
        next.position += next.velocity * STEP_SECONDS;
        next.yaw_rate += torque / (inertia / 12.0) * STEP_SECONDS;
        next.heading = wrap(next.heading + next.yaw_rate * STEP_SECONDS);
        next.validate()?;
        let forward = next.forward();
        let right = DVec2::new(forward.y, -forward.x);
        let half_length = self.config.length_m * 0.4;
        let half_beam = self.config.length_m * 0.125;
        let time = (self.ticks + 1) as f64 * STEP_SECONDS;
        let sample = |offset| self.waves.sample(next.position + offset, time).0;
        let bow = sample(forward * half_length);
        let stern = sample(-forward * half_length);
        let starboard = sample(right * half_beam);
        let port = sample(-right * half_beam);
        self.motion.step([
            (sample(DVec2::ZERO) * 2.0 + bow + stern + starboard + port) / 6.0,
            (bow - stern).atan2(2.0 * half_length),
            (starboard - port).atan2(2.0 * half_beam),
        ]);
        self.state = next;
        self.rig = rig;
        Ok(())
    }
}

fn wrap(angle: f64) -> f64 {
    (angle + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wind() -> Wind {
        Wind {
            speed_mps: 12.0,
            direction: 0.0,
        }
    }
    fn sim() -> Sailing {
        Sailing::new(ShipConfig::default(), wind(), 0.0).unwrap()
    }

    #[test]
    fn wave_response_is_damped_bounded_and_independent_of_frame_partition() {
        let mut a = sim();
        let mut b = sim();
        let waves = Waves {
            height_m: 4.0,
            direction: 1.1,
        };
        a.set_waves(waves).unwrap();
        b.set_waves(waves).unwrap();
        a.advance(60.0).unwrap();
        for _ in 0..3600 {
            b.advance(1.0 / 60.0).unwrap();
        }
        assert_eq!(a.state(), b.state());
        assert_eq!(a.motion(), b.motion());
        let mut peaks = [0.0_f64; 3];
        for _ in 0..3540 {
            a.advance(1.0).unwrap();
            let m = a.motion();
            assert!(m.heave_m.is_finite() && m.heave_m.abs() < 3.0);
            assert!(m.pitch.is_finite() && m.pitch.abs() < 0.4);
            assert!(m.roll.is_finite() && m.roll.abs() < 0.4);
            for (peak, value) in peaks.iter_mut().zip([m.heave_m, m.pitch, m.roll]) {
                *peak = peak.max(value.abs());
            }
        }
        assert!(peaks[0] > 0.1 && peaks[1] > 0.01 && peaks[2] > 0.01);
        a.set_waves(Waves::default()).unwrap();
        a.advance(60.0).unwrap();
        let m = a.motion();
        assert!(m.heave_m.abs() + m.pitch.abs() + m.roll.abs() < 1e-10);
        let mut flat = sim();
        flat.advance(60.0).unwrap();
        assert_eq!(flat.motion(), HullMotion::default());
    }

    #[test]
    fn wind_drives_the_ship_but_headwind_and_calm_do_not() {
        let mut down = sim();
        down.advance(60.0).unwrap();
        assert!(down.state.position.y > 50.0 && down.state.speed_knots() > 2.0);
        assert!(down.state.position.x.abs() < 1e-8);
        for w in [
            Wind {
                speed_mps: 0.0,
                direction: 0.0,
            },
            Wind {
                direction: std::f64::consts::PI,
                ..wind()
            },
        ] {
            let mut boat = Sailing::new(ShipConfig::default(), w, 0.0).unwrap();
            boat.advance(60.0).unwrap();
            assert!(boat.state.velocity.length() < 1e-8);
        }
    }

    #[test]
    fn fixed_steps_ignore_frame_partition_and_invalid_input_is_rejected() {
        let mut a = sim();
        a.set_helm(Helm {
            sail: 0.8,
            rudder: 0.3,
        })
        .unwrap();
        let mut b = sim();
        b.set_helm(a.helm).unwrap();
        a.advance(30.0).unwrap();
        for _ in 0..1800 {
            b.advance(1.0 / 60.0).unwrap();
        }
        assert_eq!(a.state, b.state);
        assert_eq!(a.ticks, b.ticks);
        let before = a.state;
        assert!(a.advance(f64::NAN).is_err());
        assert!(
            a.set_helm(Helm {
                sail: 2.0,
                rudder: 0.0
            })
            .is_err()
        );
        assert!(
            a.set_wind(Wind {
                speed_mps: 41.0,
                ..wind()
            })
            .is_err()
        );
        assert_eq!(before, a.state);
    }

    #[test]
    fn rudder_needs_water_speed_and_turns_in_opposite_directions() {
        let mut port = sim();
        let mut starboard = sim();
        port.set_helm(Helm {
            sail: 1.0,
            rudder: -0.7,
        })
        .unwrap();
        starboard
            .set_helm(Helm {
                sail: 1.0,
                rudder: 0.7,
            })
            .unwrap();
        port.advance(30.0).unwrap();
        starboard.advance(30.0).unwrap();
        assert!(port.state.heading < -0.05 && starboard.state.heading > 0.05);
        assert!((port.state.position.x + starboard.state.position.x).abs() < 1e-8);
        assert!((port.state.position.y - starboard.state.position.y).abs() < 1e-8);
        let mut parked = sim();
        parked
            .set_wind(Wind {
                speed_mps: 0.0,
                ..wind()
            })
            .unwrap();
        parked.set_helm(starboard.helm).unwrap();
        parked.advance(30.0).unwrap();
        assert_eq!(parked.state.heading, 0.0);
    }

    #[test]
    fn reduced_sails_reduce_drive_and_water_resistance_dissipates_motion() {
        let mut full = sim();
        let mut reefed = sim();
        reefed
            .set_helm(Helm {
                sail: 0.25,
                rudder: 0.0,
            })
            .unwrap();
        full.advance(30.0).unwrap();
        reefed.advance(30.0).unwrap();
        assert!(full.state.velocity.length() > reefed.state.velocity.length());
        let mut coast = Sailing::with_state(
            ShipConfig::default(),
            Wind {
                speed_mps: 0.0,
                ..wind()
            },
            ShipState {
                position: DVec2::ZERO,
                velocity: DVec2::new(1.0, 8.0),
                heading: 0.0,
                yaw_rate: 0.1,
            },
        )
        .unwrap();
        coast
            .set_helm(Helm {
                sail: 0.0,
                rudder: 0.0,
            })
            .unwrap();
        let mut previous = coast.state.velocity.length_squared();
        for _ in 0..1200 {
            coast.advance(STEP_SECONDS).unwrap();
            let speed = coast.state.velocity.length_squared();
            assert!(speed <= previous + 1e-10);
            previous = speed;
        }
        assert!(coast.state.velocity.length() < 6.0);
        assert!(coast.state.yaw_rate.abs() < 0.01);
    }

    #[test]
    fn maximum_wind_remains_finite_over_an_hour_and_zero_sails_provide_no_drive() {
        for rudder in [-1.0, 0.0, 1.0] {
            let mut boat = Sailing::new(
                ShipConfig::default(),
                Wind {
                    speed_mps: 40.0,
                    direction: 0.7,
                },
                0.0,
            )
            .unwrap();
            boat.set_helm(Helm { sail: 1.0, rudder }).unwrap();
            boat.advance(3600.0).unwrap();
            assert!(boat.state.velocity.length() < 40.0);
            assert!(boat.state.position.is_finite());
        }
        let mut stopped = Sailing::new_with_helm(
            ShipConfig::default(),
            wind(),
            0.0,
            Helm {
                sail: 0.0,
                rudder: 1.0,
            },
        )
        .unwrap();
        stopped.advance(30.0).unwrap();
        assert_eq!(stopped.state.velocity, DVec2::ZERO);
    }

    #[test]
    fn sails_move_at_a_fixed_rate_reverse_and_drive_with_actual_area() {
        let mut a = sim();
        let mut b = sim();
        let helm = Helm {
            sail: 0.0,
            rudder: 0.0,
        };
        a.set_helm(helm).unwrap();
        b.set_helm(helm).unwrap();
        assert_eq!(
            a.sail_deployed(),
            1.0,
            "setting a target must not teleport the rig"
        );
        a.advance(2.0).unwrap();
        for _ in 0..120 {
            b.advance(1.0 / 60.0).unwrap();
        }
        assert_eq!(a.sail_deployed(), b.sail_deployed());
        assert_eq!(a.state(), b.state());
        assert!((a.sail_deployed() - 0.5).abs() < 1e-12);
        assert!(
            a.state.speed_knots() > 0.1,
            "remaining exposed cloth still drives the boat"
        );
        a.set_helm(Helm::default()).unwrap();
        a.advance(1.0).unwrap();
        assert!((a.sail_deployed() - 0.75).abs() < 1e-12);
        a.set_helm(helm).unwrap();
        a.advance(3.0).unwrap();
        assert_eq!(a.sail_deployed(), 0.0);
        a.set_helm(Helm::default()).unwrap();
        a.advance(4.0).unwrap();
        assert_eq!(a.sail_deployed(), 1.0);
        assert!(
            a.set_helm(Helm {
                sail: f64::NAN,
                ..helm
            })
            .is_err()
        );
        assert_eq!(a.sail_deployed(), 1.0);
    }
}
