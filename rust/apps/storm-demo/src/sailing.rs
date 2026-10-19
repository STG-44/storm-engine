use super::Options;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use storm_core::{Entity, Message, Value as MessageValue};
use storm_render::model::{ModelPose, SailWind};
use storm_sim::{Helm, RigLayout, Sailing, ShipConfig, Waves, Wind};
use winit::{event::ElementState, keyboard::KeyCode};

pub struct Session {
    pub simulation: Sailing,
    last_clock: f32,
    left: bool,
    right: bool,
    sail_up: bool,
    sail_down: bool,
    pub initial_yaw: f32,
    script_report: Option<Value>,
}

fn wind(value: SailWind) -> Wind {
    Wind {
        speed_mps: f64::from(value.speed),
        direction: f64::from(value.direction),
    }
}
impl Session {
    pub fn new(options: &Options, layout: RigLayout) -> Result<Option<Self>> {
        if !options.sail {
            return Ok(None);
        }
        ensure!(
            (0.0..=3600.0).contains(&options.time),
            "sailing --time must be 0..=3600 seconds"
        );
        let heading = options
            .heading
            .unwrap_or(options.wind_direction)
            .rem_euclid(360.0)
            .to_radians();
        let mut simulation = Sailing::new_with_helm(
            ShipConfig::default(),
            wind(options.wind()),
            f64::from(heading),
            Helm {
                sail: f64::from(options.sail_setting),
                rudder: f64::from(options.rudder),
            },
        )?;
        simulation.set_rig_layout(layout)?;
        if let Some(target) = options.sail_target {
            simulation.set_helm(Helm {
                sail: f64::from(target),
                ..simulation.helm()
            })?;
        }
        simulation.set_waves(Waves {
            height_m: f64::from(options.wave_height.unwrap_or(1.5)),
            direction: f64::from(
                options
                    .wave_direction
                    .unwrap_or(options.wind_direction)
                    .rem_euclid(360.0)
                    .to_radians(),
            ),
        })?;
        let script_report = if let Some(path) = &options.sail_script {
            let (scripted, report) = super::ship_script::run(simulation, path)?;
            simulation = scripted;
            Some(report)
        } else {
            None
        };
        simulation.advance(f64::from(options.time))?;
        Ok(Some(Self {
            simulation,
            last_clock: options.time,
            left: false,
            right: false,
            sail_up: false,
            sail_down: false,
            initial_yaw: heading + std::f32::consts::PI,
            script_report,
        }))
    }

    pub fn update(&mut self, seconds: f32, ambient: SailWind) -> Result<()> {
        self.simulation.set_wind(wind(ambient))?;
        // Cap live catch-up after stalls; headless replay always integrates the full interval.
        let delta = f64::from(seconds - self.last_clock).clamp(0.0, 0.25);
        self.simulation.advance(delta)?;
        self.last_clock = seconds;
        Ok(())
    }

    pub fn pose(&self) -> ModelPose {
        let state = self.simulation.state();
        let motion = self.simulation.motion();
        ModelPose {
            translation: [
                state.position.x as f32,
                motion.heave_m as f32,
                state.position.y as f32,
            ],
            // Storm bows point +Z; the static loader reflects Z into renderer coordinates.
            yaw: (state.heading + std::f64::consts::PI) as f32,
            pitch: motion.pitch as f32,
            // After the Z reflection, model +X is port in the heading frame.
            roll: -motion.roll as f32,
        }
    }

    pub fn cloth_wind(&self, ambient: SailWind) -> SailWind {
        let apparent = self.simulation.state().apparent_wind(wind(ambient));
        SailWind {
            speed: apparent.length().min(40.0) as f32,
            direction: apparent.x.atan2(apparent.y) as f32,
        }
    }

