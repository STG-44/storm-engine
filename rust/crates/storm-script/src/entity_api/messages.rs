//! COMPILER::CreateMessage's script formats, with checked owned payloads.
use crate::objects::native::NativeArguments;
use crate::{Type, Value, World};
use anyhow::{Result, bail, ensure};
use storm_core::{Message, Value as Field};

pub(super) struct Prepared {
    pub message: Message,
    // Keep by-value stack cells alive until native dispatch returns. Captured
    // ref targets stay weak and follow their own local/global/array lifetimes.
    _arguments: NativeArguments,
}

pub(super) fn make(arguments: &[Value], world: &World) -> Result<Prepared> {
    let format = arguments[0].resolved()?;
    let format = format.text()?;
    ensure!(
        format.len() <= 128 && format.bytes().all(|c| b"lpfisae".contains(&c)),
        "unsupported SendMessage format or field budget"
    );
    // CreateMessage consumes the format's fields and ignores surplus arguments.
    ensure!(arguments.len() > format.len(), "missing SendMessage fields");
    let mut fields = Vec::with_capacity(format.len());
    let mut native = NativeArguments::new(world);
    for (code, value) in format.bytes().zip(&arguments[1..]) {
        fields.push(match code {
            b'e' => Field::ScriptVariable(native.capture(value)?),
            b'l' => {
                let Value::Int(n) = value.resolved()? else {
                    bail!("SendMessage l field requires int");
                };
                Field::Long(n)
            }
            b'p' => {
                let Value::Ptr(n) = value.resolved()? else {
                    bail!("SendMessage p field requires ptr");
                };
                Field::Pointer(n)
            }
            b'f' => {
                let number = match value.resolved()? {
                    Value::Float(n) => n,
                    Value::Int(n) => {
                        if let Value::Reference(reference) = value {
                            reference.convert(Type::Float)?;
                        }
                        n as f32
                    }
                    _ => bail!("SendMessage f field requires float or int"),
                };
                Field::Float(number)
            }
            b'i' => Field::Entity(super::entity(value)?),
            b'a' => Field::Attribute(world.attribute_id(&value.attributes()?)?),
            b's' => {
                // Unlike the other fields, C++ tests the stack value's type
                // before dereferencing it. A ref to a string must still fail.
                let Value::String(s) = value else {
                    bail!("SendMessage s field requires a direct string value");
                };
                Field::String(s.clone())
            }
            _ => unreachable!(),
        });
    }
    Ok(Prepared {
        message: Message::new(format, fields)?,
        _arguments: native,
    })
}

#[cfg(test)]
mod tests;
