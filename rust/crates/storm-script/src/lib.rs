//! Bounded source-language subset of Storm's retained stack VM.
//! The private instruction format is not the original cached-program format.
mod compile;
pub mod preview;
mod value;

use anyhow::{Context, Result, bail, ensure};
use std::collections::BTreeMap;
use value::Op;
pub use value::{Type, Value};

/// Native functions must be declared in source. The embedding application
/// supplies behavior; scripts have no implicit filesystem, network or FFI API.
pub trait Host {
    fn call(&mut self, name: &str, arguments: &[Value]) -> Result<Value>;
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
        // Keep recursion and allocation bounded even if caller raises limits.
        ensure!(
            limits.frames > 0 && limits.frames <= 32 && limits.stack > 0 && limits.stack <= 1024,
            "runtime limits exceed supported bounds"
        );
        for value in arguments {
            value.validate()?;
        }
        let mut fuel = limits.instructions;
        self.invoke(
            &name.to_ascii_lowercase(),
            arguments,
            host,
            limits,
            &mut fuel,
            0,
        )
    }

    fn invoke(
        &self,
        name: &str,
        arguments: &[Value],
        host: &mut impl Host,
        limits: Limits,
        fuel: &mut u64,
        depth: usize,
    ) -> Result<Value> {
        spend(fuel)?;
        ensure!(depth < limits.frames, "script call depth limit");
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
            let result = host
                .call(name, arguments)
                .with_context(|| format!("native {name}"))?;
            result.validate()?;
            ensure!(
                result.kind() == function.returns,
                "native {name} return type mismatch"
            );
            return Ok(result);
        }
        let mut locals: Vec<_> = function.locals.iter().map(|kind| kind.initial()).collect();
        // Original argument type checks are disabled; DATA::Copy/Set also change
        // local scalar type on assignment. Return types, however, are checked.
        locals[..arguments.len()].clone_from_slice(arguments);
        let mut stack: Vec<Value> = Vec::new();
        let mut ip = 0;
        loop {
            let located = function
                .code
                .get(ip)
                .context("script reached invalid instruction")?;
            let result: Result<Option<Value>> = (|| {
                spend(fuel)?;
                ip += 1;
                let pop =
                    |stack: &mut Vec<Value>| stack.pop().context("script operand stack underflow");
                match &located.op {
                    Instruction::Push(value) => stack.push(value.clone()),
                    Instruction::Load(index) => stack.push(locals[*index].clone()),
                    Instruction::Store(index) => {
                        let v = pop(&mut stack)?;
                        ensure!(v != Value::Void, "void assignment");
                        locals[*index] = v;
                    }
                    Instruction::Increment(index, decrement) => {
                        let Value::Int(value) = &mut locals[*index] else {
                            bail!("increment requires integer")
                        };
                        *value = value.wrapping_add(if *decrement { -1 } else { 1 });
                    }
                    Instruction::Binary(op) => {
                        let rhs = pop(&mut stack)?;
                        let mut lhs = pop(&mut stack)?;
                        if matches!(op, Op::And | Op::Or) {
                            lhs = Value::Int(i32::from(lhs.truth()?));
                        }
                        stack.push(lhs.binary(*op, rhs, fuel)?);
                    }
                    Instruction::Negate => {
                        let v = pop(&mut stack)?;
                        stack.push(v.negate()?);
                    }
                    Instruction::Not => {
                        let v = pop(&mut stack)?;
                        stack.push(v.not()?);
                    }
                    Instruction::Drop => {
                        pop(&mut stack)?;
                    }
                    Instruction::Jump(target) => ip = *target,
                    Instruction::JumpFalse(target) => {
                        if pop(&mut stack)?.integer()? == 0 {
                            ip = *target;
                        }
                    }
                    Instruction::Short(target, when_true) => {
                        let value = stack
                            .last_mut()
                            .context("script condition stack underflow")?;
                        if (value.integer()? != 0) == *when_true {
                            *value = Value::Int(i32::from(value.truth()?));
                            ip = *target;
                        }
                    }
                    Instruction::Call(callee, count) => {
                        ensure!(stack.len() >= *count, "script argument stack underflow");
                        let args = stack.split_off(stack.len() - count);
                        let result = self.invoke(callee, &args, host, limits, fuel, depth + 1)?;
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
                ensure!(stack.len() <= limits.stack, "script operand stack limit");
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
mod tests;