    pub fn key(&mut self, key: KeyCode, state: ElementState) -> Result<()> {
        let pressed = state == ElementState::Pressed;
        let helm = self.simulation.helm();
        match key {
            KeyCode::KeyA => self.left = pressed,
            KeyCode::KeyD => self.right = pressed,
            KeyCode::KeyW => {
                if pressed && !self.sail_up {
                    self.set_sails((helm.sail + 0.5).min(1.0) as f32)?;
                }
                self.sail_up = pressed;
            }
            KeyCode::KeyS => {
                if pressed && !self.sail_down {
                    self.set_sails((helm.sail - 0.5).max(0.0) as f32)?;
                }
                self.sail_down = pressed;
            }
            _ => {}
        }
        if matches!(key, KeyCode::KeyA | KeyCode::KeyD) {
            self.simulation
                .set_rudder(f64::from(i8::from(self.right) - i8::from(self.left)))?;
        }
        Ok(())
    }
    fn set_sails(&mut self, setting: f32) -> Result<()> {
        let mut message = Message::new(
            "lf",
            vec![
                MessageValue::Long(storm_sim::ship_sails::MSG_SHIP_SET_SAIL_STATE),
                MessageValue::Float(setting),
            ],
        )?;
        self.simulation.process_message(&mut message)?;
        self.simulation.advance(0.0)?;
        Ok(())
    }
    pub fn release_rudder(&mut self) -> Result<()> {
        self.left = false;
        self.right = false;
        self.sail_up = false;
        self.sail_down = false;
        self.simulation.set_rudder(0.0)?;
        Ok(())
    }

