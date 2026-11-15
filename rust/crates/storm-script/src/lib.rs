//! Bounded source-language subset of Storm's retained stack VM.
//! The private instruction format is not the original cached-program format.
mod builtins;
mod compile;
mod compile_limits;
pub use compile_limits::CompileLimits;
mod conversions;
mod entity_api;
mod events;
mod objects;
pub mod preview;
mod source;
mod storage;
pub use source::{DirectorySource, LibraryRequest, SourceLoader};
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

/// External native functions must be declared in source. Internal builtins
/// use explicit hooks below; scripts have no implicit filesystem, network or FFI API.
pub trait Host {
    fn call(&mut self, name: &str, arguments: &[Value]) -> Result<Value>;
    /// Resolve a #libriary name through explicit registrations. Called before
    /// fresh script state; the embedding owns idempotence and native bindings.
    fn load_library(&mut self, name: &str) -> Result<()> {
        bail!("native script library {name} is not registered")
    }
    fn world(&self) -> World {
        World::default()
    }
    /// Embeddings must explicitly provide entity lookup and deferred deletion.
    fn is_entity(&self, _entity: EntityId) -> Result<bool> {
        bail!("entity lookup is not bound by this host")
    }
    fn delete_entity(&mut self, _entity: EntityId) -> Result<()> {
        bail!("entity deletion is not bound by this host")
    }
    fn create_entity(
        &mut self,
        _class: &str,
        _store: &mut AttributeStore,
        _attributes: Option<AttributeId>,
    ) -> Result<EntityId> {
        bail!("entity creation is not bound by this host")
    }
    /// Synchronous native dispatch. Attribute handles belong to this store and
    /// are only borrowed for this call; pointer tokens are opaque 64-bit values.
    fn send_message(
        &mut self,
        _entity: EntityId,
        _message: &mut storm_core::Message,
        _attributes: &mut AttributeStore,
    ) -> Result<u64> {
        bail!("entity messages are not bound by this host")
    }
    fn attribute_changed(
        &mut self,
        _entity: EntityId,
        _attributes: &AttributeStore,
        _changed: AttributeId,
    ) -> Result<()> {
        Ok(())
    }
    fn set_layer_type(&mut self, _index: u8, _kind: storm_core::LayerType) -> Result<()> {
        bail!("layer types are not bound by this host")
    }
    fn add_to_layer(&mut self, _index: u8, _entity: EntityId, _priority: u32) -> Result<()> {
        bail!("entity layers are not bound by this host")
    }
    fn remove_from_layer(&mut self, _index: u8, _entity: EntityId) -> Result<()> {
        bail!("entity layers are not bound by this host")
    }
    fn freeze_layer(&mut self, _index: u8, _frozen: bool) -> Result<()> {
        bail!("layer freezing is not bound by this host")
    }
    fn delta_time(&self) -> Result<u32> {
        bail!("frame time is not bound by this host")
    }
    fn trace(&mut self, _text: &str) -> Result<()> {
        bail!("script logging is not bound by this host")
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
    sources: Vec<String>,
    static_handlers: Vec<(String, String)>,
    libraries: Vec<LibraryRequest>,
}
struct Function {
    parameters: usize,
    returns: Type,
    kind: FunctionKind,
    locals: Vec<Declaration>,
    code: Vec<Located>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum FunctionKind {
    Script,
    Native,
    Extern,
}
struct Located {
    op: Instruction,
    file: std::rc::Rc<str>,
    line: usize,
    column: usize,
}
enum Instruction {
    Push(Value),
    Select(Slot),
    Index,
    Load(bool),
    Address,
    Store(bool),
    MakeRef,
    MakeARef,
    Attribute(Vec<Access>, AttributeMode),
    WriteAttribute,
    Increment(bool),
    Binary(Op),
    CompareSwitch,
    Negate,
    Not,
    Drop,
    Jump(usize),
    JumpFalse(usize, Option<usize>),
    RestoreExpression(usize),
    JumpIfExpressionNonZero(usize),
    Short(usize, bool),
    Call(String, usize),
    DynamicCall(Slot, usize),
    Return(bool, usize),
}
enum Access {
    Literal(String),
    Dynamic(Slot),
}
#[derive(Clone, Copy)]
enum AttributeMode {
    Read,
    Bind,
    Write,
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
        compile::compile(source::memory(source)?)
    }
    pub fn compile_with_limits(source: &str, limits: CompileLimits) -> Result<Self> {
        compile::compile(source::memory_with_limits(source, limits)?)
    }

