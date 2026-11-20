//! Runtime segments share function names, global slots and defines.
use crate::{
    Execution, Function, FunctionKind, FunctionSignature, Host, Program, Type, Value, Variable,
    compile::compile_segment,
    events::Handlers,
    source::{self, SourceLoader},
    storage::{Budget, Declaration, Slot, State},
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, rc::Rc};

pub(crate) struct GlobalDeclaration {
    pub name: String,
    pub index: usize,
    pub declaration: Declaration,
}

struct Global {
    owner: Option<usize>,
    kind: Type,
    variable: Variable,
}

struct BoundFunction {
    owner: Option<usize>,
    signature: FunctionSignature,
    body: Option<Rc<Function>>,
}

struct Segment {
    id: usize,
    name: Option<String>,
    bytes: usize,
    tokens: usize,
    files: usize,
    unload: bool,
}

#[derive(Default)]
pub(crate) struct Segments {
    next_id: usize,
    loaded: Vec<Segment>,
    functions: BTreeMap<String, BoundFunction>,
    global_indices: BTreeMap<String, usize>,
    globals: Vec<Global>,
    defines: BTreeMap<String, (usize, Value)>,
}

impl Segments {
    pub fn global_index(&self, name: &str) -> Option<usize> {
        self.global_indices.get(name).copied()
    }

    pub fn global_count(&self) -> usize {
        self.globals.len()
    }

    pub fn global(&self, name: &str) -> Option<Slot> {
        let index = self.global_index(name)?;
        let global = &self.globals[index];
        global.owner.map(|_| Slot::Global(index, global.kind))
    }

    pub fn variable(&self, index: usize) -> Result<Variable> {
        let global = self.globals.get(index).context("unknown script global")?;
        ensure!(global.owner.is_some(), "script global is not loaded");
        Ok(global.variable.clone())
    }

    pub fn signature(&self, name: &str) -> Option<FunctionSignature> {
        self.functions.get(name).map(|entry| entry.signature)
    }

    pub fn function(&self, name: &str) -> Result<(usize, Rc<Function>)> {
        let entry = self
            .functions
            .get(name)
            .with_context(|| format!("unknown function {name}"))?;
        match (&entry.owner, &entry.body) {
            (Some(owner), Some(body)) => Ok((*owner, Rc::clone(body))),
            _ => anyhow::bail!("extern script function {name} is not loaded"),
        }
    }

    pub fn define(&self, name: &str) -> Option<&Value> {
        self.defines.get(name).map(|(_, value)| value)
    }

    pub fn define_count(&self) -> usize {
        self.defines.len()
    }

    fn check_limits(&self, program: &Program) -> Result<()> {
        let (files, bytes, tokens) = self.loaded.iter().fold(
            (program.sources.len(), program.bytes, program.raw_tokens),
            |(files, bytes, tokens), segment| {
                (
                    files + segment.files,
                    bytes + segment.bytes,
                    tokens + segment.tokens,
                )
            },
        );
        ensure!(
            files <= program.limits.files,
            "loaded script file count limit"
        );
        ensure!(
            bytes <= program.limits.total_bytes,
            "loaded script byte limit"
        );
        ensure!(tokens <= program.limits.tokens, "loaded script token limit");
        let new_functions = program
            .functions
            .keys()
            .filter(|name| !self.functions.contains_key(*name))
            .count();
        ensure!(
            self.functions.len() + new_functions <= program.limits.functions,
            "linked script function limit"
        );
        Ok(())
    }

    pub fn allocate(
        program: &Program,
        world: &crate::World,
        budget: &Budget,
    ) -> Result<Vec<Variable>> {
        program
            .globals
            .iter()
            .map(|global| global.declaration.allocate(world, budget))
            .collect()
    }

