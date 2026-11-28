//! The two retained SHIP sail-command messages, independent of cloth motion.
use anyhow::{Result, bail};
use storm_core::{Entity, Message, ScriptScalar};

pub const MSG_SHIP_SET_SAIL_STATE: i32 = 50305;
pub const MSG_SHIP_GET_SAIL_STATE: i32 = 50325;

/// SHIP starts with NAN before mounting. SetSailState stores a raw float without
/// clamping; this is a requested command, not the deployed canvas fraction.
#[derive(Clone, Copy, Debug)]
pub struct ShipSailState(f32);
impl Default for ShipSailState {
    fn default() -> Self {
        Self(f32::NAN)
    }
}
impl ShipSailState {
    pub fn get(self) -> f32 {
        self.0
    }
    pub fn set(&mut self, value: f32) {
        self.0 = value;
    }
}
impl Entity for ShipSailState {
    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        match message.long()? {
            MSG_SHIP_SET_SAIL_STATE => self.set(message.float()?),
            MSG_SHIP_GET_SAIL_STATE => message
                .script_variable()?
                .set(ScriptScalar::Float(self.get()))?,
            code => bail!("SHIP message {code} is not implemented by the sail-command subset"),
        }
        Ok(0)
    }
}

impl Entity for crate::Sailing {
    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        self.sail_request.process_message(message)
    }
}

#[cfg(test)]
mod tests;
