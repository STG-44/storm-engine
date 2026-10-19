//! COMPILER::CreateMessage's script formats, with checked owned payloads.
use crate::{Type, Value, World};
use anyhow::{Result, bail, ensure};
use storm_core::{Message, Value as Field};

pub(super) fn make(arguments: &[Value], world: &World) -> Result<Message> {
    let format = arguments[0].resolved()?;
    let format = format.text()?;
    ensure!(
        format.len() <= 128 && format.bytes().all(|c| b"lpfisa".contains(&c)),
        "unsupported SendMessage format or field budget; script-variable e fields are pending"
    );
    // CreateMessage consumes the format's fields and ignores surplus arguments.
    ensure!(arguments.len() > format.len(), "missing SendMessage fields");
    let mut fields = Vec::with_capacity(format.len());
    for (code, value) in format.bytes().zip(&arguments[1..]) {
        fields.push(match code {
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
    Message::new(format, fields)
}

#[cfg(test)]
mod tests;
