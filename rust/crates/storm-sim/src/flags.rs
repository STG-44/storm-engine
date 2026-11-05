//! Deterministic ship-flag rules from sd-teho-public's procGetRiggingData.
//! Character/group lookup remains with the caller; no script VM runs here.
use crate::{Result, SimError};
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlagNation {
    England,
    France,
    Spain,
    Holland,
    Pirate,
}
impl FlagNation {
    pub const ALL: [Self; 5] = [
        Self::England,
        Self::France,
        Self::Spain,
        Self::Holland,
        Self::Pirate,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::England => "england",
            Self::France => "france",
            Self::Spain => "spain",
            Self::Holland => "holland",
            Self::Pirate => "pirate",
        }
    }
    pub fn next(self) -> Self {
        Self::ALL[(Self::ALL.iter().position(|&n| n == self).unwrap() + 1) % Self::ALL.len()]
    }
}
impl FromStr for FlagNation {
    type Err = SimError;
    fn from_str(s: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|n| n.name() == s)
            .ok_or(SimError(
                "flag nation must be england, france, spain, holland or pirate",
            ))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShipFlagRole {
    Commander,
    Naval,
    Merchant,
    #[default]
    Personal,
}
impl ShipFlagRole {
    pub const ALL: [Self; 4] = [Self::Personal, Self::Commander, Self::Naval, Self::Merchant];
    pub fn name(self) -> &'static str {
        match self {
            Self::Commander => "commander",
            Self::Naval => "naval",
            Self::Merchant => "merchant",
            Self::Personal => "personal",
        }
    }
    pub fn next(self) -> Self {
        Self::ALL[(Self::ALL.iter().position(|&r| r == self).unwrap() + 1) % Self::ALL.len()]
    }
    fn row(self) -> u32 {
        match self {
            Self::Commander => 1,
            Self::Naval => 2,
            Self::Merchant => 3,
            Self::Personal => 6,
        }
    }
}
impl FromStr for ShipFlagRole {
    type Err = SimError;
    fn from_str(s: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|r| r.name() == s)
            .ok_or(SimError(
                "flag role must be personal, commander, naval or merchant",
            ))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlagCell {
    pub column: u32,
    pub row: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlagTexture {
    Cell(FlagCell),
    Hidden,
}
impl FlagTexture {
    /// C++ event fallback, scalar column, or [column, row]. Either -1 hides.
    pub fn from_script_reply(reply: Option<&[i32]>) -> Result<Self> {
        let (column, row) = match reply {
            None => (0, 0),
            Some([column]) => (*column, 0),
            Some([column, row]) => (*column, *row),
            _ => return Err(SimError("flag reply must have one or two integers")),
        };
        if column == -1 || row == -1 {
            return Ok(Self::Hidden);
        }
        if column < 0 || row < 0 {
            return Err(SimError("invalid negative flag cell"));
        }
        Ok(Self::Cell(FlagCell {
            column: column as u32,
            row: row as u32,
        }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShipFlagState {
    pub nation: FlagNation,
    pub role: ShipFlagRole,
    pub surrendered: bool,
    /// Explicit character/group result; the preview does not randomly choose it.
    pub pirate_variant: u32,
}
impl Default for ShipFlagState {
    fn default() -> Self {
        Self {
            nation: FlagNation::Holland,
            role: ShipFlagRole::Personal,
            surrendered: false,
            pirate_variant: 0,
        }
    }
}
impl ShipFlagState {
    pub fn texture(self, special: bool) -> FlagTexture {
        let column = match self.nation {
            FlagNation::Holland => 0,
            FlagNation::England => 1,
            FlagNation::France => 2,
            FlagNation::Spain => 3,
            FlagNation::Pirate => self.pirate_variant,
        };
        let row = if self.nation == FlagNation::Pirate {
            7
        } else if !special {
            0
        } else if self.surrendered {
            4
        } else {
            self.role.row()
        };
        FlagTexture::Cell(FlagCell { column, row })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlagSelection {
    #[default]
    Fallback,
    Ship(ShipFlagState),
    Override(FlagTexture),
}
impl FlagSelection {
    pub fn texture(self, special: bool) -> FlagTexture {
        match self {
            Self::Fallback => FlagTexture::Cell(FlagCell::default()),
            Self::Ship(state) => state.texture(special),
            Self::Override(texture) => texture,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cell(c: u32, r: u32) -> FlagTexture {
        FlagTexture::Cell(FlagCell { column: c, row: r })
    }
    #[test]
    fn nations_map_to_atlas_columns_rather_than_the_original_nation_ids() {
        for (nation, column) in [
            (FlagNation::Holland, 0),
            (FlagNation::England, 1),
            (FlagNation::France, 2),
            (FlagNation::Spain, 3),
        ] {
            let state = ShipFlagState {
                nation,
                ..Default::default()
            };
            assert_eq!(state.texture(false), cell(column, 0));
            assert_eq!(state.texture(true), cell(column, 6));
            assert_eq!(nation.name().parse::<FlagNation>().unwrap(), nation);
        }
        let mut nation = FlagNation::England;
        for _ in 0..5 {
            nation = nation.next();
        }
        assert_eq!(nation, FlagNation::England);
        assert!("unknown".parse::<FlagNation>().is_err());
    }
    #[test]
    fn special_flags_follow_role_and_surrender_but_pirates_keep_their_flag() {
        for (role, row) in [
            (ShipFlagRole::Commander, 1),
            (ShipFlagRole::Naval, 2),
            (ShipFlagRole::Merchant, 3),
            (ShipFlagRole::Personal, 6),
        ] {
            let state = ShipFlagState {
                role,
                ..Default::default()
            };
            assert_eq!(state.texture(false), cell(0, 0));
            assert_eq!(state.texture(true), cell(0, row));
            let surrendered = ShipFlagState {
                surrendered: true,
                ..state
            };
            assert_eq!(surrendered.texture(true), cell(0, 4));
            assert_eq!(surrendered.texture(false), cell(0, 0));
            let pirate = ShipFlagState {
                nation: FlagNation::Pirate,
                pirate_variant: 2,
                ..surrendered
            };
            assert_eq!(pirate.texture(true), cell(2, 7));
            assert_eq!(pirate.texture(false), cell(2, 7));
        }
    }
    #[test]
    fn event_replies_preserve_fallback_scalar_pair_and_hidden_semantics() {
        assert_eq!(FlagTexture::from_script_reply(None).unwrap(), cell(0, 0));
        assert_eq!(
            FlagTexture::from_script_reply(Some(&[3])).unwrap(),
            cell(3, 0)
        );
        assert_eq!(
            FlagTexture::from_script_reply(Some(&[2, 7])).unwrap(),
            cell(2, 7)
        );
        for values in [&[-1, 0][..], &[0, -1], &[-1]] {
            assert_eq!(
                FlagTexture::from_script_reply(Some(values)).unwrap(),
                FlagTexture::Hidden
            );
        }
        for values in [&[][..], &[-2, 0], &[1, 2, 3]] {
            assert!(FlagTexture::from_script_reply(Some(values)).is_err());
        }
    }
}
