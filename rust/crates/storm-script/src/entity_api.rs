//! The original entity lookup/deletion and compiler notification switch.
use crate::{AttributeReference, Execution, Host, Program, Type, Value};
use anyhow::{Context, Result, bail, ensure};
use storm_core::EntityId;

pub(crate) fn signature(name: &str) -> Option<(usize, Type)> {
    Some(match name {
        "isentity" => (1, Type::Int),
        "createentity" => (2, Type::Int),
        "createclass" => (1, Type::Object),
        "sendmessage" => (2, Type::Ptr),
        "deleteclass" | "entityupdate" => (1, Type::Void),
        "layersetexecute" | "layersetrealize" | "trace" => (1, Type::Void),
        "layeraddobject" => (3, Type::Void),
        "layerdelobject" | "layerfreeze" => (2, Type::Void),
        "getdeltatime" => (0, Type::Int),
        "gettargetplatform" => (0, Type::String),
        _ => return None,
    })
}

pub(crate) fn arguments_valid(name: &str, count: usize) -> bool {
    if name == "sendmessage" {
        (2..=130).contains(&count)
    } else {
        signature(name).is_some_and(|(expected, _)| count == expected)
    }
}

fn entity(value: &Value) -> Result<EntityId> {
    match value.resolved()? {
        Value::Object(object) => Ok(object.entity()),
        Value::AReference(Some(attribute)) => attribute.entity(),
        _ => bail!("entity argument requires an initialized object or aref"),
    }
}

fn integer(value: &Value) -> Result<i32> {
    let Value::Int(n) = value.resolved()? else {
        bail!("core argument requires integer");
    };
    Ok(n)
}
fn layer(value: &Value) -> Result<u8> {
    let n = integer(value)?;
    ensure!((0..32).contains(&n), "layer index must be 0..31");
    Ok(n as u8)
}

pub(crate) fn call(
    program: &Program,
    name: &str,
    arguments: &[Value],
    execution: &mut Execution<'_, impl Host>,
    depth: usize,
) -> Result<Value> {
    ensure!(
        arguments_valid(name, arguments.len()),
        "entity builtin argument count"
    );
    match name {
        "layersetexecute" | "layersetrealize" => {
            let kind = if name == "layersetexecute" {
                storm_core::LayerType::Execute
            } else {
                storm_core::LayerType::Realize
            };
            execution.host.set_layer_type(layer(&arguments[0])?, kind)?;
            Ok(Value::Void)
        }
        "layeraddobject" => {
            execution.host.add_to_layer(
                layer(&arguments[0])?,
                entity(&arguments[1])?,
                integer(&arguments[2])? as u32,
            )?;
            Ok(Value::Void)
        }
        "layerdelobject" => {
            execution
                .host
                .remove_from_layer(layer(&arguments[0])?, entity(&arguments[1])?)?;
            Ok(Value::Void)
        }
        "layerfreeze" => {
            execution
                .host
                .freeze_layer(layer(&arguments[0])?, integer(&arguments[1])? != 0)?;
            Ok(Value::Void)
        }
        "getdeltatime" => Ok(Value::Int(execution.host.delta_time()? as i32)),
        // Original FUNC_GETTARGETPLATFORM returns "pc", including native Unix.
        "gettargetplatform" => Ok(Value::String("pc".into())),
        "trace" => {
            execution.host.trace(&arguments[0].clone().path_text()?)?;
            Ok(Value::Void)
        }
        "sendmessage" => {
            let id = entity(&arguments[0])?;
            let mut prepared = messages::make(&arguments[1..], &execution.world)?;
            prepared.message.move_to_start();
            let result = execution.world.initialize(None, |store, _| {
                execution
                    .host
                    .send_message(id, &mut prepared.message, store)
            })?;
            for (event, delay) in execution.host.take_entity_posts(id) {
                execution.events.post(event, delay, None)?;
            }
            Ok(Value::Ptr(result))
        }
        "createentity" => {
            // Loading a plain object already made a stack copy; only ref
            // arguments can replace the caller's entity identity.
            let attributes = arguments[0].attributes()?;
            let class = arguments[1].resolved()?;
            let id = create(program, class.text()?, Some(&attributes), execution, depth)?;
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
            let id = create(program, class.text()?, None, execution, depth)?;
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
    program: &Program,
    class: &str,
    attributes: Option<&AttributeReference>,
    execution: &mut Execution<'_, impl Host>,
    depth: usize,
) -> Result<EntityId> {
    let mut created = EntityId::INVALID;
    let result = execution
        .world
        .initialize(attributes, |store, root| {
            created = execution.host.create_entity(class, store, root)?;
            Ok(created)
        })
        .and_then(|id| {
            if id != EntityId::INVALID {
                for (event, message) in execution.host.entity_init_events(id)? {
                    let value = program
                        .dispatch_message(event, Some(message), execution, depth + 1)
                        .with_context(|| format!("Init entity {class}, script event {event}"))?;
                    execution
                        .host
                        .entity_init_result(id, event, value)
                        .with_context(|| {
                            format!("Init entity {class}, result of script event {event}")
                        })?;
                }
            }
            Ok(id)
        });
    if result.is_err() && created != EntityId::INVALID {
        // Neither an attribute budget failure nor a callback failure may
        // leave an entity the caller never received.
        execution.host.delete_entity(created)?;
    }
    result
}

#[cfg(test)]
mod creation_tests;
mod messages;
#[cfg(test)]
mod tests;
