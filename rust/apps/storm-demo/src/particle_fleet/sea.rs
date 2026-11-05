//! Original frame bytes are external; generated fixtures keep CI asset-free.
use super::*;
mod stages;
mod weather;
use std::path::Path;
use storm_sim::sea::{CELLS, FRAMES, Frames, Parameters, Sea};

pub(super) struct State {
    pub sea: Rc<RefCell<Sea>>,
    weather: Option<weather::Session>,
    external: bool,
    stages: Option<stages::Stages>,
}

fn blue_bytes(bytes: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        bytes.len() >= 18 + CELLS * 4 && bytes.len() <= 128 * 1024,
        "invalid sea TGA length"
    );
    ensure!(
        bytes[0] == 0
            && bytes[1] == 0
            && bytes[2] == 2
            && u16::from_le_bytes([bytes[12], bytes[13]]) == 128
            && u16::from_le_bytes([bytes[14], bytes[15]]) == 128
            && bytes[16] == 32,
        "SEA requires raw 128x128 32-bit TGA without ID/color map"
    );
    // Native Init walks raw BGRA after TGA_H. Do not flip by origin flags or
    // use display-oriented image decoding; ignore alpha/red/green/footer.
    Ok(bytes[18..18 + CELLS * 4]
        .chunks_exact(4)
        .map(|p| p[0])
        .collect())
}
impl State {
    pub fn load(folder: Option<&Path>) -> Result<Self> {
        let frames = if let Some(folder) = folder {
            let mut frames = Vec::with_capacity(FRAMES);
            for i in 0..FRAMES {
                let file = folder.join(format!("sea{i:04}.tga"));
                ensure!(
                    std::fs::metadata(&file)?.len() <= 128 * 1024,
                    "sea TGA too large"
                );
                frames.push(blue_bytes(&std::fs::read(file)?)?);
            }
            Frames::new(frames)?
        } else {
            Frames::fixture()
        };
        Ok(Self {
            sea: Rc::new(RefCell::new(Sea::new(
                Arc::new(frames),
                Parameters::default(),
            )?)),
            weather: None,
            stages: None,
            external: folder.is_some(),
        })
    }
    pub fn enable_weather(&mut self) -> Result<()> {
        self.sea.borrow_mut().enable_weather();
        self.weather = Some(weather::Session::new(self.sea.clone())?);
        Ok(())
    }
    pub fn advance(&mut self, delta_ms: u32) -> Result<()> {
        if let Some(weather) = &mut self.weather {
            weather.advance(delta_ms)?;
        }
        self.sea.borrow_mut().advance(delta_ms)
    }
    pub fn report(&self) -> Json {
        let sea = self.sea.borrow();
        let s = sea.surface();
        let hash = s
            .maps()
            .iter()
            .flat_map(|v| v.to_bits().to_le_bytes())
            .fold(14695981039346656037u64, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(1099511628211)
            });
        let mut report = json!({"frame_source":if self.external {"external_original_tga"}else{"synthetic_fixture"},"frames":64,"side":128,
            "amplitude_bits":s.parameters.amplitude.map(f32::to_bits),"scale_bits":s.parameters.scale.map(f32::to_bits),
            "frame_bits":s.animation.frame.map(f32::to_bits),"movement_bits":s.animation.movement.map(|r|r.map(f32::to_bits)),
            "camera_bits":s.camera.map(f32::to_bits),"camera_provider":"explicit_fixed_native_origin","max_distance_bits":s.parameters.max_distance.to_bits(),
            "prepared_fnv1a64":hash.to_string(),"weather_provider":"original_edit_mode_defaults","native_entity_order":false});
        if let Some(weather) = &self.weather {
            report["weather_provider"] = json!("script_attribute_callbacks");
            report["weather"] = weather.report(&sea);
        }
        if self.staged() {
            report["camera_provider"] = json!("actual_gpu_native_build_view_transposition");
            report["stages"] = self.stages_report();
        }
        report
    }
}