    pub fn title(&self) -> String {
        let s = self.simulation.state();
        let helm = self.simulation.helm();
        format!(
            " — {:.1} kn {:.0}° — sails {} ({:.0}% area) rudder {:+.0}%",
            s.speed_knots(),
            s.heading.to_degrees().rem_euclid(360.0),
            self.simulation.sail_mode().name(),
            self.simulation.sail_deployed() * 100.0,
            helm.rudder * 100.0
        )
    }
    pub fn report(&self) -> Value {
        let s = self.simulation.state();
        let helm = self.simulation.helm();
        let config = self.simulation.config();
        let wind = self.simulation.wind();
        let waves = self.simulation.waves();
        let motion = self.simulation.motion();
        let rig = self.simulation.rig();
        let layout = self.simulation.rig_layout();
        let mut report = json!({ "model": "new approximate sailing and damped wave response", "step_seconds": storm_sim::STEP_SECONDS,
            "simulation_seconds": self.simulation.time(), "position_xz_m": s.position.to_array(),
            "velocity_xz_mps": s.velocity.to_array(), "speed_knots": s.speed_knots(),
            "heading_degrees": s.heading.to_degrees().rem_euclid(360.0),
            "yaw_rate_radians_per_second": s.yaw_rate,
            "hull_motion": { "heave_m": motion.heave_m, "pitch_degrees_bow_up": motion.pitch.to_degrees(),
                "roll_degrees_starboard_up": motion.roll.to_degrees(), "rates_mps_radps": motion.rates },
            "waves": { "envelope_height_m": waves.height_m, "direction_degrees": waves.direction.to_degrees().rem_euclid(360.0) },
            "sail_setting": helm.sail, "sail_deployed": self.simulation.sail_deployed(),
            "sail_mode": self.simulation.sail_mode().name(),
            "rig": { "normal_deployed": rig.normal, "full_only_deployed": rig.full_only,
                "normal_weight": layout.normal_weight, "full_only_weight": layout.full_only_weight,"missing_weight":layout.missing_weight },
            "sail_deploy_rate_per_second": storm_sim::SAIL_DEPLOY_RATE, "rudder": helm.rudder,
            "wind": { "speed_mps": wind.speed_mps, "direction_degrees": wind.direction.to_degrees().rem_euclid(360.0) },
            "config": { "mass_kg": config.mass_kg, "length_m": config.length_m, "sail_area_m2": config.sail_area_m2 },
        });
        if let Some(script) = &self.script_report {
            report["ship_sail_script"] = script.clone();
            report["sail_request_bits"] = json!(self.simulation.sail_request().to_bits());
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn rudder_and_focus_changes_preserve_the_raw_native_sail_command() {
        let options = Options::try_parse_from(["storm-demo", "--sail-demo", "--sail"]).unwrap();
        let mut session = Session::new(&options, RigLayout::default())
            .unwrap()
            .unwrap();
        session.set_sails(0.4).unwrap();
        session.key(KeyCode::KeyA, ElementState::Pressed).unwrap();
        session.release_rudder().unwrap();
        session.update(0.0, options.wind()).unwrap();
        assert_eq!(
            session.simulation.sail_request().to_bits(),
            0.4f32.to_bits()
        );
        assert_eq!(session.simulation.helm().sail, 0.5);
        assert_eq!(
            session.simulation.rig(),
            storm_sim::RigState::for_mode(storm_sim::SailMode::Full)
        );
    }

    #[test]
    fn sail_keys_select_three_modes_on_activation_without_key_repeat_steps() {
        let options = Options::try_parse_from(["storm-demo", "--sail-demo", "--sail"]).unwrap();
        let mut session = Session::new(&options, RigLayout::default())
            .unwrap()
            .unwrap();
        for expected in [0.5, 0.0, 0.0] {
            session.key(KeyCode::KeyS, ElementState::Pressed).unwrap();
            assert_eq!(session.simulation.helm().sail, expected);
            session.key(KeyCode::KeyS, ElementState::Pressed).unwrap();
            assert_eq!(session.simulation.helm().sail, expected);
            session.key(KeyCode::KeyS, ElementState::Released).unwrap();
        }
        for expected in [0.5, 1.0, 1.0] {
            session.key(KeyCode::KeyW, ElementState::Pressed).unwrap();
            assert_eq!(session.simulation.helm().sail, expected);
            session.key(KeyCode::KeyW, ElementState::Released).unwrap();
        }
    }

    #[test]
    fn replay_reports_actual_area_and_pause_holds_a_rig_mid_manoeuvre() {
        let options = Options::try_parse_from([
            "storm-demo",
            "--model",
            "fixture.gm",
            "--sail",
            "--sail-setting",
            "1",
            "--sail-target",
            "0",
            "--time",
            "2",
        ])
        .unwrap();
        let mut session = Session::new(&options, RigLayout::default())
            .unwrap()
            .unwrap();
        assert!((session.simulation.sail_deployed() - 0.5).abs() < 1e-12);
        assert_eq!(session.simulation.helm().sail, 0.0);
        let motion = session.simulation.state();
        session.update(2.0, options.wind()).unwrap();
        assert_eq!(session.simulation.state(), motion);
        for _ in 0..10 {
            session.key(KeyCode::KeyW, ElementState::Pressed).unwrap();
            session.key(KeyCode::KeyW, ElementState::Released).unwrap();
        }
        assert_eq!(session.simulation.helm().sail, 1.0);
        let deployed = session.simulation.sail_deployed();
        session.update(2.0, options.wind()).unwrap();
        assert_eq!(
            session.simulation.sail_deployed(),
            deployed,
            "paused controls change only the target"
        );
        session.update(2.25, options.wind()).unwrap();
        assert!((session.simulation.sail_deployed() - 0.5625).abs() < 1e-12);
        let report = session.report();
        assert!((report["sail_deployed"].as_f64().unwrap() - 0.5625).abs() < 1e-12);
    }

    #[test]
    fn helm_tracks_releases_opposing_keys_and_focus_loss() {
        let options = Options::try_parse_from([
            "storm-demo",
            "--model",
            "fixture.gm",
            "--untextured",
            "--sail",
        ])
        .unwrap();
        let mut session = Session::new(&options, RigLayout::default())
            .unwrap()
            .unwrap();
        let pose = session.pose();
        let bow = pose
            .transform_point([0.0, 0.0, -1.0].into())
            .unwrap()
            .to_array();
        let forward = session.simulation.state().forward();
        assert!((f64::from(bow[0]) - forward.x).abs() < 1e-5);
        assert!((f64::from(bow[2]) - forward.y).abs() < 1e-5);
        session.key(KeyCode::KeyA, ElementState::Pressed).unwrap();
        assert_eq!(session.simulation.helm().rudder, -1.0);
        session.key(KeyCode::KeyD, ElementState::Pressed).unwrap();
        assert_eq!(session.simulation.helm().rudder, 0.0);
        session.key(KeyCode::KeyD, ElementState::Released).unwrap();
        assert_eq!(session.simulation.helm().rudder, -1.0);
        session.release_rudder().unwrap();
        assert_eq!(session.simulation.helm().rudder, 0.0);
        session.key(KeyCode::KeyS, ElementState::Pressed).unwrap();
        let sail = session.simulation.helm().sail;
        session.key(KeyCode::KeyS, ElementState::Released).unwrap();
        assert_eq!(session.simulation.helm().sail, sail);
        let stopped_time = session.simulation.state();
        session.update(0.0, options.wind()).unwrap();
        assert_eq!(session.simulation.state(), stopped_time);
    }
}
