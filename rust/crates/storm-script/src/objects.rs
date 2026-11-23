//! Checked ownership replaces the original VM's raw DATA/ATTRIBUTES pointers.
use crate::{Host, Type, Value};
use anyhow::{Context, Result, bail, ensure};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
};
use storm_core::{AttributeId, AttributeStore, EntityId};

const MAX_NODES: usize = 32768;
const MAX_NAMES: usize = 8192;
pub(crate) mod native;

#[derive(Clone, Default)]
pub struct World(Rc<Storage>);
#[derive(Default)]
struct Storage {
    attributes: RefCell<AttributeStore>,
    limits: Cell<AttributeLimits>,
    owners: RefCell<BTreeMap<AttributeId, Weak<Owner>>>,
    released: RefCell<Vec<AttributeId>>,
}
#[derive(Clone, Copy)]
struct AttributeLimits {
    nodes: usize,
    names: usize,
}
impl Default for AttributeLimits {
    fn default() -> Self {
        Self {
            nodes: MAX_NODES,
            names: MAX_NAMES,
        }
    }
}
struct Owner {
    world: World,
    root: AttributeId,
    entity: Cell<EntityId>,
    initialized: Cell<bool>,
}
impl Drop for Owner {
    fn drop(&mut self) {
        // Destruction may occur during an entity notification with a read borrow.
        // Drain on the next operation, never re-enter RefCell in a destructor.
        self.world.0.released.borrow_mut().push(self.root);
    }
}

#[derive(Clone)]
pub struct Object(Rc<Owner>);
#[derive(Clone)]
pub struct AttributeReference {
    owner: Weak<Owner>,
    node: AttributeId,
    entity: EntityId,
}
#[derive(Clone, Default)]
pub struct Reference(Weak<RefCell<Value>>);
#[derive(Clone)]
pub struct Variable(Rc<RefCell<Value>>);

macro_rules! pointer_identity {
    ($kind:ty, $same:expr) => {
        impl std::fmt::Debug for $kind {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(stringify!($kind))
            }
        }
        impl PartialEq for $kind {
            fn eq(&self, other: &Self) -> bool {
                ($same)(self, other)
            }
        }
    };
}
pointer_identity!(Object, |a: &Object, b: &Object| Rc::ptr_eq(&a.0, &b.0));
pointer_identity!(Reference, |a: &Reference, b: &Reference| Weak::ptr_eq(
    &a.0, &b.0
));
pointer_identity!(
    AttributeReference,
    |a: &AttributeReference, b: &AttributeReference| a.node == b.node
        && Weak::ptr_eq(&a.owner, &b.owner)
        && (a.node != AttributeId::INVALID || a.entity == b.entity)
);

impl World {
    /// Configured games keep their ship, world-map, location and quest tables
    /// in the same attribute store through startup and play.
    pub fn use_game_limits(&self) {
        self.0.limits.set(AttributeLimits {
            nodes: 524_288,
            names: 16_384,
        });
    }

    fn prepare(&self, nodes: usize, names: usize) -> Result<()> {
        let mut store = self
            .0
            .attributes
            .try_borrow_mut()
            .context("reentrant attribute access")?;
        for id in self.0.released.borrow_mut().drain(..) {
            self.0.owners.borrow_mut().remove(&id);
            store.drop_root(id)?;
        }
        let limits = self.0.limits.get();
        ensure!(
            store.len() + nodes <= limits.nodes,
            "script attribute node budget"
        );
        ensure!(
            store.name_count() + names <= limits.names,
            "script attribute name budget"
        );
        Ok(())
    }
    pub fn object(&self, entity: EntityId) -> Result<Object> {
        self.prepare(1, 1)?;
        let root = self.0.attributes.borrow_mut().create_root()?;
        Ok(self.owned_object(root, entity, false))
    }
    fn owned_object(&self, root: AttributeId, entity: EntityId, initialized: bool) -> Object {
        let owner = Rc::new(Owner {
            world: self.clone(),
            root,
            entity: Cell::new(entity),
            initialized: Cell::new(initialized),
        });
        self.0
            .owners
            .borrow_mut()
            .insert(root, Rc::downgrade(&owner));
        Object(owner)
    }
    pub(crate) fn entity_reference(
        &self,
        entity: EntityId,
        binding: Option<AttributeId>,
    ) -> Result<AttributeReference> {
        if let Some(node) = binding {
            let owner = self.read(|store| {
                let mut root = node;
                while let Some(parent) = store.get(root)?.parent() {
                    root = parent;
                }
                self.0.owners.borrow().get(&root).cloned()
            })?;
            if let Some(owner) = owner {
                return Ok(AttributeReference {
                    owner,
                    node,
                    entity,
                });
            }
        }
        Ok(AttributeReference {
            owner: Weak::new(),
            node: AttributeId::INVALID,
            entity,
        })
    }
    /// Read attributes from the same store that native entities borrow.
    pub fn read<T>(&self, action: impl FnOnce(&AttributeStore) -> T) -> Result<T> {
        self.prepare(0, 0)?;
        Ok(action(&self.0.attributes.borrow()))
    }
    /// Run a native frame callback with the same checked attribute budget as messages.
    pub fn update<T>(&self, action: impl FnOnce(&mut AttributeStore) -> Result<T>) -> Result<T> {
        self.initialize(None, |store, _| action(store))
    }

