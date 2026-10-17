use storm_render::model::{ShipAsset, ShipState};
use winit::keyboard::KeyCode;
pub fn key(asset: &ShipAsset, state: &mut ShipState, key: KeyCode, seconds: f32) -> bool {
    match key {
        KeyCode::KeyC => state.crew_visible = !state.crew_visible,
        KeyCode::KeyB => {
            if asset.fires.is_empty() {
                return false;
            }
            state.fire_started = if state.fire_started.is_some() {
                None
            } else {
                Some(seconds)
            }
        }
        KeyCode::KeyM => {
            let Some(id) = asset
                .masts()
                .into_iter()
                .find(|i| !state.broken_masts.contains_key(i))
            else {
                return false;
            };
            state.broken_masts.insert(id, seconds);
        }
        KeyCode::KeyP => {
            let Some(name) = asset
                .hulls()
                .into_iter()
                .find(|n| !state.broken_hulls.contains_key(n))
            else {
                return false;
            };
            state.broken_hulls.insert(name, seconds);
        }
        KeyCode::Digit0 => *state = ShipState::default(),
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controls_damage_only_existing_parts_and_repair_restores_full_area() {
        let asset = storm_render::model::ModelAsset::ship_demo().unwrap();
        let mut state = ShipState::default();
        for code in [KeyCode::KeyM, KeyCode::KeyP, KeyCode::KeyB, KeyCode::KeyC] {
            assert!(key(&asset.ship, &mut state, code, 2.0));
        }
        state.validate(&asset.ship).unwrap();
        assert!(!state.crew_visible);
        assert_eq!(state.fire_started, Some(2.0));
        assert!(!state.broken_masts.is_empty());
        assert!(!state.broken_hulls.is_empty());
        assert!(asset.rig_layout_at(&state, 3.0).missing_weight > 0.0);
        key(&asset.ship, &mut state, KeyCode::Digit0, 3.0);
        assert!(
            state.broken_masts.is_empty()
                && state.broken_hulls.is_empty()
                && state.fire_started.is_none()
                && state.crew_visible
        );
        assert_eq!(asset.rig_layout_at(&state, 3.0).missing_weight, 0.0);
    }
    #[test]
    fn holes_and_mast_loss_reduce_the_same_sailing_session_and_repair_restores_drive() {
        use clap::Parser;
        let options = crate::Options::try_parse_from([
            "storm-demo",
            "--ship-demo",
            "--sail",
            "--wave-height",
            "0",
            "--time",
            "40",
        ])
        .unwrap();
        let asset = storm_render::model::ModelAsset::ship_demo().unwrap();
        let mut state = ShipState::default();
        let healthy = crate::sailing::Session::new(&options, asset.rig_layout_at(&state, 0.0))
            .unwrap()
            .unwrap();
        asset
            .sail_volley(
                &mut state,
                3,
                storm_render::model::SailWind::default(),
                0.0,
                storm_sim::RigState::uniform(1.0),
            )
            .unwrap();
        let holed = crate::sailing::Session::new(&options, asset.rig_layout_at(&state, 0.0))
            .unwrap()
            .unwrap();
        assert!(holed.simulation.state().speed_knots() < healthy.simulation.state().speed_knots());
        key(&asset.ship, &mut state, KeyCode::KeyM, 0.0);
        key(&asset.ship, &mut state, KeyCode::KeyM, 0.0);
        let dismasted = crate::sailing::Session::new(&options, asset.rig_layout_at(&state, 0.0))
            .unwrap()
            .unwrap();
        assert_eq!(dismasted.simulation.sail_deployed(), 0.0);
        assert_eq!(dismasted.simulation.state().speed_knots(), 0.0);
        key(&asset.ship, &mut state, KeyCode::Digit0, 0.0);
        let repaired = crate::sailing::Session::new(&options, asset.rig_layout_at(&state, 0.0))
            .unwrap()
            .unwrap();
        assert_eq!(healthy.report(), repaired.report());
    }
}