    pub fn insert(&mut self, program: &Program, variables: Vec<Variable>) {
        let id = self.next_id;
        self.next_id += 1;
        for (global, variable) in program.globals.iter().zip(variables) {
            let entry = Global {
                owner: Some(id),
                kind: global.declaration.kind,
                variable,
            };
            if let Some(previous) = self.globals.get_mut(global.index) {
                *previous = entry;
            } else {
                self.global_indices
                    .insert(global.name.clone(), global.index);
                self.globals.push(entry);
            }
        }
        for (name, body) in &program.functions {
            self.functions.insert(
                name.clone(),
                BoundFunction {
                    owner: Some(id),
                    signature: body.signature(),
                    body: (body.kind != FunctionKind::Extern).then(|| Rc::clone(body)),
                },
            );
        }
        self.defines.extend(
            program
                .defines
                .iter()
                .map(|(name, value)| (name.clone(), (id, value.clone()))),
        );
        self.loaded.push(Segment {
            id,
            name: program.entry.clone(),
            bytes: program.bytes,
            tokens: program.raw_tokens,
            files: program.sources.len(),
            unload: false,
        });
    }

    fn install_handlers(&self, program: &Program, handlers: &mut Handlers) -> Result<()> {
        handlers.register_segment(&program.static_handlers, |function| {
            if let Some(body) = program.functions.get(function) {
                ensure!(
                    body.kind != FunctionKind::Extern,
                    "event handler is not loaded: {function}"
                );
                Ok(self.next_id)
            } else {
                self.function(function).map(|(owner, _)| owner)
            }
        })
    }

    fn lookup(&self, name: &str) -> Option<&Segment> {
        self.loaded
            .iter()
            .find(|segment| segment.name.as_deref() == Some(name))
    }

    fn unload(&mut self, id: usize, handlers: &mut Handlers) {
        for segment in &mut self.loaded {
            if segment.id == id {
                segment.unload = true;
                break;
            }
        }
        for function in self
            .functions
            .values_mut()
            .filter(|entry| entry.owner == Some(id))
        {
            function.owner = None;
            function.signature.kind = FunctionKind::Extern;
            function.body = None;
        }
        for global in self
            .globals
            .iter_mut()
            .filter(|entry| entry.owner == Some(id))
        {
            // VarTable keeps DATA for existing references until the name is loaded again.
            global.owner = None;
        }
        self.defines.retain(|_, (owner, _)| *owner != id);
        handlers.unload_segment(id);
    }

    pub fn reap(&mut self) {
        let mut index = 0;
        while index < self.loaded.len() {
            if self.loaded[index].unload {
                self.loaded.remove(index);
                // COMPILER::ProcessFrame resets its loop index to zero before incrementing.
                index = 1;
            } else {
                index += 1;
            }
        }
    }
}

struct SegmentSource<'a, H>(&'a mut H);

impl<H: Host> SourceLoader for SegmentSource<'_, H> {
    fn load(&mut self, path: &str) -> Result<String> {
        self.0.load_script_source(path)
    }
}

fn load(name: &str, program: &Program, execution: &mut Execution<'_, impl Host>) -> Result<()> {
    let scope = &execution.state.segments;
    if scope.borrow().lookup(name).is_some() {
        return Ok(());
    }
    let sources =
        source::load_with_limits(name, &mut SegmentSource(execution.host), program.limits)?;
    let segment = compile_segment(sources, &scope.borrow())?;
    scope.borrow().check_limits(&segment)?;
    State::load_libraries(&segment, execution.host)?;
    let variables = Segments::allocate(&segment, &execution.world, &execution.state.budget)?;
    let mut scope = scope.borrow_mut();
    scope.install_handlers(&segment, execution.events)?;
    scope.insert(&segment, variables);
    Ok(())
}

