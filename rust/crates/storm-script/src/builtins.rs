use crate::{Type, Value};
use anyhow::{Result, ensure};

pub(crate) fn signature(name: &str) -> Option<(usize, Type)> {
    Some(match name {
        "sti" | "makeint" => (1, Type::Int),
        "stf" | "makefloat" => (1, Type::Float),
        "getarraysize" => (1, Type::Int),
        "setarraysize" => (2, Type::Void),
        "checkattribute" => (2, Type::Int),
        "deleteattribute" | "copyattributes" => (2, Type::Void),
        "getattributesnum" => (1, Type::Int),
        "getattributen" => (2, Type::AReference),
        "getattributename" | "getattributevalue" => (1, Type::String),
        _ => return crate::entity_api::signature(name).or_else(|| crate::events::signature(name)),
    })
}

pub(crate) fn call(name: &str, arguments: &[Value]) -> Result<Value> {
    let (count, _) = signature(name).expect("builtin");
    ensure!(arguments.len() == count, "builtin argument count");
    let value = arguments[0].resolved();
    if name == "checkattribute" && (value.is_err() || matches!(value, Ok(Value::AReference(None))))
    {
        return Ok(Value::Int(0));
    }
    let value = value?;
    if matches!(name, "sti" | "makeint" | "stf" | "makefloat") {
        let kind = if matches!(name, "sti" | "makeint") {
            Type::Int
        } else {
            Type::Float
        };
        return if let Value::Reference(reference) = &arguments[0] {
            reference.convert(kind)
        } else {
            value.convert(kind)
        };
    }
    if matches!(name, "getarraysize" | "setarraysize") {
        ensure!(
            matches!(arguments[0], Value::Reference(_)),
            "array builtin requires ref"
        );
        let Value::Array(array) = value else {
            anyhow::bail!("not array");
        };
        if name == "getarraysize" {
            return Ok(Value::Int(array.len() as i32));
        }
        let Value::Int(count) = arguments[1].resolved()? else {
            anyhow::bail!("array size requires integer");
        };
        array.resize(count)?;
        return Ok(Value::Void);
    }
    let attribute = value.attributes()?;
    if name == "copyattributes" {
        attribute.replace(&arguments[1].resolved()?.attributes()?)?;
        return Ok(Value::Void);
    }
    let argument = arguments.get(1).map(Value::resolved).transpose()?;
    attribute.builtin(name, argument.as_ref())
}