    pub fn live_nodes(&self) -> Result<usize> {
        self.read(|s| s.len())
    }
    pub(crate) fn attribute_id(&self, attribute: &AttributeReference) -> Result<AttributeId> {
        let owner = attribute.owner()?;
        ensure!(
            Rc::ptr_eq(&self.0, &owner.world.0),
            "foreign attribute world"
        );
        Ok(attribute.node)
    }
    /// A native callback can populate the selected attribute store directly.
    /// The embedding host is trusted native code; script budgets are checked
    /// again on return. Re-entry and foreign roots fail before the callback.
    pub(crate) fn initialize<T>(
        &self,
        attributes: Option<&AttributeReference>,
        action: impl FnOnce(&mut AttributeStore, Option<AttributeId>) -> Result<T>,
    ) -> Result<T> {
        let owner = attributes.map(AttributeReference::owner).transpose()?;
        if let Some(owner) = &owner {
            ensure!(
                Rc::ptr_eq(&self.0, &owner.world.0),
                "foreign attribute world"
            );
        }
        self.prepare(0, 0)?;
        let mut store = self
            .0
            .attributes
            .try_borrow_mut()
            .context("reentrant attribute access")?;
        let result = action(&mut store, attributes.map(|a| a.node));
        let limits = self.0.limits.get();
        ensure!(store.len() <= limits.nodes, "script attribute node budget");
        ensure!(
            store.name_count() <= limits.names,
            "script attribute name budget"
        );
        result
    }
}

impl Object {
    pub(crate) fn set_entity(&self, entity: EntityId) {
        self.0.entity.set(entity);
    }
    pub fn entity(&self) -> EntityId {
        self.0.entity.get()
    }
    pub fn root(&self) -> AttributeReference {
        self.0.initialized.set(true);
        AttributeReference {
            owner: Rc::downgrade(&self.0),
            node: self.0.root,
            entity: self.entity(),
        }
    }
    pub(crate) fn copied(&self) -> Result<Self> {
        let world = &self.0.world;
        let count = world.read(|s| s.subtree_len(self.0.root))??;
        world.prepare(count, 0)?;
        let root = world.0.attributes.borrow_mut().copy(self.0.root)?;
        Ok(world.owned_object(root, self.entity(), true))
    }
    pub(crate) fn replace(&self, source: &Self) -> Result<()> {
        ensure!(
            Rc::ptr_eq(&self.0.world.0, &source.0.world.0),
            "foreign attribute world"
        );
        // DATA::Copy leaves an existing destination tree intact if a source
        // reached through ref has never allocated its attributes. Loading a
        // plain object, however, creates an initialized deep copy on the stack.
        if source.0.initialized.get() {
            self.root().replace(&source.root())?;
        }
        self.0.initialized.set(true);
        self.0.entity.set(source.entity());
        Ok(())
    }
}

