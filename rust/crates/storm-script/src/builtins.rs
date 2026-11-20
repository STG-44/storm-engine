use crate::{Type, Value};
use anyhow::{Result, ensure};

pub(crate) fn signature(name: &str) -> Option<(usize, Type)> {
    Some(match name {
        "getengineversion" => (0, Type::Int),
        "strlen" => (1, Type::Int),
        "findsubstr" => (3, Type::Int),
        "argb" => (4, Type::Int),
        "sin" | "sqrt" => (1, Type::Float),
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

pub(crate) fn arity(name: &str) -> Option<usize> {
    signature(name)
        .map(|(count, _)| count)
        .or_else(|| crate::retained_builtins::signature(name).map(|(count, _)| count))
}

pub(crate) fn call(name: &str, arguments: &[Value]) -> Result<Value> {
    let (count, _) = signature(name).expect("builtin");
    ensure!(arguments.len() == count, "builtin argument count");
    if name == "getengineversion" {
        // ENGINE_SCRIPT_VERSION in the retained core_impl.h.
        return Ok(Value::Int(54128));
    }
    if name == "argb" {
        let mut packed = 0;
        for (argument, shift) in arguments.iter().zip([24, 16, 8, 0]) {
            let Value::Int(channel) = argument.resolved()? else {
                anyhow::bail!("ARGB channel requires int");
            };
            // The original shifts and combines the full values without masking each channel.
            packed |= channel << shift;
        }
        return Ok(Value::Int(packed));
    }
    let value = arguments[0].resolved();
    if name == "checkattribute" && (value.is_err() || matches!(value, Ok(Value::AReference(None))))
    {
        return Ok(Value::Int(0));
    }
    let value = value?;
    if name == "strlen" {
        return Ok(Value::Int(value.text()?.len() as i32));
    }
    if name == "findsubstr" {
        let source = value.text()?;
        let needle = arguments[1].resolved()?;
        let needle = needle.text()?;
        let Value::Int(start) = arguments[2].resolved()? else {
            anyhow::bail!("FindSubStr requires an integer starting byte offset");
        };
        if start < 0 || start as usize > source.len() {
            return Ok(Value::Int(-1));
        }
        if needle.is_empty() {
            return Ok(Value::Int(start));
        }
        // The original reports byte positions and folds ASCII letters only.
        // A valid UTF-8 search string cannot start on a continuation byte.
        let start = source.ceil_char_boundary(start as usize);
        let offset = source[start..]
            .to_ascii_uppercase()
            .find(&needle.to_ascii_uppercase())
            .map_or(-1, |offset| (start + offset) as i32);
        return Ok(Value::Int(offset));
    }
    if matches!(name, "sin" | "sqrt") {
        return match (name, value) {
            ("sin", Value::Int(value)) => Ok(Value::Float((value as f32).sin())),
            ("sin", Value::Float(value)) => Ok(Value::Float(f64::from(value).sin() as f32)),
            ("sqrt", Value::Int(value)) if value >= 0 => {
                Ok(Value::Int((value as f32).sqrt() as i32))
            }
            ("sqrt", Value::Float(value)) => {
                // The native comparison permits NaN and preserves signed zero.
                ensure!(value.is_nan() || value >= 0.0, "negative sqrt argument");
                Ok(Value::Float(value.sqrt()))
            }
            ("sqrt", Value::Int(_)) => anyhow::bail!("negative sqrt argument"),
            _ => anyhow::bail!("invalid {name} argument"),
        };
    }
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