    pub fn compile_with_loader(entry: &str, loader: &mut impl SourceLoader) -> Result<Self> {
        compile::compile(source::load(entry, loader)?)
    }
    pub fn compile_with_loader_and_limits(
        entry: &str,
        loader: &mut impl SourceLoader,
        limits: CompileLimits,
    ) -> Result<Self> {
        compile::compile(source::load_with_limits(entry, loader, limits)?)
    }
    /// Normalized file names in original append order; in-memory source has one synthetic name.
    pub fn sources(&self) -> &[String] {
        &self.sources
    }
    pub fn libraries(&self) -> &[LibraryRequest] {
        &self.libraries
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
            &mut events::Handlers::for_program(self)?,
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
        if entity_api::signature(name).is_some() {
            return entity_api::call(name, arguments, execution);
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
        ensure!(
            function.kind != FunctionKind::Extern,
            "extern script function {name} is not loaded"
        );
        if function.kind == FunctionKind::Native {
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
        let mut if_conditions: Vec<Option<Value>> = Vec::new();
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
                    Instruction::Address => {
                        let (variable, _) =
                            targets.pop().context("script target stack underflow")?;
                        stack.push(variable.address()?);
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
                        let converted = targets
                            .pop()
                            .context("script target stack underflow")?
                            .0
                            .assign(v, *convert)?;
                        if let Some(value) = converted {
                            execution.state.set_expression(value);
                        }
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
                    Instruction::Attribute(path, mode) => {
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
                                    Access::Dynamic(slot) => {
                                        let value = slot
                                            .variable(&locals, &execution.state.globals)
                                            .load()?;
                                        let key = value.clone().path_text()?;
                                        // ADVANCE_AP and VERIFY_AP convert the key in EX.
                                        // Assignment targets use ACCESS_VAR instead.
                                        if !matches!(mode, AttributeMode::Write) {
                                            let expression = if matches!(value, Value::Reference(_))
                                            {
                                                value
                                            } else {
                                                Value::String(key.clone())
                                            };
                                            execution.state.set_expression(expression);
                                        }
                                        (key, true)
                                    }
                                })
                            })
                            .collect::<Result<Vec<_>>>()?;
                        let attribute =
                            base.walk(&segments, !matches!(mode, AttributeMode::Read))?;
                        stack.push(match mode {
                            AttributeMode::Read => Value::String(attribute.value()?),
                            AttributeMode::Bind | AttributeMode::Write => {
                                Value::AReference(Some(attribute))
                            }
                        });
                    }
                    Instruction::WriteAttribute => {
                        let source = pop(&mut stack)?;
                        let value = source.clone().attribute_text()?;
                        let attribute = pop(&mut stack)?.attributes()?;
                        execution
                            .state
                            .set_expression(if matches!(source, Value::Reference(_)) {
                                source
                            } else {
                                Value::String(value.clone())
                            });
                        if execution.state.entity_update.get() {
                            attribute.write(&value, execution.host)?;
                        } else {
                            attribute.set_value(&value)?;
                        }
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
                    Instruction::CompareSwitch => {
                        let case = pop(&mut stack)?;
                        let selector = stack
                            .last()
                            .context("script switch stack underflow")?
                            .clone();
                        // Native STACK_COMPARE uses the case expression as the
                        // left operand and retains the selector on SStack.
                        stack.push(case.apply_binary(Op::Eq, selector, &mut execution.fuel)?);
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
                    Instruction::JumpFalse(target, retain) => {
                        let value = pop(&mut stack)?;
                        if let Some(slot) = retain {
                            if *slot >= if_conditions.len() {
                                if_conditions.resize_with(*slot + 1, || None);
                            }
                            if_conditions[*slot] = Some(value.clone());
                        }
                        execution.state.set_expression(value);
                        if execution.state.expression_condition()? == 0 {
                            ip = *target;
                        }
                    }
                    Instruction::RestoreExpression(slot) => {
                        let value = if_conditions[*slot]
                            .take()
                            .context("script if condition was not retained")?;
                        execution.state.set_expression(value);
                    }
                    Instruction::JumpIfExpressionNonZero(target) => {
                        if execution.state.expression_condition()? != 0 {
                            ip = *target;
                        }
                    }
                    Instruction::Short(target, when_true) => {
                        let value = stack
                            .last_mut()
                            .context("script condition stack underflow")?;
                        execution.state.set_expression(value.clone());
                        if (execution.state.expression_condition()? != 0) == *when_true {
                            *value = value.bool_convert()?;
                            ip = *target;
                        }
                    }
                    Instruction::Call(_, count) | Instruction::DynamicCall(_, count) => {
                        ensure!(stack.len() >= *count, "script argument stack underflow");
                        let args = stack.split_off(stack.len() - count);
                        let dynamic_name;
                        let callee = match &located.op {
                            Instruction::Call(callee, _) => callee,
                            Instruction::DynamicCall(slot, _) => {
                                // CALL reads the name after evaluating the
                                // arguments, including any mutations they make.
                                let Value::String(name) =
                                    slot.variable(&locals, &execution.state.globals).value()?
                                else {
                                    bail!("dynamic call requires a string variable");
                                };
                                dynamic_name = name.to_ascii_lowercase();
                                &dynamic_name
                            }
                            _ => unreachable!(),
                        };
                        let result = self.invoke(callee, &args, execution, depth + 1)?;
                        if matches!(&located.op, Instruction::DynamicCall(..)) {
                            execution
                                .state
                                .set_expression(Value::Int(i32::from(result.kind() != Type::Void)));
                        }
                        stack.push(result);
                    }
                    Instruction::Return(value, switch_depth) => {
                        ensure!(
                            *value == (function.returns != Type::Void),
                            "{name} return value presence does not match declaration"
                        );
                        let mut result = if *value {
                            let result = pop(&mut stack)?;
                            execution.state.set_expression(result.clone());
                            result
                        } else {
                            Value::Void
                        };
                        // The retained FUNCTION_RETURN permits exactly this
                        // implicit mismatch, keeping the low int32 bits.
                        if function.returns == Type::Int && result.kind() == Type::Ptr {
                            result = result.convert(Type::Int)?;
                        }
                        ensure!(
                            result.kind() == function.returns,
                            "{name} return type mismatch: expected {:?}, got {:?}",
                            function.returns,
                            result.kind()
                        );
                        ensure!(
                            stack.len() == *switch_depth && targets.is_empty(),
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
            if let Some(value) = result.with_context(|| {
                format!(
                    "{name} {}:{}:{}",
                    located.file, located.line, located.column
                )
            })? {
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

#[cfg(test)]
mod address_tests;
#[cfg(test)]
mod source_tests;
