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
}
