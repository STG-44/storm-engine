use crate::{AttributeId, AttributeStore, Message};
use anyhow::{Context, Result, anyhow, ensure};
use std::{
    cell::{Ref, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

const MAX_CLASSES: usize = 256;
const MAX_ENTITIES: usize = 4096;
const MAX_INIT_DEPTH: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EntityId(u64);
impl EntityId {
    pub const INVALID: Self = Self(0);
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayerType {
    #[default]
    Common,
    Execute,
    Realize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Execute,
    Realize,
}

pub trait Entity {
    fn init(&mut self, _context: &mut EntityInit<'_>) -> Result<bool> {
        Ok(true)
    }
    fn process_stage(&mut self, _stage: Stage, _delta_ms: u32) {}
    fn process_message(&mut self, _message: &mut Message) -> Result<u64> {
        Ok(0)
    }
    fn attribute_changed(
        &mut self,
        _attributes: &AttributeStore,
        _changed: AttributeId,
    ) -> Result<u32> {
        Ok(0)
    }
}

/// The instance is registered and its attributes are bound before Init.
/// Nested construction and manager operations use the same checked context.
pub struct EntityInit<'a> {
    pub id: EntityId,
    pub attributes: Option<AttributeId>,
    pub store: &'a mut AttributeStore,
    pub manager: &'a mut EntityManager,
}

type Instance = Rc<RefCell<Box<dyn Entity>>>;
type Factory = Rc<dyn Fn() -> Result<Box<dyn Entity>>>;

/// A teardown lookup remains present even during Init. Accessing the instance
/// while its callback is already running returns a checked borrow error.
pub struct EntityRef<'a> {
    id: EntityId,
    instance: &'a RefCell<Box<dyn Entity>>,
}
impl EntityRef<'_> {
    pub fn id(&self) -> EntityId {
        self.id
    }
    pub fn borrow(&self) -> Result<Ref<'_, dyn Entity>> {
        let entity = self
            .instance
            .try_borrow()
            .context("reentrant entity access")?;
        Ok(Ref::map(entity, |entity| entity.as_ref()))
    }
}

fn checked_class(class: &str) -> Result<u32> {
    ensure!(
        class.is_ascii() && !class.contains('\0') && class.len() <= 128,
        "entity class must be an ASCII engine name of at most 128 bytes"
    );
    let hash = class_hash(class);
    ensure!(hash != 0, "null entity class hash");
    Ok(hash)
}

/// MakeHashValue from core/include/vma.hpp, for ASCII engine class names.
pub fn class_hash(name: &str) -> u32 {
    let mut hash = 0_u32;
    for byte in name.bytes().take_while(|b| *b != 0) {
        hash = hash
            .wrapping_shl(4)
            .wrapping_add(u32::from(byte.to_ascii_lowercase()));
        let high = hash & 0xf000_0000;
        if high != 0 {
            hash ^= high >> 24;
            hash ^= high;
        }
    }
    hash
}

struct Entry {
    id: EntityId,
    hash: u32,
    valid: bool,
    entity: Instance,
    attributes: Option<AttributeId>,
}
#[derive(Default)]
struct Layer {
    kind: LayerType,
    frozen: bool,
    entries: Vec<(u32, EntityId)>,
}

/// IDs use a global allocation stamp in the high 32 bits and slot in the low 32.
/// Erase invalidates immediately; storage and layer membership survive until NewLifecycle.
#[derive(Default)]
pub struct EntityManager {
    entries: Vec<Option<Entry>>,
    free: Vec<usize>,
    deleted: Vec<usize>,
    layers: [Layer; 32],
    stamp: u32,
    factories: BTreeMap<String, Factory>,
    init_depth: usize,
}
impl EntityManager {
    /// Like the retained registry's first matching VMA, the first registration
    /// wins. A duplicate returns false without replacing its constructor.
    pub fn register_class(
        &mut self,
        class: &str,
        factory: impl Fn() -> Result<Box<dyn Entity>> + 'static,
    ) -> Result<bool> {
        checked_class(class)?;
        let name = class.to_ascii_lowercase();
        if self.factories.contains_key(&name) {
            return Ok(false);
        }
        ensure!(
            self.factories.len() < MAX_CLASSES,
            "entity class registry budget"
        );
        self.factories.insert(name, Rc::new(factory));
        Ok(true)
    }

