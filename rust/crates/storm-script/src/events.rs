//! Bounded immediate/nested handlers and frame-driven posted events.
mod queue;
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
    queue: queue::Queue,
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
        self.queue.cancel(&event);
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
    /// This legacy method advances handler statuses only. Use advance_frame for posted events.
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
        check_message(message)?;
        let mut execution = Program::execution(host, limits, &mut self.handlers, Some(message))?;
        let result = self.program.dispatch(name, &mut execution, 0);
        // Keep the caller's cursor observable even after nested/error paths.
        *message = execution.messages.remove(0);
        result
    }
    pub fn posted_events(&self) -> u64 {
        self.handlers.queue.total
    }
    pub fn pending_events(&self) -> usize {
        self.handlers.queue.pending()
    }
    pub fn retained_post_events(&self) -> usize {
        self.handlers.queue.entries.len()
    }

    /// Advance the original frame scheduler. One budget covers the complete
    /// tick, including nested events and new events posted by its handlers.
    pub fn advance_frame(
        &mut self,
        delta_ms: u32,
        host: &mut impl Host,
        limits: Limits,
    ) -> Result<usize> {
        let mut execution = Program::execution(host, limits, &mut self.handlers, None)?;
        execution.events.frame();
        let result = (|| {
            let mut fired = 0;
            let mut index = 0;
            while index < execution.events.queue.entries.len() {
                spend(&mut execution.fuel)?;
                if let Some((name, message)) = execution.events.queue.ready(index, delta_ms) {
                    self.program
                        .dispatch_message(&name, message, &mut execution, 0)?;
                    fired += 1;
                }
                index += 1;
            }
            Ok(fired)
        })();
        execution.events.queue.cleanup();
        result
    }
}

fn check_message(message: &Message) -> Result<()> {
    ensure!(
        message.values().len() <= 128,
        "script event message field budget"
    );
    for field in message.values() {
        if let storm_core::Value::String(s) = field {
            ensure!(
                s.len() <= crate::value::MAX_STRING && !s.contains('\0'),
                "script event string limit or NUL"
            );
        }
    }
    Ok(())
}

impl Program {
    fn dispatch(
        &self,
        name: &str,
        execution: &mut Execution<'_, impl Host>,
        depth: usize,
    ) -> Result<Option<Value>> {
        // Original bEventsBreak is shared: a nested dispatch resets it on entry,
        // and an inner EventsBreak propagates to the outer loop on return.
        execution.break_events = false;
        let name = event_name(name)?;
        let mut result = None;
        let mut index = 0;
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
            let saved_message = execution.current_message;
            if let Some(slot) = saved_message {
                execution.messages[slot].move_to_start();
            }
            let value = self
                .invoke(&function, &[], execution, depth)
                .with_context(|| format!("event {name}, handler {function}"));
            // COMPILER::ProcessEvent restores pMem at each handler boundary.
            execution.current_message = saved_message;
            let value = value?;
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
    fn dispatch_message(
        &self,
        name: &str,
        message: Option<Message>,
        execution: &mut Execution<'_, impl Host>,
        depth: usize,
    ) -> Result<Option<Value>> {
        if let Some(message) = message {
            ensure!(
                execution.messages.len() < execution.limits.frames,
                "script event message depth limit"
            );
            execution.current_message = Some(execution.messages.len());
            execution.messages.push(message);
            let result = self.dispatch(name, execution, depth);
            execution.messages.pop();
            // The MESSAGE* wrapper clears pEventMessage rather than restoring
            // its parent. Outer GetEventData therefore fails until that outer
            // handler returns. Event(name) alone instead inherits its message.
            execution.current_message = None;
            result
        } else {
            execution.current_message = None;
            let result = self.dispatch(name, execution, depth);
            execution.current_message = None;
            result
        }
    }
}

pub(crate) fn signature(name: &str) -> Option<(usize, Type)> {
    Some(match name {
        "seteventhandler" => (3, Type::Void),
        "event" => (1, Type::Void),
        "postevent" => (2, Type::Void),
        "clearpostevents" => (0, Type::Void),
        "deleventhandler" => (2, Type::Void),
        "eventsbreak" | "geteventdata" => (0, Type::Void), // GetEventData is dynamically typed.
        _ => return None,
    })
}
pub(crate) fn arguments_valid(name: &str, count: usize) -> bool {
    match name {
        "event" => (1..=130).contains(&count),
        "postevent" => (2..=131).contains(&count),
        _ => signature(name).is_some_and(|(expected, _)| count == expected),
    }
}

impl Program {
    pub(crate) fn event_builtin(
        &self,
        name: &str,
        arguments: &[Value],
        execution: &mut Execution<'_, impl Host>,
        depth: usize,
    ) -> Result<Value> {
        ensure!(
            arguments_valid(name, arguments.len()),
            "event builtin argument count"
        );
        match name {
            "geteventdata" => {
                let slot = execution.current_message.context("No data on this event")?;
                let message = &mut execution.messages[slot];
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
            "event" | "postevent" => {
                let event = arguments[0].resolved()?;
                let event = event.text()?;
                event_name(event)?;
                if name == "event" {
                    if arguments.len() == 1 {
                        self.dispatch(event, execution, depth + 1)?;
                    } else {
                        let message = make_message(&arguments[1..])?;
                        self.dispatch_message(event, Some(message), execution, depth + 1)?;
                    }
                } else {
                    let delay = arguments[1].resolved()?;
                    let Value::Int(delay) = delay else {
                        anyhow::bail!("PostEvent delay must be int");
                    };
                    // Original ignores a third argument unless there are >=4.
                    let message = if arguments.len() >= 4 {
                        Some(make_message(&arguments[2..])?)
                    } else {
                        None
                    };
                    execution.events.queue.post(event, delay, message)?;
                }
                Ok(Value::Void)
            }
            "clearpostevents" => {
                execution.events.queue.invalidate_all();
                Ok(Value::Void)
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

fn make_message(arguments: &[Value]) -> Result<Message> {
    use storm_core::Value as Field;
    let format = arguments[0].resolved()?;
    let format = format.text()?;
    ensure!(
        format.len() <= 128 && format.bytes().all(|b| b"lfs".contains(&b)),
        "unsupported event format or field budget"
    );
    ensure!(
        arguments.len() > format.len(),
        "missing event message fields"
    );
    let mut fields = Vec::with_capacity(format.len());
    for (code, value) in format.bytes().zip(&arguments[1..]) {
        fields.push(match code {
            b'l' => {
                let Value::Int(n) = value.resolved()? else {
                    anyhow::bail!("event l field requires int");
                };
                Field::Long(n)
            }
            b'f' => {
                let resolved = value.resolved()?;
                let number = match resolved {
                    Value::Float(n) => n,
                    Value::Int(n) => {
                        if let Value::Reference(reference) = value {
                            reference.convert(Type::Float)?;
                        }
                        n as f32
                    }
                    _ => anyhow::bail!("event f field requires float or int"),
                };
                Field::Float(number)
            }
            b's' => {
                let Value::String(s) = value else {
                    anyhow::bail!("event s field requires a direct string value");
                };
                Field::String(s.clone())
            }
            _ => unreachable!(),
        });
    }
    Message::new(format, fields)
}

#[cfg(test)]
mod scheduled_tests;
#[cfg(test)]
mod tests;
