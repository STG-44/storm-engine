use crate::{Type, Value};
use anyhow::{Result, ensure};

pub(crate) fn signature(name: &str) -> Option<(usize, Type)> {
    Some(match name {
        "sti" | "makeint" => (1, Type::Int),
        "stf" | "makefloat" => (1, Type::Float),
        "checkattribute" => (2, Type::Int),
        "deleteattribute" | "copyattributes" => (2, Type::Void),
        "getattributesnum" => (1, Type::Int),
        "getattributen" => (2, Type::AReference),
        "getattributename" | "getattributevalue" => (1, Type::String),
        _ => return crate::events::signature(name),
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
    let attribute = value.attributes()?;
    if name == "copyattributes" {
        attribute.replace(&arguments[1].resolved()?.attributes()?)?;
        return Ok(Value::Void);
    }
    let argument = arguments.get(1).map(Value::resolved).transpose()?;
    attribute.builtin(name, argument.as_ref())
}
