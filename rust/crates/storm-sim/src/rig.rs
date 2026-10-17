//! Original three sail sets and per-sail membership; gathering remains kinematic.
use crate::{Result, SimError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SailMode {
    Furled,
    Normal,
    Full,
}
impl SailMode {
    /// MSG_SAIL_GET_SPEED thresholds, including the original inclusive middle band.
    pub fn from_setting(value: f64) -> Result<Self> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(SimError("invalid sail mode setting"));
        }
        Ok(if value < 0.3 {
            Self::Furled
        } else if value > 0.7 {
            Self::Full
        } else {
            Self::Normal
        })
    }
    pub fn setting(self) -> f64 {
        match self {
            Self::Furled => 0.0,
            Self::Normal => 0.5,
            Self::Full => 1.0,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Furled => "furled",
            Self::Normal => "normal",
            Self::Full => "full",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SailTier {
    Normal,
    FullOnly,
}
impl SailTier {
    /// SAIL::AddSailLabel: node/parent names, then the v/n locator override.
    pub fn from_original(group: &str, node: &str, parent: &str) -> Self {
        let mast_one = parent.strip_prefix("mast").is_some_and(|tail| {
            let tail = tail.trim_start_matches(|c: char| c.is_ascii_whitespace());
            let tail = tail.strip_prefix('+').unwrap_or(tail);
            let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
            digits.trim_start_matches('0') == "1"
        });
        if matches!(group.as_bytes().get(4), Some(b'v' | b'n'))
            || (node.starts_with("rey") && (node.as_bytes().get(4) == Some(&b'a') || mast_one))
        {
            Self::FullOnly
        } else {
            Self::Normal
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::FullOnly => "full_only",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RigLayout {
    pub normal_weight: f64,
    pub full_only_weight: f64,
}
impl Default for RigLayout {
    fn default() -> Self {
        Self {
            normal_weight: 1.0,
            full_only_weight: 1.0,
        }
    }
}
impl RigLayout {
    pub fn validate(self) -> Result<()> {
        if !self.normal_weight.is_finite()
            || self.normal_weight < 0.0
            || !self.full_only_weight.is_finite()
            || self.full_only_weight < 0.0
            || !(self.normal_weight + self.full_only_weight).is_finite()
        {
            return Err(SimError("invalid rig weights"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RigState {
    pub normal: f64,
    pub full_only: f64,
}
impl RigState {
    pub fn for_mode(mode: SailMode) -> Self {
        match mode {
            SailMode::Furled => Self::uniform(0.0),
            SailMode::Normal => Self {
                normal: 1.0,
                full_only: 0.0,
            },
            SailMode::Full => Self::uniform(1.0),
        }
    }
    pub fn uniform(value: f64) -> Self {
        Self {
            normal: value,
            full_only: value,
        }
    }
    pub fn validate(self) -> Result<()> {
        if [self.normal, self.full_only]
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err(SimError("invalid rig deployment"));
        }
        Ok(())
    }
    pub fn deployed(self, tier: SailTier) -> f64 {
        match tier {
            SailTier::Normal => self.normal,
            SailTier::FullOnly => self.full_only,
        }
    }
    pub fn fraction(self, layout: RigLayout) -> f64 {
        let total = layout.normal_weight + layout.full_only_weight;
        if total == 0.0 {
            0.0
        } else {
            (self.normal * layout.normal_weight + self.full_only * layout.full_only_weight) / total
        }
    }
    pub(crate) fn advance(&mut self, mode: SailMode) {
        let target = Self::for_mode(mode);
        let step = crate::SAIL_DEPLOY_RATE * crate::STEP_SECONDS;
        for (actual, target) in [
            (&mut self.normal, target.normal),
            (&mut self.full_only, target.full_only),
        ] {
            let distance = target - *actual;
            *actual = if distance.abs() <= step + 1e-12 {
                target
            } else {
                *actual + distance.signum() * step
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_thresholds_and_all_locator_kinds_use_the_original_node_rules() {
        for (v, mode) in [
            (0.0, SailMode::Furled),
            (0.299, SailMode::Furled),
            (0.3, SailMode::Normal),
            (0.5, SailMode::Normal),
            (0.7, SailMode::Normal),
            (0.701, SailMode::Full),
            (1.0, SailMode::Full),
        ] {
            assert_eq!(SailMode::from_setting(v).unwrap(), mode);
        }
        assert!(SailMode::from_setting(f64::NAN).is_err());
        for kind in ['s', 'g', 'f', 'd', 'x', 't', 'v', 'n'] {
            let group = format!("sail{kind}1");
            for (node, parent, extra) in [
                ("rey_a2", "mast2", true),
                ("rey_b2", "mast2", false),
                ("rey_b1", "mast1", true),
                ("rey_c1", "mast01x", true),
                ("rey_b1", "mast10", false),
                ("mast1", "root", false),
                ("hull", "mast1", false),
                ("rey_b1", "mast-1", false),
            ] {
                assert_eq!(
                    SailTier::from_original(&group, node, parent),
                    if extra || matches!(kind, 'v' | 'n') {
                        SailTier::FullOnly
                    } else {
                        SailTier::Normal
                    }
                );
            }
        }
    }
    #[test]
    fn intermediate_set_opens_complete_normal_sails_and_uses_rig_weights() {
        let layout = RigLayout {
            normal_weight: 7.0,
            full_only_weight: 3.0,
        };
        let mut state = RigState::for_mode(SailMode::Full);
        for _ in 0..480 {
            state.advance(SailMode::Normal);
        }
        assert_eq!(
            state,
            RigState {
                normal: 1.0,
                full_only: 0.0
            }
        );
        assert!((state.fraction(layout) - 0.7).abs() < 1e-12);
        for _ in 0..480 {
            state.advance(SailMode::Furled);
        }
        assert_eq!(state, RigState::default());
        for _ in 0..480 {
            state.advance(SailMode::Normal);
        }
        assert_eq!(
            state,
            RigState {
                normal: 1.0,
                full_only: 0.0
            }
        );
        assert_eq!(
            state.fraction(RigLayout {
                normal_weight: 0.0,
                full_only_weight: 0.0
            }),
            0.0
        );
    }
}
