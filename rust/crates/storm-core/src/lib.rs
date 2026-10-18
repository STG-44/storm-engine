//! Native entity/message foundation, following the retained Storm core.
//! This crate does not implement the script VM or expose C++ pointer messages.
mod attributes;
mod entities;
mod events;
mod message;

pub use attributes::{AttributeId, AttributeNode, AttributeStore, format_float};
pub use entities::{
    Entity, EntityId, EntityInit, EntityManager, EntityRef, LayerType, Stage, class_hash,
};
pub use events::EventDispatcher;
pub use message::{Message, Value};

#[cfg(test)]
mod tests;
