use storm_sim::{FlagNation, FlagSelection, FlagTexture, ShipFlagState};
use winit::keyboard::KeyCode;

pub struct Controls {
    state: ShipFlagState,
    configured: bool,
    hidden: bool,
}
impl Controls {
    pub fn new(state: Option<ShipFlagState>, hidden: bool) -> Self {
        Self {
            configured: state.is_some(),
            state: state.unwrap_or_default(),
            hidden,
        }
    }
    pub fn selection(&self) -> FlagSelection {
        if self.hidden {
            FlagSelection::Override(FlagTexture::Hidden)
        } else if self.configured {
            FlagSelection::Ship(self.state)
        } else {
            FlagSelection::Fallback
        }
    }
    pub fn key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::KeyF => {
                self.state.nation = if self.configured {
                    self.state.nation.next()
                } else {
                    FlagNation::England
                };
                self.configured = true;
            }
            KeyCode::KeyG => {
                self.state.role = self.state.role.next();
                self.configured = true;
            }
            KeyCode::KeyH => self.hidden = !self.hidden,
            KeyCode::KeyJ => {
                self.state.surrendered = !self.state.surrendered;
                self.configured = true;
            }
            _ => return false,
        }
        true
    }
    pub fn title(&self) -> String {
        if self.hidden {
            " — flags hidden".into()
        } else if !self.configured {
            " — flags fallback".into()
        } else {
            format!(
                " — flag {} / {}{}",
                self.state.nation.name(),
                self.state.role.name(),
                if self.state.surrendered {
                    " / surrendered"
                } else {
                    ""
                }
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hiding_preserves_selected_nation_role_and_surrender_state() {
        let mut controls = Controls::new(None, false);
        assert_eq!(controls.selection(), FlagSelection::Fallback);
        assert!(controls.key(KeyCode::KeyF));
        controls.key(KeyCode::KeyG);
        controls.key(KeyCode::KeyJ);
        let before = controls.selection();
        controls.key(KeyCode::KeyH);
        assert_eq!(
            controls.selection(),
            FlagSelection::Override(FlagTexture::Hidden)
        );
        controls.key(KeyCode::KeyH);
        assert_eq!(controls.selection(), before);
        assert!(!controls.key(KeyCode::KeyW));
    }
}
