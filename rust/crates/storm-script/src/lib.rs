//! Bounded source-language subset of Storm's retained stack VM.
//! The private instruction format is not the original cached-program format.
mod builtins;
mod compile;
mod conversions;
mod events;
mod objects;
pub mod preview;
mod value;

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
}
struct Function {
    parameters: usize,
    returns: Type,
    native: bool,
    locals: Vec<Type>,
    code: Vec<Located>,
}
struct Located {
    op: Instruction,
    line: usize,
    column: usize,
}
enum Instruction {
    Push(Value),
    Load(usize),
    Store(usize),
    StoreDynamic(usize),
    MakeRef(usize, usize),
    MakeARef(usize),
    Attribute(usize, Vec<Access>, bool),
    WriteAttribute,
    Increment(usize, bool),
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
    Dynamic(usize),
}

struct Execution<'a, H: Host> {
    host: &'a mut H,
    world: World,
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

    pub fn call(
        &self,
        name: &str,
        arguments: &[Value],
        host: &mut impl Host,
        limits: Limits,
    ) -> Result<Value> {
        self.execute(
            name,
            arguments,
            host,
            limits,
            &mut events::Handlers::default(),
            None,
        )
    }

    fn execute(
        &self,
        name: &str,
        arguments: &[Value],
        host: &mut impl Host,
        limits: Limits,
        events: &mut events::Handlers,
        message: Option<&storm_core::Message>,
    ) -> Result<Value> {
        let mut execution = Self::execution(host, limits, events, message)?;
        self.invoke(&name.to_ascii_lowercase(), arguments, &mut execution, 0)
    }

    fn execution<'a, H: Host>(
        host: &'a mut H,
        limits: Limits,
        events: &'a mut events::Handlers,
        message: Option<&storm_core::Message>,
    ) -> Result<Execution<'a, H>> {
        // Keep recursion and allocation bounded even if caller raises limits.
        ensure!(
            limits.frames > 0 && limits.frames <= 32 && limits.stack > 0 && limits.stack <= 1024,
            "runtime limits exceed supported bounds"
        );
        let world = host.world();
        Ok(Execution {
            host,
            world,
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
            .map(|(i, kind)| -> Result<Variable> {
                let value = if let Some(argument) = arguments.get(i) {
                    argument.clone()
                } else if *kind == Type::Object {
                    Value::Object(execution.world.object(EntityId::INVALID)?)
                } else {
                    kind.initial()
                };
                Ok(Variable::new(value))
            })
            .collect::<Result<Vec<_>>>()?;
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
                    Instruction::Load(index) => stack.push(locals[*index].load()?),
                    Instruction::Store(index) | Instruction::StoreDynamic(index) => {
                        let v = pop(&mut stack)?;
                        locals[*index].assign(v, matches!(located.op, Instruction::Store(_)))?;
                    }
                    Instruction::MakeRef(destination, source) => {
                        locals[*destination].bind(&locals[*source])?
                    }
                    Instruction::MakeARef(destination) => {
                        let attribute = pop(&mut stack)?.attributes()?;
                        locals[*destination].bind_attribute(attribute)?;
                    }
                    Instruction::Attribute(index, path, create) => {
                        let base = locals[*index].value()?.attributes()?;
                        let segments = path
                            .iter()
                            .map(|access| -> Result<_> {
                                Ok(match access {
                                    Access::Literal(s) => (s.clone(), false),
                                    Access::Dynamic(i) => (locals[*i].load()?.path_text()?, true),
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
                    Instruction::Increment(index, decrement) => {
                        locals[*index].increment(*decrement)?;
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
                        ensure!(stack.is_empty(), "script return stack is not balanced");
                        return Ok(Some(result));
                    }
                }
                ensure!(
                    stack.len() <= execution.limits.stack,
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
mod tests;
