//! Checked native access to script DATA cells without depending on the VM.
use crate::{AttributeId, AttributeStore, EntityId};
use anyhow::{Result, bail};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum ScriptScalar {
    Int(i32),
    Float(f32),
    Ptr(u64),
    String(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptVariableKind {
    Int,
    Float,
    Ptr,
    String,
    Object,
    AttributeReference,
    Array,
    Void,
}

/// An embedding supplies checked access, including variable lifetime checks.
/// Scalar setters retype DATA; they do not perform script assignment conversion.
pub trait ScriptVariableAccess: std::any::Any {
    fn same_target(&self, _other: &dyn ScriptVariableAccess) -> bool {
        false
    }
    fn read(&self) -> Result<ScriptScalar>;
    fn kind(&self) -> Result<ScriptVariableKind> {
        Ok(match self.read()? {
            ScriptScalar::Int(_) => ScriptVariableKind::Int,
            ScriptScalar::Float(_) => ScriptVariableKind::Float,
            ScriptScalar::Ptr(_) => ScriptVariableKind::Ptr,
            ScriptScalar::String(_) => ScriptVariableKind::String,
        })
    }
    fn write(&self, value: ScriptScalar) -> Result<()>;
    fn elements(&self) -> Result<usize> {
        self.kind()?;
        Ok(1)
    }
    fn element(&self, _index: i32) -> Result<ScriptVariable> {
        bail!("script variable is not an array")
    }
    fn attributes(&self, _store: &AttributeStore) -> Result<AttributeId> {
        bail!("script variable attribute access is not bound")
    }
    fn entity(&self, _store: &AttributeStore) -> Result<EntityId> {
        bail!("script variable entity access is not bound")
    }
    fn set_entity(&self, _id: EntityId, _store: &mut AttributeStore) -> Result<()> {
        bail!("script variable entity assignment is not bound")
    }
}

/// Cloning a handle must not extend the underlying VM variable's lifetime.
#[derive(Clone)]
pub struct ScriptVariable(Rc<dyn ScriptVariableAccess>);
impl ScriptVariable {
    pub fn new(access: impl ScriptVariableAccess + 'static) -> Self {
        Self(Rc::new(access))
    }
    pub fn read(&self) -> Result<ScriptScalar> {
        self.0.read()
    }
    pub fn kind(&self) -> Result<ScriptVariableKind> {
        self.0.kind()
    }
    pub fn set(&self, value: ScriptScalar) -> Result<()> {
        self.0.write(value)
    }
    pub fn integer(&self) -> Result<i32> {
        match self.read()? {
            ScriptScalar::Int(n) => Ok(n),
            _ => bail!("script variable requires int"),
        }
    }
    pub fn float(&self) -> Result<f32> {
        match self.read()? {
            ScriptScalar::Float(n) => Ok(n),
            _ => bail!("script variable requires float"),
        }
    }
    pub fn pointer(&self) -> Result<u64> {
        match self.read()? {
            ScriptScalar::Ptr(n) => Ok(n),
            _ => bail!("script variable requires ptr"),
        }
    }
    pub fn string(&self) -> Result<String> {
        match self.read()? {
            ScriptScalar::String(s) => Ok(s),
            _ => bail!("script variable requires string"),
        }
    }
    pub fn elements(&self) -> Result<usize> {
        self.0.elements()
    }
    pub fn element(&self, index: i32) -> Result<Self> {
        self.0.element(index)
    }
    pub fn attributes(&self, store: &AttributeStore) -> Result<AttributeId> {
        self.0.attributes(store)
    }
    pub fn entity(&self, store: &AttributeStore) -> Result<EntityId> {
        self.0.entity(store)
    }
    pub fn set_entity(&self, id: EntityId, store: &mut AttributeStore) -> Result<()> {
        self.0.set_entity(id, store)
    }
}
impl std::fmt::Debug for ScriptVariable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScriptVariable")
    }
}
impl PartialEq for ScriptVariable {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0.same_target(&*other.0)
    }
}
