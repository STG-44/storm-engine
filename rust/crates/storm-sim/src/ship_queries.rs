//! Retained SHIP queries over live character attributes and fireplace records.
use anyhow::{Context, Result, bail, ensure};
use storm_core::{AttributeId, AttributeStore, Entity, Message, ScriptScalar};

pub const MSG_SHIP_GET_NUM_FIRE_PLACES: i32 = 50306;
pub const MSG_SHIP_GET_CHARACTER_INDEX: i32 = 50307;

pub struct ShipQueries<F> {
    character: Option<AttributeId>,
    fire_places: Vec<F>,
}
impl<F> ShipQueries<F> {
    pub fn new(fire_places: Vec<F>) -> Self {
        Self {
            character: None,
            fire_places,
        }
    }
    pub fn bind_character(
        &mut self,
        attributes: &AttributeStore,
        character: Option<AttributeId>,
    ) -> Result<()> {
        ensure!(
            character.is_none_or(|root| attributes.get(root).is_some()),
            "invalid SHIP character binding"
        );
        self.character = character;
        Ok(())
    }
    pub fn fire_places(&self) -> &[F] {
        &self.fire_places
    }
    pub fn fire_places_mut(&mut self) -> &mut Vec<F> {
        &mut self.fire_places
    }
    fn process(
        &mut self,
        message: &mut Message,
        attributes: Option<&AttributeStore>,
    ) -> Result<u64> {
        match message.long()? {
            MSG_SHIP_GET_CHARACTER_INDEX => {
                let output = message.script_variable()?;
                let store =
                    attributes.context("SHIP character query requires an attribute context")?;
                let root = self.character.context("SHIP character is not bound")?;
                ensure!(
                    store.get(root).is_some(),
                    "stale or foreign SHIP character binding"
                );
                ensure!(
                    store.child(root, "index").is_some(),
                    "SHIP character has no index attribute"
                );
                output.set(ScriptScalar::Int(
                    store.as_dword(root, Some("index"), 0) as i32
                ))?;
            }
            MSG_SHIP_GET_NUM_FIRE_PLACES => {
                let output = message.script_variable()?;
                output.set(ScriptScalar::Int(i32::try_from(self.fire_places.len())?))?;
            }
            code => bail!("SHIP message {code} is not implemented by the query subset"),
        }
        Ok(0)
    }
}
impl<F: 'static> Entity for ShipQueries<F> {
    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        self.process(message, None)
    }
    fn process_message_with_attributes(
        &mut self,
        message: &mut Message,
        attributes: &mut AttributeStore,
    ) -> Result<u64> {
        self.process(message, Some(attributes))
    }
}

#[cfg(test)]
mod tests;
