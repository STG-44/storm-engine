//! SHIP priority defaults, mount layer registration and ProcessStage selection.
//! Model drawing and the surrounding SHIP Init/Mount bodies remain external.
use anyhow::{Result, ensure};
use storm_core::{EntityId, EntityManager, Stage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Priorities {
    pub execute: u32,
    pub realize: u32,
}
impl Priorities {
    /// SHIP::Init reads optional signed script integers; AddToLayer receives u32.
    pub fn from_script(execute: Option<i32>, realize: Option<i32>) -> Self {
        Self {
            execute: execute.unwrap_or(2) as u32,
            realize: realize.unwrap_or(31) as u32,
        }
    }
    /// The two retained Mount registrations, in native call order. Layer types
    /// and indices are supplied by the caller, as with SEA_GET_LAYERS.
    pub fn register(
        self,
        manager: &mut EntityManager,
        id: EntityId,
        execute_layer: u8,
        realize_layer: u8,
    ) -> Result<()> {
        ensure!(
            execute_layer < 32 && realize_layer < 32,
            "invalid ship layer"
        );
        manager.add_to_layer(realize_layer, id, self.realize)?;
        manager.add_to_layer(execute_layer, id, self.execute)
    }
}

pub trait StagePort {
    fn execute(&mut self, delta_ms: u32) -> Result<()>;
    fn realize(&mut self, delta_ms: u32) -> Result<()>;
}

/// SHIP::ProcessStage delegates the delta unchanged; Execute owns its time cap.
pub fn dispatch(stage: Stage, delta_ms: u32, port: &mut impl StagePort) -> Result<()> {
    match stage {
        Stage::Execute => port.execute(delta_ms),
        Stage::Realize => port.realize(delta_ms),
    }
}

#[cfg(test)]
mod tests;
