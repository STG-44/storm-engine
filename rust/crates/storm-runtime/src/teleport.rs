//! The original debug teleport entity in games with debug keys disabled.
use crate::{
    GameDirectory, NativeHost,
    config::{configuration_text, ini_field, ini_integer},
};
use anyhow::{Result, ensure};
use storm_core::{Entity, EntityInit, Message};

pub(crate) fn register(host: &mut NativeHost, game: GameDirectory) -> Result<()> {
    host.entities
        .register_class("TMPTELEPORT", move || Ok(Box::new(Actor(game.clone()))))?;
    Ok(())
}

struct Actor(GameDirectory);

impl Entity for Actor {
    fn init(&mut self, _: &mut EntityInit<'_>) -> Result<bool> {
        let config = configuration_text(&self.0)?;
        let debug_keys = ini_field(&config, "controls", "ondebugkeys")
            .map(ini_integer)
            .transpose()?
            .unwrap_or(0);
        ensure!(
            debug_keys == 0,
            "TMPTELEPORT with debug keys enabled is not ported"
        );
        Ok(true)
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        ensure!(
            message.long()? != 42222,
            "TMPTELEPORT list rendering is not ported"
        );
        Ok(0)
    }
}
