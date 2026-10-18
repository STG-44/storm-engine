//! The original entity lookup/deletion and compiler notification switch.
use crate::{AttributeReference, Execution, Host, Type, Value};
use anyhow::{Result, bail, ensure};
use storm_core::EntityId;

pub(crate) fn signature(name: &str) -> Option<(usize, Type)> {
    Some(match name {
        "isentity" => (1, Type::Int),
        "createentity" => (2, Type::Int),
        "createclass" => (1, Type::Object),
        "deleteclass" | "entityupdate" => (1, Type::Void),
        _ => return None,
    })
}

fn entity(value: &Value) -> Result<EntityId> {
    match value.resolved()? {
        Value::Object(object) => Ok(object.entity()),
        Value::AReference(Some(attribute)) => attribute.entity(),
        _ => bail!("entity argument requires an initialized object or aref"),
    }
}

pub(crate) fn call(
    name: &str,
    arguments: &[Value],
    execution: &mut Execution<'_, impl Host>,
) -> Result<Value> {
    ensure!(
        arguments.len() == signature(name).expect("entity builtin").0,
        "entity builtin argument count"
    );
    match name {
        "createentity" => {
            // Loading a plain object already made a stack copy; only ref
            // arguments can replace the caller's entity identity.
            let attributes = arguments[0].attributes()?;
            let class = arguments[1].resolved()?;
            let id = create(class.text()?, Some(&attributes), execution)?;
            if id == EntityId::INVALID {
                return Ok(Value::Int(0));
            }
            let binding = match &arguments[0] {
                Value::Reference(reference) => reference.set_entity(id),
                Value::Object(object) => {
                    object.set_entity(id);
                    Ok(())
                }
                Value::AReference(_) => Ok(()), // Only the temporary aref ID changes.
                _ => unreachable!("validated attributes"),
            };
            if let Err(error) = binding {
                execution.host.delete_entity(id)?;
                return Err(error);
            }
            Ok(Value::Int(1))
        }
        "createclass" => {
            let class = arguments[0].resolved()?;
            // Reserve the return object's owned tree before constructing an
            // entity; Init still receives a null attribute binding.
            let object = execution.world.object(EntityId::INVALID)?;
            let id = create(class.text()?, None, execution)?;
            ensure!(id != EntityId::INVALID, "CreateClass Init returned false");
            object.set_entity(id);
            Ok(Value::Object(object))
        }
        "isentity" => Ok(Value::Int(i32::from(
            execution.host.is_entity(entity(&arguments[0])?)?,
        ))),
        "deleteclass" => {
            execution.host.delete_entity(entity(&arguments[0])?)?;
            Ok(Value::Void)
        }
        "entityupdate" => {
            // DATA::Get(int32_t&) does not convert strings/floats. Reject
            // failed reads instead of using the C++ scratch register's value.
            let Value::Int(enabled) = arguments[0].resolved()? else {
                bail!("EntityUpdate requires integer");
            };
            execution.state.entity_update.set(enabled != 0);
            Ok(Value::Void)
        }
        _ => unreachable!("entity builtin"),
    }
}

fn create(
    class: &str,
    attributes: Option<&AttributeReference>,
    execution: &mut Execution<'_, impl Host>,
) -> Result<EntityId> {
    let mut created = EntityId::INVALID;
    let result = execution.world.initialize(attributes, |store, root| {
        created = execution.host.create_entity(class, store, root)?;
        Ok(created)
    });
    if result.is_err() && created != EntityId::INVALID {
        // A successful native Init can exceed the script's smaller attribute
        // budget. Do not leave an unreachable live instance on that path.
        execution.host.delete_entity(created)?;
    }
    result
}

#[cfg(test)]
mod creation_tests;
#[cfg(test)]
mod tests;
