//! Immediate source handlers. Delayed/nested events and pointer payloads are pending.
use crate::{Execution, Host, Limits, Program, Type, Value, spend};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;
use storm_core::Message;

const MAX_EVENTS: usize = 128;
const MAX_HANDLERS: usize = 512;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Normal,
    New,
    Deleted,
}
struct Handler {
    function: String,
    status: Status,
}
#[derive(Default)]
pub(crate) struct Handlers {
    entries: BTreeMap<String, Vec<Handler>>,
    completed: u64,
}

fn event_name(name: &str) -> Result<String> {
    ensure!(
        !name.is_empty() && name.len() <= 256 && name.is_ascii() && !name.contains('\0'),
        "invalid script event name"
    );
    Ok(name.to_ascii_lowercase())
}

impl Handlers {
    fn register(&mut self, event: &str, function: &str, flag: i32) -> Result<()> {
        let event = event_name(event)?;
        if let Some(handler) = self
            .entries
            .get_mut(&event)
            .and_then(|handlers| handlers.iter_mut().find(|h| h.function == function))
        {
            // S_EVENTTAB reactivates an existing pair immediately, even with flag != 0.
            handler.status = Status::Normal;
            return Ok(());
        }
        ensure!(
            self.entries.contains_key(&event) || self.entries.len() < MAX_EVENTS,
            "script event name budget"
        );
        ensure!(
            self.entries.values().map(Vec::len).sum::<usize>() < MAX_HANDLERS,
            "script event handler budget"
        );
        self.entries.entry(event).or_default().push(Handler {
            function: function.into(),
            status: if flag == 0 {
                Status::Normal
            } else {
                Status::New
            },
        });
        Ok(())
    }
    fn delete(&mut self, event: &str, function: &str) -> Result<()> {
        let event = event_name(event)?;
        if let Some(handler) = self
            .entries
            .get_mut(&event)
            .and_then(|handlers| handlers.iter_mut().find(|h| h.function == function))
        {
            handler.status = Status::Deleted;
        }
        Ok(())
    }
    fn frame(&mut self) {
        for handlers in self.entries.values_mut() {
            let mut index = 0;
            while index < handlers.len() {
                if handlers[index].status == Status::Deleted {
                    handlers.remove(index);
                    // Retained ProcessFrame sets i=0 inside a for-loop, whose
                    // increment then starts at 1. Preserve its skipped first
                    // shifted entry, including a still-New/Deleted status.
                    index = 1;
                } else {
                    handlers[index].status = Status::Normal;
                    index += 1;
                }
            }
        }
    }
}

/// A compiled program with persistent bounded event registration state.
/// Embedders provide the World through Host, as with Program::call.
pub struct Runtime {
    program: Program,
    handlers: Handlers,
}
impl Runtime {
    pub fn compile(source: &str) -> Result<Self> {
        Ok(Self {
            program: Program::compile(source)?,
            handlers: Handlers::default(),
        })
    }
    pub fn call(
        &mut self,
        name: &str,
        arguments: &[Value],
        host: &mut impl Host,
        limits: Limits,
    ) -> Result<Value> {
        self.program
            .execute(name, arguments, host, limits, &mut self.handlers, None)
    }
    /// Advance statuses using S_EVENTTAB::ProcessFrame, including its index quirk.
    /// Delayed event scheduling and static #event_handler declarations are not ported.
    pub fn process_frame(&mut self) {
        self.handlers.frame();
    }
    pub fn completed_handlers(&self) -> u64 {
        self.handlers.completed
    }

    /// Each handler starts at the first field. One instruction budget spans the
    /// whole dispatch, including registrations added by running handlers.
    pub fn emit(
        &mut self,
        name: &str,
        message: &mut Message,
        host: &mut impl Host,
        limits: Limits,
    ) -> Result<Option<Value>> {
        let name = event_name(name)?;
        ensure!(
            message.values().len() <= 128,
            "script event message field budget"
        );
        let mut execution = Program::execution(host, limits, &mut self.handlers, Some(message))?;
        let mut result = None;
        let mut index = 0;
        // Re-read the table after each call: deletion is immediately visible,
        // and a newly appended flag=0 handler can run in this same dispatch.
        while let Some(handlers) = execution.events.entries.get(&name) {
            let Some(handler) = handlers.get(index) else {
                break;
            };
            let function = (handler.status == Status::Normal).then(|| handler.function.clone());
            index += 1;
            spend(&mut execution.fuel)?;
            let Some(function) = function else {
                continue;
            };
            execution
                .message
                .as_deref_mut()
                .expect("event message")
                .move_to_start();
            let value = self
                .program
                .invoke(&function, &[], &mut execution, 0)
                .with_context(|| format!("event {name}, handler {function}"))?;
            execution.events.completed = execution.events.completed.saturating_add(1);
            if value != Value::Void {
                result = Some(value);
            }
            if execution.break_events {
                break;
            }
        }
        Ok(result)
    }
}

pub(crate) fn signature(name: &str) -> Option<(usize, Type)> {
    Some(match name {
        "seteventhandler" => (3, Type::Void),
        "deleventhandler" => (2, Type::Void),
        "eventsbreak" | "geteventdata" => (0, Type::Void), // GetEventData is dynamically typed.
        _ => return None,
    })
}
impl Program {
    pub(crate) fn event_builtin(
        &self,
        name: &str,
        arguments: &[Value],
        execution: &mut Execution<'_, impl Host>,
    ) -> Result<Value> {
        ensure!(
            arguments.len() == signature(name).expect("event builtin").0,
            "event builtin argument count"
        );
        match name {
            "geteventdata" => {
                let message = execution
                    .message
                    .as_deref_mut()
                    .context("No data on this event")?;
                Ok(
                    match message
                        .current_type()
                        .context("No (more) data on this event")?
                    {
                        'l' => Value::Int(message.long()?),
                        'f' => Value::Float(message.float()?),
                        's' => {
                            let value = Value::String(message.string()?.into());
                            value.validate()?;
                            value
                        }
                        code => anyhow::bail!(
                            "unsupported GetEventData type {code:?}; entity, attribute and script-variable payloads are pending"
                        ),
                    },
                )
            }
            "eventsbreak" => {
                execution.break_events = true;
                Ok(Value::Void)
            }
            "seteventhandler" | "deleventhandler" => {
                let event = arguments[0].resolved()?;
                let function = arguments[1].resolved()?.text()?.to_ascii_lowercase();
                let definition = self
                    .functions
                    .get(&function)
                    .context("unknown event handler function")?;
                ensure!(
                    definition.parameters == 0,
                    "event handler must take no parameters"
                );
                if name == "seteventhandler" {
                    let flag = arguments[2].resolved()?;
                    let Value::Int(flag) = flag else {
                        anyhow::bail!("event handler flag must be int");
                    };
                    execution.events.register(event.text()?, &function, flag)?;
                } else {
                    execution.events.delete(event.text()?, &function)?;
                }
                Ok(Value::Void)
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests;