impl AttributeReference {
    pub(crate) fn entity(&self) -> Result<EntityId> {
        if self.node != AttributeId::INVALID {
            self.owner()?;
        }
        Ok(self.entity)
    }
    fn owner(&self) -> Result<Rc<Owner>> {
        let owner = self.owner.upgrade().context("expired attribute owner")?;
        owner.world.prepare(0, 0)?;
        ensure!(
            owner.world.0.attributes.borrow().get(self.node).is_some(),
            "stale attribute reference"
        );
        Ok(owner)
    }
    pub(crate) fn walk(&self, segments: &[(String, bool)], create: bool) -> Result<Self> {
        let owner = self.owner()?;
        let names = segments
            .iter()
            .filter(|(_, dynamic)| create || *dynamic)
            .map(|(s, dynamic)| if *dynamic { s.split('.').count() } else { 1 })
            .sum();
        // Dynamic lookup interns names even when the requested path is missing.
        owner.world.prepare(if create { names } else { 0 }, names)?;
        let mut store = owner.world.0.attributes.borrow_mut();
        let mut current = self.node;
        for (name, dynamic) in segments {
            current = if *dynamic {
                if create {
                    store.create_path(current, name)?
                } else {
                    store
                        .find_path(current, name)?
                        .context("missing dynamic attribute")?
                }
            } else if create {
                store.verify_attribute(current, name)?
            } else {
                store.child(current, name).context("missing attribute")?
            };
        }
        Ok(Self {
            owner: self.owner.clone(),
            node: current,
            entity: self.entity,
        })
    }
    pub fn value(&self) -> Result<String> {
        let owner = self.owner()?;
        let store = owner.world.0.attributes.borrow();
        Ok(store
            .get(self.node)
            .expect("validated node")
            .value()
            .unwrap_or("")
            .to_owned())
    }
    pub(crate) fn scalar_value(&self) -> Result<String> {
        let owner = self.owner()?;
        let store = owner.world.0.attributes.borrow();
        Ok(store
            .get(self.node)
            .expect("validated node")
            .value()
            .context("attribute has no scalar value")?
            .to_owned())
    }
    /// Bounded native path creation; does not notify an entity by itself.
    pub fn create_path(&self, path: &str) -> Result<Self> {
        Value::String(path.into()).validate()?;
        self.walk(&[(path.into(), true)], true)
    }
    pub fn delete_path(&self, path: &str) -> Result<()> {
        let path = Value::String(path.into());
        path.validate()?;
        self.builtin("deleteattribute", Some(&path))?;
        Ok(())
    }
    /// Native population follows ATTRIBUTES::SetValue: explicit notification is separate.
    pub fn set_value(&self, value: &str) -> Result<()> {
        Value::String(value.to_owned()).validate()?;
        let owner = self.owner()?;
        owner
            .world
            .0
            .attributes
            .borrow_mut()
            .set_value(self.node, Some(value))?;
        Ok(())
    }
    pub fn write(&self, value: &str, host: &mut impl Host) -> Result<()> {
        self.set_value(value)?;
        let owner = self.owner()?;
        // Original VM calls Entity_AttributeChanged only for attribute assignment.
        host.attribute_changed(self.entity, &owner.world.0.attributes.borrow(), self.node)
    }
    pub(crate) fn replace(&self, source: &Self) -> Result<()> {
        let destination = self.owner()?;
        let source_owner = source.owner()?;
        ensure!(
            Rc::ptr_eq(&destination.world.0, &source_owner.world.0),
            "foreign attribute world"
        );
        let world = &destination.world;
        // replace_with_copy snapshots the source before freeing destination children.
        let count = world.read(|s| s.subtree_len(source.node))??;
        world.prepare(count, 0)?;
        world
            .0
            .attributes
            .borrow_mut()
            .replace_with_copy(self.node, source.node)
    }
    pub(crate) fn builtin(&self, name: &str, argument: Option<&Value>) -> Result<Value> {
        if name == "checkattribute" && self.node == AttributeId::INVALID {
            return Ok(Value::Int(0));
        }
        let owner = self.owner()?;
        if matches!(name, "checkattribute" | "deleteattribute") {
            owner.world.prepare(
                0,
                argument.context("missing path")?.text()?.split('.').count(),
            )?;
        }
        let mut store = owner.world.0.attributes.borrow_mut();
        Ok(match name {
            "checkattribute" | "deleteattribute" => {
                let path = argument.context("missing path")?.text()?;
                let found = store.find_path(self.node, path)?;
                if name == "checkattribute" {
                    Value::Int(i32::from(found.is_some()))
                } else {
                    if let Some(id) = found {
                        store.delete_class(self.node, id)?;
                    }
                    Value::Void
                }
            }
            "getattributesnum" => {
                Value::Int(store.get(self.node).expect("node").children().len() as i32)
            }
            "getattributen" => {
                let n = argument.context("missing index")?.integer()?;
                ensure!(n >= 0, "negative attribute index");
                let node = store
                    .child_at(self.node, n as usize)
                    .context("attribute index out of range")?;
                // GetAttributeN creates a new aref without copying an entity ID.
                Value::AReference(Some(Self {
                    owner: self.owner.clone(),
                    node,
                    entity: EntityId::default(),
                }))
            }
            "getattributename" => Value::String(store.name(self.node).expect("node").to_owned()),
            "getattributevalue" => Value::String(
                store
                    .get(self.node)
                    .expect("node")
                    .value()
                    .unwrap_or("")
                    .to_owned(),
            ),
            _ => bail!("unknown attribute builtin"),
        })
    }
    pub fn inspect<T>(&self, f: impl FnOnce(&AttributeStore, AttributeId) -> T) -> Result<T> {
        let owner = self.owner()?;
        owner.world.read(|store| f(store, self.node))
    }
}