    pub fn create(
        &mut self,
        class: &str,
        store: &mut AttributeStore,
        attributes: Option<AttributeId>,
    ) -> Result<EntityId> {
        checked_class(class)?;
        ensure!(
            attributes.is_none_or(|root| store.get(root).is_some()),
            "invalid attribute binding"
        );
        ensure!(self.init_depth < MAX_INIT_DEPTH, "entity Init depth budget");
        let factory = self
            .factories
            .get(&class.to_ascii_lowercase())
            .context("invalid entity name")?
            .clone();
        let entity = factory().with_context(|| format!("construct entity {class}"))?;
        let id = self.insert_box(class, entity)?;
        let entry = self.entry_mut(id).expect("inserted entity");
        entry.attributes = attributes;
        let instance = entry.entity.clone();
        self.init_depth += 1;
        let initialized = (|| {
            instance
                .try_borrow_mut()
                .context("reentrant entity Init")?
                .init(&mut EntityInit {
                    id,
                    attributes,
                    store,
                    manager: self,
                })
        })();
        self.init_depth -= 1;
        match initialized {
            Ok(true) => Ok(id),
            Ok(false) => {
                self.erase(id);
                Ok(EntityId::INVALID)
            }
            Err(error) => {
                self.erase(id);
                Err(error).with_context(|| format!("Init entity {class}"))
            }
        }
    }