pub(crate) fn call(
    name: &str,
    argument: &Value,
    program: &Program,
    execution: &mut Execution<'_, impl Host>,
) -> Result<Value> {
    let argument = argument.resolved()?;
    let argument = argument.text()?;
    let scope = &execution.state.segments;
    match name {
        "loadsegment" => match load(argument, program, execution) {
            Ok(()) => Ok(Value::Int(1)),
            Err(error) => {
                execution
                    .host
                    .script_diagnostic(&format!("LoadSegment {argument:?}: {error:#}"));
                Ok(Value::Int(0))
            }
        },
        "segmentisloaded" => Ok(Value::Int(i32::from(
            scope.borrow().lookup(argument).is_some(),
        ))),
        "unloadsegment" => {
            let mut scope = scope.borrow_mut();
            if let Some(id) = scope.lookup(argument).map(|segment| segment.id) {
                scope.unload(id, execution.events);
            }
            Ok(Value::Void)
        }
        "checkfunction" => {
            let function = argument.to_ascii_lowercase();
            Ok(Value::Int(i32::from(
                crate::builtins::arity(&function).is_some()
                    || scope.borrow().signature(&function).is_some(),
            )))
        }
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompileLimits, Limits, Runtime};

    #[test]
    fn dependent_code_reuses_global_slot_after_reload() -> Result<()> {
        struct Files;
        impl Host for Files {
            fn call(&mut self, name: &str, _: &[Value]) -> Result<Value> {
                anyhow::bail!("unexpected host call {name}")
            }
            fn load_script_source(&mut self, path: &str) -> Result<String> {
                Ok(match path {
                    "owner.c" => "int shared = 5;",
                    "reader.c" => "int ReadValue() { shared++; return shared; }",
                    _ => anyhow::bail!("missing source {path}"),
                }
                .into())
            }
        }
        let mut runtime = Runtime::compile(
            r#"
            extern int ReadValue();
            int LoadOwner() { return LoadSegment("owner.c"); }
            int LoadReader() { return LoadSegment("reader.c"); }
            void RemoveOwner() { UnloadSegment("owner.c"); }
            "#,
        )?;
        let mut host = Files;
        let limits = Limits::default();
        assert_eq!(
            runtime.call("LoadOwner", &[], &mut host, limits)?,
            Value::Int(1)
        );
        assert_eq!(
            runtime.call("LoadReader", &[], &mut host, limits)?,
            Value::Int(1)
        );
        assert_eq!(
            runtime.call("ReadValue", &[], &mut host, limits)?,
            Value::Int(6)
        );
        runtime.call("RemoveOwner", &[], &mut host, limits)?;
        let error = runtime
            .call("ReadValue", &[], &mut host, limits)
            .unwrap_err();
        assert!(format!("{error:#}").contains("script global is not loaded"));
        runtime.advance_frame(0, &mut host, limits)?;
        assert_eq!(
            runtime.call("LoadOwner", &[], &mut host, limits)?,
            Value::Int(1)
        );
        assert_eq!(
            runtime.call("ReadValue", &[], &mut host, limits)?,
            Value::Int(6)
        );
        Ok(())
    }

    #[test]
    fn failed_install_leaves_functions_globals_and_events_unchanged() -> Result<()> {
        struct Files;
        impl Host for Files {
            fn call(&mut self, name: &str, _: &[Value]) -> Result<Value> {
                anyhow::bail!("unexpected host call {name}")
            }
            fn load_script_source(&mut self, path: &str) -> Result<String> {
                Ok(match path {
                    "over-budget.c" => {
                        r#"
                        #event_handler("existing", "Candidate");
                        #event_handler("another", "Candidate");
                        int scratch;
                        void Candidate() { Existing(); }
                    "#
                    }
                    "retry.c" => "int scratch = 9; int Candidate() { return scratch; }",
                    _ => anyhow::bail!("missing source {path}"),
                }
                .into())
            }
        }
        let mut runtime = Runtime::compile_with_limits(
            r#"
            #event_handler("existing", "Existing");
            int calls;
            void Existing() { calls++; }
            int ReadCalls() { return calls; }
            int FailLoad() { return LoadSegment("over-budget.c"); }
            int Retry() { return LoadSegment("retry.c"); }
            "#,
            CompileLimits {
                event_handlers: 2,
                ..CompileLimits::default()
            },
        )?;
        let mut host = Files;
        let limits = Limits::default();
        assert_eq!(
            runtime.call("FailLoad", &[], &mut host, limits)?,
            Value::Int(0)
        );
        runtime.emit(
            "existing",
            &mut storm_core::Message::new("", vec![])?,
            &mut host,
            limits,
        )?;
        assert_eq!(
            runtime.call("ReadCalls", &[], &mut host, limits)?,
            Value::Int(1)
        );
        assert_eq!(
            runtime.call("Retry", &[], &mut host, limits)?,
            Value::Int(1)
        );
        assert_eq!(
            runtime.call("Candidate", &[], &mut host, limits)?,
            Value::Int(9)
        );
        Ok(())
    }
}