impl Variable {
    pub fn new(value: Value) -> Self {
        Self(Rc::new(RefCell::new(value)))
    }
    pub fn reference(&self) -> Value {
        Value::Reference(Reference(Rc::downgrade(&self.0)))
    }
    pub(crate) fn address(&self) -> Result<Value> {
        // SETREF binds GetVarPointer now, before subsequent arguments run.
        Ok(self.resolved()?.reference())
    }
    pub fn value(&self) -> Result<Value> {
        self.resolved()?
            .0
            .try_borrow()
            .map(|v| v.clone())
            .context("reentrant variable access")
    }
    fn resolved(&self) -> Result<Self> {
        let mut current = self.clone();
        for _ in 0..128 {
            let next = match &*current
                .0
                .try_borrow()
                .context("reentrant variable access")?
            {
                Value::Reference(reference) => Some(
                    reference
                        .0
                        .upgrade()
                        .context("uninitialized or expired ref")?,
                ),
                _ => None,
            };
            if let Some(next) = next {
                ensure!(!Rc::ptr_eq(&current.0, &next), "cyclic ref");
                current = Self(next);
            } else {
                return Ok(current);
            }
        }
        bail!("reference depth or cycle limit")
    }
    pub(crate) fn is_reference(&self) -> bool {
        matches!(*self.0.borrow(), Value::Reference(_))
    }
    pub(crate) fn load(&self) -> Result<Value> {
        Ok(match &*self.0.borrow() {
            Value::Object(object) => Value::Object(object.copied()?),
            Value::Array(_) => bail!("array value requires an explicit ref"),
            value => value.clone(),
        })
    }
    pub(crate) fn bind(&self, source: &Self) -> Result<()> {
        ensure!(
            matches!(*self.0.borrow(), Value::Reference(_)),
            "makeref destination is not ref"
        );
        let source = source.resolved()?;
        ensure!(!Rc::ptr_eq(&self.0, &source.0), "cyclic ref");
        *self.0.borrow_mut() = source.reference();
        Ok(())
    }
    pub(crate) fn bind_attribute(&self, value: AttributeReference) -> Result<()> {
        let target = self.resolved()?;
        ensure!(
            matches!(*target.0.borrow(), Value::AReference(_)),
            "makearef destination is not aref"
        );
        *target.0.borrow_mut() = Value::AReference(Some(value));
        Ok(())
    }
    // A MOVE conversion leaves its converted source in the compiler's EX.
    pub(crate) fn assign(&self, value: Value, convert: bool) -> Result<Option<Value>> {
        if matches!(*self.0.borrow(), Value::Reference(_)) && matches!(value, Value::Reference(_)) {
            *self.0.borrow_mut() = value;
            return Ok(None);
        }
        let target = self.resolved()?;
        let value = value.resolved()?;
        ensure!(
            !matches!(*target.0.borrow(), Value::Array(_)) && !matches!(value, Value::Array(_)),
            "whole-array assignment is not supported"
        );
        let kind = target.0.borrow().kind();
        let converted = convert && value.kind() != kind;
        let value = if converted {
            value.convert(kind)?
        } else {
            value
        };
        if let (Value::Object(destination), Value::Object(source)) = (&*target.0.borrow(), &value) {
            destination.replace(source)?;
            return Ok(None);
        }
        ensure!(value.kind() != Type::Void, "void assignment");
        let expression = converted.then(|| value.clone());
        *target.0.borrow_mut() = value;
        Ok(expression)
    }
    pub(crate) fn increment(&self, decrement: bool) -> Result<()> {
        let target = self.resolved()?;
        let mut cell = target.0.borrow_mut();
        let Value::Int(value) = &mut *cell else {
            bail!("increment requires integer")
        };
        *value = value.wrapping_add(if decrement { -1 } else { 1 });
        Ok(())
    }
}

impl Reference {
    pub(crate) fn set_entity(&self, entity: EntityId) -> Result<()> {
        let variable =
            Variable(self.0.upgrade().context("uninitialized or expired ref")?).resolved()?;
        let mut value = variable
            .0
            .try_borrow_mut()
            .context("reentrant variable access")?;
        match &mut *value {
            Value::Object(object) => object.set_entity(entity),
            Value::AReference(Some(attribute)) => {
                attribute.owner()?;
                attribute.entity = entity;
            }
            _ => bail!("CreateEntity destination requires object or initialized aref"),
        }
        Ok(())
    }
    pub(crate) fn value(&self) -> Result<Value> {
        Variable(self.0.upgrade().context("uninitialized or expired ref")?).value()
    }
    pub(crate) fn store(&self, value: Value) -> Result<()> {
        Variable(self.0.upgrade().context("uninitialized or expired ref")?).assign(value, false)?;
        Ok(())
    }
    pub(crate) fn convert(&self, kind: Type) -> Result<Value> {
        let value = self.value()?.convert(kind)?;
        self.store(value.clone())?;
        Ok(value)
    }
}
