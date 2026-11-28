use crate::{Type, Value};
use anyhow::{Result, ensure};

pub(crate) fn signature(name: &str) -> Option<(usize, Type)> {
    Some(match name {
        "getengineversion" => (0, Type::Int),
        "strlen" => (1, Type::Int),
        "getsymbol" => (2, Type::String),
        "strcut" => (3, Type::String),
        "findsubstr" => (3, Type::Int),
        "argb" => (4, Type::Int),
        "and" | "or" | "shl" | "shr" => (2, Type::Int),
        "sin" | "cos" | "acos" | "sqrt" => (1, Type::Float),
        "pow" => (2, Type::Float),
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
    if name == "pow" {
        let number = |arg: &Value| -> Result<f64> {
            let converted = if let Value::Reference(reference) = arg {
                reference.convert(Type::Float)?
            } else {
                arg.resolved()?.convert(Type::Float)?
            };
            let Value::Float(value) = converted else {
                unreachable!()
            };
            Ok(f64::from(value))
        };
        // The native stack converts the exponent before the base.
        let exponent = number(&arguments[1])?;
        let base = number(&arguments[0])?;
        return Ok(Value::Float(base.powf(exponent) as f32));
    }
    let value = arguments[0].resolved();
    if name == "checkattribute" && (value.is_err() || matches!(value, Ok(Value::AReference(None))))
    {
        return Ok(Value::Int(0));
    }
    let value = value?;
    if matches!(name, "and" | "or" | "shl" | "shr") {
        // The original initializes the first DATA::Get output to zero.
        let first = if let Value::Int(value) = value {
            value
        } else {
            0
        };
        let Value::Int(second) = arguments[1].resolved()? else {
            anyhow::bail!("{name} requires an integer second argument");
        };
        return Ok(Value::Int(match name {
            "and" => first & second,
            "or" => first | second,
            "shl" | "shr" => {
                ensure!((0..32).contains(&second), "bit shift count must be 0..31");
                if name == "shl" {
                    first << second
                } else {
                    first >> second
                }
            }
            _ => unreachable!(),
        }));
    }
    if name == "strlen" {
        return Ok(Value::Int(value.text()?.len() as i32));
    }
    if name == "getsymbol" {
        let text = value.text()?;
        let Value::Int(index) = arguments[1].resolved()? else {
            anyhow::bail!("GetSymbol requires an integer character index");
        };
        // FUNC_GETSYMBOL counts UTF-8 code points and returns an empty string
        // for indices outside the string, including negative indices.
        let symbol = usize::try_from(index)
            .ok()
            .and_then(|index| text.chars().nth(index));
        return Ok(Value::String(
            symbol.map_or_else(String::new, |symbol| symbol.to_string()),
        ));
    }
    if name == "strcut" {
        let (Value::Int(start), Value::Int(end)) =
            (arguments[1].resolved()?, arguments[2].resolved()?)
        else {
            anyhow::bail!("StrCut requires integer byte offsets");
        };
        // Native StrCut includes its final byte and returns empty for invalid
        // ranges. Cuts that would split UTF-8 cannot form a script string.
        let text = value.text().ok();
        let cut = text.and_then(|text| {
            let start = usize::try_from(start).ok()?;
            let end = usize::try_from(end).ok()?;
            text.get(start..=end)
        });
        return Ok(Value::String(cut.unwrap_or_default().to_owned()));
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
    if matches!(name, "sin" | "cos" | "acos" | "sqrt") {
        return match (name, value) {
            ("sin", Value::Int(value)) => Ok(Value::Float((value as f32).sin())),
            ("sin", Value::Float(value)) => Ok(Value::Float(f64::from(value).sin() as f32)),
            ("cos", Value::Int(value)) => Ok(Value::Float((value as f32).cos())),
            ("cos", Value::Float(value)) => Ok(Value::Float(f64::from(value).cos() as f32)),
            ("acos", Value::Int(value)) => {
                ensure!(
                    (-1..=1).contains(&value),
                    "acos argument is outside [-1, 1]"
                );
                Ok(Value::Float((value as f32).acos()))
            }
            ("acos", Value::Float(value)) => {
                ensure!(
                    value.is_nan() || (-1.0..=1.0).contains(&value),
                    "acos argument is outside [-1, 1]"
                );
                Ok(Value::Float(f64::from(value).acos() as f32))
            }
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