    /// Inserts an already constructed object without calling Init.
    pub fn insert(&mut self, class: &str, entity: impl Entity + 'static) -> Result<EntityId> {
        self.insert_box(class, Box::new(entity))
    }
    fn insert_box(&mut self, class: &str, entity: Box<dyn Entity>) -> Result<EntityId> {
        let hash = checked_class(class)?;
        ensure!(
            !self.free.is_empty() || self.entries.len() < MAX_ENTITIES,
            "entity storage budget"
        );
        let stamp = self
            .stamp
            .checked_add(1)
            .ok_or_else(|| anyhow!("entity stamp exhausted"))?;
        let index = self.free.last().copied().unwrap_or(self.entries.len());
        let slot = u32::try_from(index)?;
        self.stamp = stamp;
        let id = EntityId((u64::from(stamp) << 32) | u64::from(slot));
        let entry = Some(Entry {
            id,
            hash,
            valid: true,
            entity: Rc::new(RefCell::new(entity)),
            attributes: None,
        });
        if self.free.pop().is_some() {
            self.entries[index] = entry;
        } else {
            self.entries.push(entry);
        }
        Ok(id)
    }
    fn entry(&self, id: EntityId) -> Option<&Entry> {
        self.entries
            .get(id.0 as u32 as usize)?
            .as_ref()
            .filter(|e| e.id == id)
    }
    fn entry_mut(&mut self, id: EntityId) -> Option<&mut Entry> {
        self.entries
            .get_mut(id.0 as u32 as usize)?
            .as_mut()
            .filter(|e| e.id == id)
    }
    pub fn is_valid(&self, id: EntityId) -> bool {
        self.entry(id).is_some_and(|e| e.valid)
    }
    /// As in GetEntityPointer, pending deletion remains accessible for teardown.
    pub fn get(&self, id: EntityId) -> Option<EntityRef<'_>> {
        self.entry(id).map(|e| EntityRef {
            id,
            instance: &e.entity,
        })
    }
    pub fn class_code(&self, id: EntityId) -> Option<u32> {
        self.entry(id).map(|e| e.hash)
    }
    /// Entity::AttributesPointer borrows an externally owned root/subtree. As
    /// with the original attribute API, pending deletion can still access it.
    pub fn bind_attributes(
        &mut self,
        id: EntityId,
        store: &AttributeStore,
        attributes: Option<AttributeId>,
    ) -> Result<bool> {
        ensure!(
            attributes.is_none_or(|root| store.get(root).is_some()),
            "invalid attribute binding"
        );
        let Some(entry) = self.entry_mut(id) else {
            return Ok(false);
        };
        entry.attributes = attributes;
        Ok(true)
    }
    pub fn attribute_root(&self, id: EntityId) -> Option<AttributeId> {
        self.entry(id)?.attributes
    }
    /// Source Entity_SetAttribute addresses a literal child name and does not
    /// automatically call AttributeChanged. Keep the zero-based return index.
    pub fn set_attribute(
        &mut self,
        id: EntityId,
        store: &mut AttributeStore,
        name: &str,
        value: Option<&str>,
    ) -> Result<Option<usize>> {
        let Some(root) = self.attribute_root(id) else {
            return Ok(None);
        };
        Ok(Some(store.set_attribute(root, name, value)?))
    }
    pub fn notify_attribute_changed(
        &mut self,
        id: EntityId,
        store: &AttributeStore,
        changed: AttributeId,
    ) -> Result<u32> {
        // CoreImpl::Entity_AttributeChanged uses the teardown lookup, rather
        // than the safe lookup used by ordinary entity messages.
        let Some(entry) = self.entry_mut(id) else {
            return Ok(0);
        };
        ensure!(
            store.get(changed).is_some(),
            "invalid changed attribute handle"
        );
        entry
            .entity
            .try_borrow_mut()
            .context("reentrant entity callback")?
            .attribute_changed(store, changed)
    }
    pub fn ids(&self, class: &str) -> Vec<EntityId> {
        let hash = class_hash(class);
        let mut ids = self
            .entries
            .iter()
            .flatten()
            .filter(|e| e.valid && e.hash == hash)
            .map(|e| e.id)
            .collect::<Vec<_>>();
        // EntityContainerCache::Add orders IDs by allocation stamp, including
        // after a newer instance reuses an earlier storage slot.
        ids.sort_by_key(|id| id.raw());
        ids
    }
    pub fn erase(&mut self, id: EntityId) {
        if let Some(entry) = self.entry_mut(id).filter(|e| e.valid) {
            entry.valid = false;
            self.deleted.push(id.0 as u32 as usize);
        }
    }
    pub fn new_lifecycle(&mut self) {
        // Never destroy/reuse an instance whose Init is still on the stack.
        // The next lifecycle boundary after Init performs the queued cleanup.
        if self.init_depth != 0 {
            return;
        }
        while let Some(index) = self.deleted.pop() {
            let entry = self.entries[index].take().expect("pending entity exists");
            for layer in &mut self.layers {
                layer.entries.retain(|(_, id)| *id != entry.id);
            }
            drop(entry);
            self.free.push(index);
        }
    }
    pub fn set_layer_type(&mut self, index: u8, kind: LayerType) -> Result<()> {
        self.layer_mut(index)?.kind = kind;
        Ok(())
    }
    pub fn freeze_layer(&mut self, index: u8, frozen: bool) -> Result<()> {
        self.layer_mut(index)?.frozen = frozen;
        Ok(())
    }
    fn layer_mut(&mut self, index: u8) -> Result<&mut Layer> {
        self.layers
            .get_mut(usize::from(index))
            .ok_or_else(|| anyhow!("layer index must be 0..31"))
    }
    pub fn add_to_layer(&mut self, index: u8, id: EntityId, priority: u32) -> Result<()> {
        ensure!(
            usize::from(index) < self.layers.len(),
            "layer index must be 0..31"
        );
        if self.entry(id).is_none() {
            return Ok(());
        }
        let entries = &mut self.layer_mut(index)?.entries;
        if !entries.iter().any(|(_, member)| *member == id) {
            let at = entries.partition_point(|(p, _)| *p <= priority);
            entries.insert(at, (priority, id));
        }
        Ok(())
    }
    pub fn remove_from_layer(&mut self, index: u8, id: EntityId) -> Result<()> {
        self.layer_mut(index)?
            .entries
            .retain(|(_, member)| *member != id);
        Ok(())
    }
    /// Ascending layer index, then stable ascending priority. Duplicates across
    /// layers intentionally survive, matching EntityManager::GetEntityIds(type).
    pub fn layer_ids(&self, kind: LayerType) -> Vec<EntityId> {
        self.layers
            .iter()
            .filter(|l| l.kind == kind && !l.frozen)
            .flat_map(|l| l.entries.iter().map(|(_, id)| *id))
            .collect()
    }
    pub fn process_stage(&mut self, stage: Stage, delta_ms: u32) -> Result<()> {
        let kind = match stage {
            Stage::Execute => LayerType::Execute,
            Stage::Realize => LayerType::Realize,
        };
        for id in self.layer_ids(kind) {
            if let Some(entry) = self.entry_mut(id).filter(|e| e.valid) {
                entry
                    .entity
                    .try_borrow_mut()
                    .context("reentrant entity callback")?
                    .process_stage(stage, delta_ms);
            }
        }
        Ok(())
    }
    /// CoreImpl::Send_Message uses the safe lookup and returns zero for invalid IDs.
    pub fn send_message(&mut self, id: EntityId, message: &mut Message) -> Result<u64> {
        match self.entry_mut(id).filter(|e| e.valid) {
            Some(entry) => entry
                .entity
                .try_borrow_mut()
                .context("reentrant entity callback")?
                .process_message(message),
            None => Ok(0),
        }
    }
}

#[cfg(test)]
mod creation_tests;
