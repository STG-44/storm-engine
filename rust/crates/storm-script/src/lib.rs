//! Bounded source-language subset of Storm's retained stack VM.
//! The private instruction format is not the original cached-program format.
mod builtins;
mod compile;
mod conversions;
mod events;
mod objects;
pub mod preview;
mod storage;
mod value;
pub use storage::Array;
use storage::{Declaration, Slot, State};

use anyhow::{Context, Result, bail, ensure};
pub use events::Runtime;
pub use objects::{AttributeReference, Object, Reference, Variable, World};
use std::collections::BTreeMap;
use storm_core::{AttributeId, AttributeStore, EntityId};
use value::Op;
pub use value::{Type, Value};

/// Native functions must be declared in source. The embedding application
/// supplies behavior; scripts have no implicit filesystem, network or FFI API.
pub trait Host {
    fn call(&mut self, name: &str, arguments: &[Value]) -> Result<Value>;
    fn world(&self) -> World {
        World::default()
    }
    fn attribute_changed(
        &mut self,
        _entity: EntityId,
        _attributes: &AttributeStore,
        _changed: AttributeId,
    ) -> Result<()> {
        Ok(())
    }
}

impl Host for () {
    fn call(&mut self, name: &str, _: &[Value]) -> Result<Value> {
        bail!("native function {name} is not registered")
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub instructions: u64,
    pub frames: usize,
    pub stack: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            instructions: 100000,
            frames: 32,
            stack: 1024,
        }
    }
}

pub struct Program {
    functions: BTreeMap<String, Function>,
    globals: Vec<Declaration>,
}
struct Function {
    parameters: usize,
    returns: Type,
    native: bool,
    locals: Vec<Declaration>,
    code: Vec<Located>,
}
struct Located {
    op: Instruction,
    line: usize,
    column: usize,
}
enum Instruction {
    Push(Value),
    Select(Slot),
    Index,
    Load(bool),
    Store(bool),
    MakeRef,
    MakeARef,
    Attribute(Vec<Access>, bool),
    WriteAttribute,
    Increment(bool),
    Binary(Op),
    Negate,
    Not,
    Drop,
    Jump(usize),
    JumpFalse(usize),
    Short(usize, bool),
    Call(String, usize),
    Return(bool),
}
enum Access {
    Literal(String),
    Dynamic(Slot),
}

struct Execution<'a, H: Host> {
    host: &'a mut H,
    world: World,
    state: &'a State,
    limits: Limits,
    fuel: u64,
    events: &'a mut events::Handlers,
    messages: Vec<storm_core::Message>,
    current_message: Option<usize>,
    break_events: bool,
}

pub(crate) fn spend(fuel: &mut u64) -> Result<()> {
    ensure!(*fuel > 0, "script instruction budget exhausted");
    *fuel -= 1;
    Ok(())
}

impl Program {
    pub fn compile(source: &str) -> Result<Self> {
        compile::compile(source)
    }

    /// Execute with fresh globals and temporary event state. Use Runtime to persist them.
    pub fn call(
        &self,
        name: &str,
        arguments: &[Value],
        host: &mut impl Host,
        limits: Limits,
    ) -> Result<Value> {
        let state = State::new(self, host)?;
        self.execute(
            name,
            arguments,
            host,
            limits,
            &mut events::Handlers::default(),
            &state,
        )
    }

    fn execute(
        &self,
        name: &str,
        arguments: &[Value],
        host: &mut impl Host,
        limits: Limits,
        events: &mut events::Handlers,
        state: &State,
    ) -> Result<Value> {
        let mut execution = Self::execution(host, limits, events, None, state)?;
        self.invoke(&name.to_ascii_lowercase(), arguments, &mut execution, 0)
    }