impl Fleet {
    pub(super) fn enable_native_sea_weather(&mut self) -> Result<()> {
        ensure!(
            self.next_frame == 0,
            "weather fixture must start before history recording"
        );
        self.native_motion
            .as_mut()
            .and_then(|m| m.rocking.as_mut())
            .and_then(|r| r.native.as_mut())
            .ok_or_else(|| anyhow::anyhow!("weather requires native sea"))?
            .enable_weather()?;
        for asset in &mut self.assets {
            asset.ship_state.particle_history = Some(Arc::new(
                storm_sim::pose_history::PoseHistory::with_pinned_math(),
            ));
        }
        Ok(())
    }
    pub(super) fn enable_native_sea(&mut self, folder: Option<&Path>) -> Result<()> {
        let sea = State::load(folder)?;
        let rocking = self
            .native_motion
            .as_mut()
            .and_then(|m| m.rocking.as_mut())
            .ok_or_else(|| anyhow::anyhow!("native SEA requires ship rocking"))?;
        rocking.native = Some(sea);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_tga_contract_uses_blue_bytes_in_file_order() {
        let mut bytes = vec![0u8; 18 + CELLS * 4 + 26];
        bytes[2] = 2;
        bytes[12] = 128;
        bytes[14] = 128;
        bytes[16] = 32;
        bytes[17] = 8;
        for i in 0..CELLS {
            bytes[18 + i * 4..22 + i * 4].copy_from_slice(&[(i % 256) as u8, 3, 7, 255]);
        }
        let expected = blue_bytes(&bytes).unwrap();
        assert_eq!(
            expected,
            (0..CELLS).map(|i| (i % 256) as u8).collect::<Vec<_>>()
        );
        bytes[17] = 40;
        assert_eq!(blue_bytes(&bytes).unwrap(), expected);
        bytes[2] = 10;
        assert!(blue_bytes(&bytes).is_err());
        bytes[2] = 2;
        bytes[0] = 1;
        assert!(blue_bytes(&bytes).is_err());
        assert!(blue_bytes(&bytes[..100]).is_err());
    }
    #[test]
    fn coupled_sea_is_partition_independent_and_released_ship_keeps_its_grid() {
        fn fleet() -> Fleet {
            let mut f = generated().unwrap();
            f.enable_native_motion();
            f.enable_native_rocking().unwrap();
            f.enable_native_sea(None).unwrap();
            f
        }
        let mut a = fleet();
        let mut b = fleet();
        a.advance_frames(3.0).unwrap();
        a.present_at(3.0, false).unwrap();
        for t in [0.25, 0.75, 1.5, 2.125, 3.0] {
            b.advance_frames(t).unwrap();
            b.present_at(t, false).unwrap();
        }
        assert_eq!(a.motion_report(), b.motion_report());
        let report = a.motion_report().unwrap();
        assert_eq!(report["rocking"]["surface"], "native_scalar_sea");
        let released = a
            .snapshots
            .iter()
            .find(|s| s["phase"] == "first_released")
            .unwrap();
        assert_eq!(
            released["native_motion"]["rocking"]["bodies"][0],
            report["rocking"]["bodies"][0]
        );
        assert_ne!(
            released["native_motion"]["rocking"]["bodies"][1],
            report["rocking"]["bodies"][1]
        );
        a.present_at(3.0, false).unwrap();
        assert_eq!(a.motion_report().unwrap(), report);
    }
    #[test]
    fn script_weather_updates_live_sea_holds_stop_and_preserves_partition_and_restart() {
        fn fleet() -> Fleet {
            let mut f = generated().unwrap();
            f.enable_native_motion();
            f.enable_native_rocking().unwrap();
            f.enable_native_sea(None).unwrap();
            f.enable_native_sea_weather().unwrap();
            assert!(f.assets.iter().all(|a| {
                a.ship_state
                    .particle_history
                    .as_ref()
                    .unwrap()
                    .uses_pinned_math()
            }));
            f
        }
        let mut a = fleet();
        let mut b = fleet();
        a.advance_frames(3.0).unwrap();
        a.present_at(3.0, false).unwrap();
        b.advance_frames(0.5).unwrap();
        let calm = b.motion_report().unwrap();
        assert_eq!(calm["rocking"]["sea"]["amplitude_bits"], json!([0, 0]));
        assert!(
            calm["rocking"]["bodies"]
                .as_array()
                .unwrap()
                .iter()
                .all(
                    |body| body["height_bits"].as_array().unwrap().iter().all(|row| row
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|v| v == 0))
                )
        );
        b.advance_frames(1.5).unwrap();
        let stopped = b.motion_report().unwrap();
        assert_eq!(stopped["rocking"]["sea"]["weather"]["stopped"], true);
        b.advance_frames(1.75).unwrap();
        let held = b.motion_report().unwrap();
        assert_eq!(
            held["rocking"]["sea"]["prepared_fnv1a64"],
            stopped["rocking"]["sea"]["prepared_fnv1a64"]
        );
        assert_eq!(
            held["rocking"]["sea"]["frame_bits"],
            stopped["rocking"]["sea"]["frame_bits"]
        );
        assert_ne!(
            held["bodies"][1]["position_bits"],
            stopped["bodies"][1]["position_bits"]
        );
        b.advance_frames(2.5).unwrap();
        let pending = b.motion_report().unwrap();
        let pending = &pending["rocking"]["sea"];
        assert_ne!(
            pending["weather"]["raw_amplitude_bits"],
            pending["amplitude_bits"]
        );
        b.advance_frames(3.0).unwrap();
        b.present_at(3.0, false).unwrap();
        assert_eq!(a.motion_report(), b.motion_report());
        let report = a.motion_report().unwrap();
        let weather = &report["rocking"]["sea"]["weather"];
        assert_eq!(weather["delivered_events"], 6);
        assert_eq!(weather["pending_events"], 0);
        assert_eq!(weather["attribute_callbacks"], 24);
        assert_eq!(
            report["rocking"]["sea"]["amplitude_bits"],
            json!([0.8_f32.to_bits(), 0.2_f32.to_bits()])
        );
        let refreshed = weather["changes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["realize_zero_requested"] == true)
            .collect::<Vec<_>>();
        assert_eq!(refreshed.len(), 5);
        assert_eq!(
            refreshed
                .iter()
                .filter(|v| v["realize_zero_applied"] == true)
                .count(),
            4
        );
        a.present_at(3.0, false).unwrap();
        assert_eq!(a.motion_report().unwrap(), report);
        let initial = fleet().motion_report().unwrap();
        assert_eq!(initial, fleet().motion_report().unwrap());
        assert_eq!(initial["rocking"]["sea"]["weather"]["delivered_events"], 0);
    }
}
