//! Weak VDATA-style cells. Only stack-value temporaries live with the send.
use super::*;
use storm_core::{ScriptScalar, ScriptVariable, ScriptVariableAccess, ScriptVariableKind};

pub(crate) struct NativeArguments {
    world: World,
    temporaries: Vec<Variable>,
}
impl NativeArguments {
    pub fn new(world: &World) -> Self {
        Self {
            world: world.clone(),
            temporaries: Vec::new(),
        }
    }
    pub fn capture(&mut self, value: &Value) -> Result<ScriptVariable> {
        let variable = if let Value::Reference(reference) = value {
            Variable(
                reference
                    .0
                    .upgrade()
                    .context("uninitialized or expired ref")?,
            )
            .resolved()?
        } else {
            let temporary = Variable::new(value.clone());
            self.temporaries.push(temporary.clone());
            temporary
        };
        let access = Access::new(&variable, &self.world);
        access.validate(&variable.value()?)?;
        // Before native entry, check aref nodes as well as their owners.
        if let Value::AReference(Some(attribute)) = variable.value()? {
            self.world.attribute_id(&attribute)?;
        }
        Ok(ScriptVariable::new(access))
    }
}

struct Access {
    target: Weak<RefCell<Value>>,
    world: World,
}
impl Access {
    fn new(variable: &Variable, world: &World) -> Self {
        Self {
            target: Rc::downgrade(&variable.0),
            world: world.clone(),
        }
    }
    fn variable(&self) -> Result<Variable> {
        Variable(self.target.upgrade().context("expired script variable")?).resolved()
    }
    fn validate(&self, value: &Value) -> Result<()> {
        let world = match value {
            Value::Object(object) => Some(object.0.world.clone()),
            Value::AReference(Some(attribute)) => Some(
                attribute
                    .owner
                    .upgrade()
                    .context("expired attribute owner")?
                    .world
                    .clone(),
            ),
            Value::Array(array) => Some(array.world().clone()),
            _ => None,
        };
        if let Some(world) = world {
            ensure!(
                Rc::ptr_eq(&world.0, &self.world.0),
                "foreign attribute world"
            );
        }
        value.validate()
    }
    fn check_store(&self, store: &AttributeStore) -> Result<()> {
        // RefCell::as_ptr is used only for identity comparison, never dereferenced.
        ensure!(
            std::ptr::eq(self.world.0.attributes.as_ptr(), store),
            "foreign attribute store"
        );
        Ok(())
    }
    fn attribute(
        &self,
        value: &Value,
        store: &AttributeStore,
        initialize: bool,
    ) -> Result<AttributeId> {
        self.check_store(store)?;
        self.validate(value)?;
        let id = match value {
            Value::Object(object) => {
                if initialize {
                    object.0.initialized.set(true);
                }
                object.0.root
            }
            Value::AReference(Some(attribute)) => attribute.node,
            _ => bail!("script variable requires object or initialized aref"),
        };
        ensure!(store.get(id).is_some(), "stale attribute reference");
        Ok(id)
    }
}

impl ScriptVariableAccess for Access {
    fn same_target(&self, other: &dyn ScriptVariableAccess) -> bool {
        (other as &dyn std::any::Any)
            .downcast_ref::<Self>()
            .is_some_and(|other| Weak::ptr_eq(&self.target, &other.target))
    }
    fn read(&self) -> Result<ScriptScalar> {
        let value = self.variable()?.value()?;
        self.validate(&value)?;
        Ok(match value {
            Value::Int(n) => ScriptScalar::Int(n),
            Value::Float(n) => ScriptScalar::Float(n),
            Value::Ptr(n) => ScriptScalar::Ptr(n),
            Value::String(s) => ScriptScalar::String(s),
            _ => bail!("script variable requires a scalar; arrays require an index"),
        })
    }
    fn kind(&self) -> Result<ScriptVariableKind> {
        let value = self.variable()?.value()?;
        self.validate(&value)?;
        Ok(match value {
            Value::Int(_) => ScriptVariableKind::Int,
            Value::Float(_) => ScriptVariableKind::Float,
            Value::Ptr(_) => ScriptVariableKind::Ptr,
            Value::String(_) => ScriptVariableKind::String,
            Value::Object(_) => ScriptVariableKind::Object,
            Value::AReference(_) => ScriptVariableKind::AttributeReference,
            Value::Array(_) => ScriptVariableKind::Array,
            Value::Void => ScriptVariableKind::Void,
            Value::Reference(_) => unreachable!("resolved variable"),
        })
    }
    fn write(&self, value: ScriptScalar) -> Result<()> {
        let variable = self.variable()?;
        self.validate(&variable.value()?)?;
        let value = match value {
            ScriptScalar::Int(n) => Value::Int(n),
            ScriptScalar::Float(n) => Value::Float(n),
            ScriptScalar::Ptr(n) => Value::Ptr(n),
            ScriptScalar::String(s) => Value::String(s),
        };
        value.validate()?;
        let nan = matches!(value, Value::Float(n) if n.is_nan());
        variable.assign(value, false)?;
        // DATA::Set(float) writes the value before reporting NAN ERROR.
        ensure!(!nan, "NAN ERROR");
        Ok(())
    }
    fn elements(&self) -> Result<usize> {
        let value = self.variable()?.value()?;
        self.validate(&value)?;
        Ok(if let Value::Array(array) = value {
            array.len()
        } else {
            1
        })
    }
    fn element(&self, index: i32) -> Result<ScriptVariable> {
        let value = self.variable()?.value()?;
        self.validate(&value)?;
        let Value::Array(array) = value else {
            bail!("script variable is not an array");
        };
        Ok(ScriptVariable::new(Access::new(
            &array.element(index)?,
            &self.world,
        )))
    }
    fn attributes(&self, store: &AttributeStore) -> Result<AttributeId> {
        self.attribute(&self.variable()?.value()?, store, true)
    }
    fn entity(&self, store: &AttributeStore) -> Result<EntityId> {
        let value = self.variable()?.value()?;
        self.attribute(&value, store, false)?;
        match value {
            Value::Object(object) => Ok(object.entity()),
            Value::AReference(Some(attribute)) => Ok(attribute.entity),
            _ => unreachable!(),
        }
    }
    fn set_entity(&self, entity: EntityId, store: &mut AttributeStore) -> Result<()> {
        self.check_store(store)?;
        let variable = self.variable()?;
        let mut value = variable
            .0
            .try_borrow_mut()
            .context("reentrant variable access")?;
        self.validate(&value)?;
        match &mut *value {
            Value::Array(_) => bail!("array entity assignment requires an index"),
            Value::Object(object) => object.set_entity(entity),
            Value::AReference(Some(attribute)) => {
                ensure!(
                    store.get(attribute.node).is_some(),
                    "stale attribute reference"
                );
                attribute.entity = entity;
            }
            Value::AReference(None) => bail!("uninitialized aref"),
            _ => {
                *value = Value::Object(Object(Rc::new(Owner {
                    world: self.world.clone(),
                    root: store.create_root()?,
                    entity: Cell::new(entity),
                    initialized: Cell::new(false),
                })));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