    fn execution<'a, H: Host>(
        host: &'a mut H,
        limits: Limits,
        events: &'a mut events::Handlers,
        message: Option<&storm_core::Message>,
        state: &'a State,
    ) -> Result<Execution<'a, H>> {
        // Keep recursion and allocation bounded even if caller raises limits.
        ensure!(
            limits.frames > 0 && limits.frames <= 32 && limits.stack > 0 && limits.stack <= 1024,
            "runtime limits exceed supported bounds"
        );
        let world = state.world.clone();
        Ok(Execution {
            host,
            world,
            state,
            limits,
            fuel: limits.instructions,
            events,
            messages: message.cloned().into_iter().collect(),
            current_message: message.map(|_| 0),
            break_events: false,
        })
    }

    fn invoke(
        &self,
        name: &str,
        arguments: &[Value],
        execution: &mut Execution<'_, impl Host>,
        depth: usize,
    ) -> Result<Value> {
        spend(&mut execution.fuel)?;
        ensure!(depth < execution.limits.frames, "script call depth limit");
        for value in arguments {
            value.validate()?;
        }
        if events::signature(name).is_some() {
            return self.event_builtin(name, arguments, execution, depth);
        }
        if builtins::signature(name).is_some() {
            return builtins::call(name, arguments);
        }
        let function = self
            .functions
            .get(name)
            .with_context(|| format!("unknown function {name}"))?;
        ensure!(
            arguments.len() == function.parameters,
            "{name} expects {} arguments",
            function.parameters
        );
        if function.native {
            let result = execution
                .host
                .call(name, arguments)
                .with_context(|| format!("native {name}"))?;
            result.validate()?;
            ensure!(
                result.kind() == function.returns,
                "native {name} return type mismatch"
            );
            return Ok(result);
        }
        let locals = function
            .locals
            .iter()
            .enumerate()
            .map(|(i, declaration)| -> Result<Variable> {
                if let Some(argument) = arguments.get(i) {
                    Ok(Variable::new(argument.clone()))
                } else {
                    declaration.allocate(&execution.world, &execution.state.budget)
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let mut targets: Vec<(Variable, bool)> = Vec::new();
        let mut stack: Vec<Value> = Vec::new();
        let mut ip = 0;
        loop {
            let located = function
                .code
                .get(ip)
                .context("script reached invalid instruction")?;
            let result: Result<Option<Value>> = (|| {
                spend(&mut execution.fuel)?;
                ip += 1;
                let pop =
                    |stack: &mut Vec<Value>| stack.pop().context("script operand stack underflow");
                match &located.op {
                    Instruction::Push(value) => stack.push(value.clone()),
                    Instruction::Select(slot) => targets.push((
                        slot.variable(&locals, &execution.state.globals).clone(),
                        false,
                    )),
                    Instruction::Index => {
                        let index = pop(&mut stack)?.resolved()?;
                        let Value::Int(index) = index else {
                            bail!("array index requires integer");
                        };
                        let base = targets.pop().context("script target stack underflow")?;
                        let indirect = base.1 || base.0.is_reference();
                        let Value::Array(array) = base.0.value()? else {
                            bail!("index on scalar");
                        };
                        targets.push((array.element(index)?, indirect));
                    }
                    Instruction::Load(keep) => {
                        let target = if *keep {
                            targets.last().cloned()
                        } else {
                            targets.pop()
                        }
                        .context("script target stack underflow")?;
                        stack.push(if target.1 {
                            target.0.reference()
                        } else {
                            target.0.load()?
                        });
                    }
                    Instruction::Store(convert) => {
                        let v = pop(&mut stack)?;
                        targets
                            .pop()
                            .context("script target stack underflow")?
                            .0
                            .assign(v, *convert)?;
                    }
                    Instruction::MakeRef => {
                        let source = targets.pop().context("script target stack underflow")?;
                        targets
                            .pop()
                            .context("script target stack underflow")?
                            .0
                            .bind(&source.0)?;
                    }
                    Instruction::MakeARef => {
                        let attribute = pop(&mut stack)?.attributes()?;
                        targets
                            .pop()
                            .context("script target stack underflow")?
                            .0
                            .bind_attribute(attribute)?;
                    }
                    Instruction::Attribute(path, create) => {
                        let base = targets
                            .pop()
                            .context("script target stack underflow")?
                            .0
                            .value()?
                            .attributes()?;
                        let segments = path
                            .iter()
                            .map(|access| -> Result<_> {
                                Ok(match access {
                                    Access::Literal(s) => (s.clone(), false),
                                    Access::Dynamic(slot) => (
                                        slot.variable(&locals, &execution.state.globals)
                                            .load()?
                                            .path_text()?,
                                        true,
                                    ),
                                })
                            })
                            .collect::<Result<Vec<_>>>()?;
                        let attribute = base.walk(&segments, *create)?;
                        stack.push(if *create {
                            Value::AReference(Some(attribute))
                        } else {
                            Value::String(attribute.value()?)
                        });
                    }
                    Instruction::WriteAttribute => {
                        let value = pop(&mut stack)?.attribute_text()?;
                        let attribute = pop(&mut stack)?.attributes()?;
                        attribute.write(&value, execution.host)?;
                    }
                    Instruction::Increment(decrement) => {
                        targets
                            .pop()
                            .context("script target stack underflow")?
                            .0
                            .increment(*decrement)?;
                    }
                    Instruction::Binary(op) => {
                        let rhs = pop(&mut stack)?;
                        let lhs = pop(&mut stack)?;
                        stack.push(lhs.apply_binary(*op, rhs, &mut execution.fuel)?);
                    }
                    Instruction::Negate => {
                        let v = pop(&mut stack)?;
                        stack.push(v.apply_negate()?);
                    }
                    Instruction::Not => {
                        let v = pop(&mut stack)?;
                        ensure!(
                            !matches!(v, Value::Reference(_)),
                            "logical negation on ref is invalid in DATA::Neg"
                        );
                        stack.push(v.not()?);
                    }
                    Instruction::Drop => {
                        pop(&mut stack)?;
                    }
                    Instruction::Jump(target) => ip = *target,
                    Instruction::JumpFalse(target) => {
                        if pop(&mut stack)?.condition_integer()? == 0 {
                            ip = *target;
                        }
                    }
                    Instruction::Short(target, when_true) => {
                        let value = stack
                            .last_mut()
                            .context("script condition stack underflow")?;
                        if (value.condition_integer()? != 0) == *when_true {
                            *value = value.bool_convert()?;
                            ip = *target;
                        }
                    }
                    Instruction::Call(callee, count) => {
                        ensure!(stack.len() >= *count, "script argument stack underflow");
                        let args = stack.split_off(stack.len() - count);
                        let result = self.invoke(callee, &args, execution, depth + 1)?;
                        stack.push(result);
                    }
                    Instruction::Return(value) => {
                        ensure!(
                            *value == (function.returns != Type::Void),
                            "{name} return value presence does not match declaration"
                        );
                        let result = if *value {
                            pop(&mut stack)?
                        } else {
                            Value::Void
                        };
                        ensure!(
                            result.kind() == function.returns,
                            "{name} return type mismatch: expected {:?}, got {:?}",
                            function.returns,
                            result.kind()
                        );
                        ensure!(
                            stack.is_empty() && targets.is_empty(),
                            "script return stack is not balanced"
                        );
                        return Ok(Some(result));
                    }
                }
                ensure!(
                    stack.len() + targets.len() <= execution.limits.stack,
                    "script operand stack limit"
                );
                Ok(None)
            })();
            if let Some(value) =
                result.with_context(|| format!("{name} {}:{}", located.line, located.column))?
            {
                return Ok(value);
            }
        }
    }
}

#[cfg(test)]
mod object_tests;
#[cfg(test)]
mod storage_tests;
#[cfg(test)]
mod tests;
