//! The original entity lookup/deletion and compiler notification switch.
use crate::{Execution, Host, Type, Value};
use anyhow::{Result, bail, ensure};
use storm_core::EntityId;

pub(crate) fn signature(name: &str) -> Option<(usize, Type)> {
    Some(match name {
        "isentity" => (1, Type::Int),
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
    ensure!(arguments.len() == 1, "entity builtin expects one argument");
    match name {
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

#[cfg(test)]
mod tests;
