//! The startup lifecycle of the original script-facing particle entity.
use crate::{GameDirectory, NativeHost};
use anyhow::{Result, bail};
use storm_core::{Entity, EntityInit, Message};

const PS_CREATE: i32 = 10000;
const PS_CREATEX: i32 = 10002;
const PS_CLEARALL: i32 = 10006;
const PS_DELETE: i32 = 10007;
const PS_CREATE_RIC: i32 = 10008;
const PS_PAUSEALL: i32 = 10010;
const PS_CREATIONCAPTURE_BEG: i32 = 10011;
const PS_CREATIONCAPTURE_END: i32 = 10012;
const PS_CLEAR_CAPTURED: i32 = 10013;
const PS_VALIDATE_PARTICLE: i32 = 10014;

pub(crate) fn register(host: &mut NativeHost, game: GameDirectory) -> Result<()> {
    host.entities
        .register_class("particles", move || Ok(Box::new(Actor(game.clone()))))?;
    Ok(())
}

struct Actor(GameDirectory);

impl Entity for Actor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        context.manager.add_to_layer(1, context.id, 0xfffff)?;
        context.manager.add_to_layer(0, context.id, 0)?;
        Ok(true)
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        match message.long()? {
            // These operations visit only existing systems. In particular,
            // PS_PAUSEALL is not a flag that also stops future emitters.
            PS_CLEARALL | PS_CREATIONCAPTURE_BEG | PS_CREATIONCAPTURE_END | PS_CLEAR_CAPTURED => {}
            PS_DELETE | PS_PAUSEALL => {
                message.long()?;
            }
            PS_VALIDATE_PARTICLE => {
                message.pointer()?;
            }
            PS_CREATE | PS_CREATEX | PS_CREATE_RIC => {
                let name = storm_formats::particles::profile_key(message.string()?)?;
                for _ in 0..6 {
                    message.float()?;
                }
                message.long()?;
                let path = format!("RESOURCE/Particles/{name}");
                if self.0.resolve_optional(&path)?.is_some() {
                    bail!("world particle emission is not ported for {path}");
                }
            }
            code => bail!("particles message {code} is not ported"),
        }
        Ok(0)
    }
}
