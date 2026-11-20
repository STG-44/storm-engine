use crate::segments::{GlobalDeclaration, Segments};
use crate::storage::{Declaration, MAX_ELEMENTS, Slot};
use crate::{
    Access, AttributeMode, CallSite, Function, FunctionKind, Instruction, Located, Program, Type,
    Value, builtins, value::Op,
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, rc::Rc};

pub(crate) const MAX_TOKENS: usize = 16384;
const MAX_NESTING: usize = 64;
pub(crate) const MAX_LOCALS: usize = 128;

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatementScope {
    Braced,
    Unbraced,
    ForInitializer,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Token {
    Word(String),
    AttributeName(String),
    Literal(Value),
    Boolean(bool),
    Symbol(String),
    Directive(String),
    Definition(String, Value),
    End,
}

#[derive(Clone)]
pub(crate) struct Lexeme {
    pub token: Token,
    pub file: Rc<str>,
    pub line: usize,
    pub column: usize,
}

pub(crate) fn lex(
    source: &str,
    file: Rc<str>,
    limits: crate::CompileLimits,
) -> Result<Vec<Lexeme>> {
    ensure!(
        source.len() <= limits.file_bytes && !source.contains('\0'),
        "script source limit or NUL"
    );
    let mut result: Vec<Lexeme> = Vec::new();
    let bytes = source.as_bytes();
    let (mut p, mut line, mut column) = (0, 1, 1);
    let mut previous_cr = false;
    while p < bytes.len() {
        let start = p;
        let location = (line, column);
        let token = if bytes[p] <= b' ' || !bytes[p].is_ascii() {
            // Original TOKEN::StartArgument tests signed char <= ' ', skipping
            // high-bit UTF-8 bytes at boundaries, including the NBSP used by
            // unchanged game scripts. Quoted/comment contents stay untouched.
            p += source[p..].chars().next().unwrap().len_utf8();
            None
        } else if source[p..].starts_with("//") {
            while p < bytes.len() && !matches!(bytes[p], b'\r' | b'\n') {
                p += 1;
            }
            None
        } else if source[p..].starts_with("/*") {
            p += 2;
            let n = source[p..]
                .find("*/")
                .with_context(|| format!("{line}:{column}: unterminated comment"))?;
            p += n + 2;
            None
        } else if bytes[p] == b'#' {
            p += 1;
            let name = p;
            while p < bytes.len() && (bytes[p].is_ascii_alphabetic() || bytes[p] == b'_') {
                p += 1;
            }
            ensure!(p > name, "{line}:{column}: expected directive name");
            let name = source[name..p].to_ascii_lowercase();
            if matches!(name.as_str(), "ifndef" | "endif") {
                // Native TOKEN maps these spellings to LINE_COMMENT. They are
                // header decoration, not conditional C preprocessing.
                while p < bytes.len() && !matches!(bytes[p], b'\r' | b'\n') {
                    p += 1;
                }
                None
            } else {
                Some(Token::Directive(name))
            }
        } else if bytes[p] == b'"' {
            p += 1;
            let n = source[p..]
                .find('"')
                .with_context(|| format!("{line}:{column}: unterminated string"))?;
            let value = Value::String(source[p..p + n].to_owned());
            value
                .validate()
                .with_context(|| format!("{line}:{column}"))?;
            p += n + 1;
            Some(Token::Literal(value))
        } else if bytes[p].is_ascii_alphabetic() || bytes[p] == b'_' {
            p += 1;
            while p < bytes.len() && (bytes[p].is_ascii_alphanumeric() || bytes[p] == b'_') {
                p += 1;
            }
            let word = source[start..p].to_ascii_lowercase();
            Some(match word.as_str() {
                "true" => Token::Boolean(true),
                "false" => Token::Boolean(false),
                _ => Token::Word(word),
            })
        } else if bytes[p].is_ascii_digit()
            && result.last().is_some_and(
                |previous| matches!(&previous.token, Token::Symbol(symbol) if symbol == "."),
            )
        {
            while p < bytes.len() && bytes[p].is_ascii_digit() {
                p += 1;
            }
            ensure!(
                bytes.get(p) != Some(&b'.'),
                "{line}:{column}: unsupported adjacent dot after numeric attribute name"
            );
            while p < bytes.len() && (bytes[p].is_ascii_alphanumeric() || bytes[p] == b'_') {
                p += 1;
            }
            // The native compiler passes the token text to the attribute
            // codec, so leading zeros are significant.
            Some(Token::AttributeName(source[start..p].to_ascii_lowercase()))
        } else if bytes[p].is_ascii_digit() {
            while p < bytes.len() && bytes[p].is_ascii_digit() {
                p += 1;
            }
            let floating = p < bytes.len() && bytes[p] == b'.';
            if floating
                && bytes
                    .get(p + 1)
                    .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
            {
                return Err(anyhow::anyhow!(
                    "{line}:{column}: numeric attribute names require a dynamic path"
                ));
            }
            if floating {
                p += 1;
                while p < bytes.len() && bytes[p].is_ascii_digit() {
                    p += 1;
                }
            }
            let value = if floating {
                Value::Float(
                    // CompileFloatNumber and #define use atof (double), then float.
                    source[start..p]
                        .parse::<f64>()
                        .with_context(|| format!("{line}:{column}: float literal"))?
                        as f32,
                )
            } else {
                Value::Int(
                    source[start..p]
                        .parse::<u32>()
                        .with_context(|| format!("{line}:{column}: 32-bit integer literal"))?
                        as i32,
                )
            };
            Some(Token::Literal(value))
        } else {
            let pair = bytes
                .get(p..p + 2)
                .and_then(|s| std::str::from_utf8(s).ok());
            if pair.is_some_and(|s| {
                [
                    "==", "!=", "<=", ">=", "&&", "||", "++", "--", "+=", "-=", "*=", "/=",
                ]
                .contains(&s)
            }) {
                p += 2;
            } else {
                ensure!(
                    b"{}[]();.,:+-*/%^=!<>&".contains(&bytes[p]),
                    "{line}:{column}: unsupported character"
                );
                p += 1;
            }
            Some(Token::Symbol(source[start..p].to_owned()))
        };
        for c in source[start..p].chars() {
            if c == '\r' || c == '\n' {
                if c == '\r' || !previous_cr {
                    line += 1;
                }
                column = 1;
            } else {
                column += 1;
            }
            previous_cr = c == '\r';
        }
        if let Some(token) = token {
            ensure!(result.len() < limits.tokens, "script token limit");
            result.push(Lexeme {
                token,
                file: file.clone(),
                line: location.0,
                column: location.1,
            });
        }
    }
    result.push(Lexeme {
        token: Token::End,
        file,
        line,
        column,
    });
    Ok(result)
}

struct Loop {
    // A switch owns a retained operand but supplies no continue target.
    start: Option<usize>,
    breaks: Vec<usize>,
}

struct Compiler<'a> {
    limits: crate::CompileLimits,
    tokens: Vec<Lexeme>,
    p: usize,
    code: Vec<Located>,
    locals: BTreeMap<String, usize>,
    types: Vec<Declaration>,
    globals: BTreeMap<String, Slot>,
    declarations: Vec<GlobalDeclaration>,
    next_global: usize,
    constants: BTreeMap<String, (Value, usize)>,
    inherited: &'a Segments,
    loops: Vec<Loop>,
    nesting: usize,
}

pub(crate) fn compile(sources: crate::source::Sources) -> Result<Program> {
    compile_segment(sources, &Segments::default())
}

pub(crate) fn compile_segment(
    sources: crate::source::Sources,
    inherited: &Segments,
) -> Result<Program> {
    let mut c = Compiler {
        limits: sources.limits,
        tokens: sources.tokens,
        p: 0,
        code: Vec::new(),
        locals: BTreeMap::new(),
        types: Vec::new(),
        globals: BTreeMap::new(),
        declarations: Vec::new(),
        next_global: inherited.global_count(),
        constants: BTreeMap::new(),
        inherited,
        loops: Vec::new(),
        nesting: 0,
    };
    // The retained compiler registers globals before executing function bodies.
    // Collect top-level declarations before resolving any function references.
    let mut headers = Vec::new();
    let mut names = BTreeMap::new();
    let mut static_handlers = Vec::new();
    let mut top_level_calls = Vec::new();
    while c.current() != &Token::End {
        if c.is_static_handler() {
            static_handlers.push(c.static_handler()?);
            continue;
        }
        if let Token::Definition(name, value) = c.current().clone() {
            ensure!(
                c.constants.len() + inherited.define_count() < c.limits.constants,
                "script constant limit"
            );
            ensure!(
                c.constant(&name).is_none(),
                "{}",
                c.error("define redefinition")
            );
            c.constants.insert(name, (value, c.p));
            c.p += 1;
            continue;
        }
        if c.take(";") {
            continue;
        }
        if matches!(c.current(), Token::Word(_))
            && matches!(&c.tokens[c.p + 1].token, Token::Symbol(symbol) if symbol == "(")
        {
            top_level_calls.push(c.p);
            c.word()?;
            c.expect("(")?;
            let mut depth = 1;
            while depth != 0 {
                if c.take("(") {
                    depth += 1;
                } else if c.take(")") {
                    depth -= 1;
                } else {
                    ensure!(
                        c.current() != &Token::End,
                        "{}",
                        c.error("unterminated top-level call")
                    );
                    c.p += 1;
                }
            }
            c.expect(";")?;
            continue;
        }
        let start = c.p;
        let function_kind = c.function_kind();
        let kind = c.kind()?;
        let name = c.word()?;
        if c.take("(") {
            ensure!(headers.len() < c.limits.functions, "script function limit");
            ensure!(
                builtins::arity(&name).is_none(),
                "builtin function cannot be redeclared: {name}"
            );
            ensure!(
                inherited
                    .signature(&name)
                    .is_none_or(|signature| signature.kind == FunctionKind::Extern),
                "duplicate function {name}"
            );
            // The original function table can replace an unloaded extern
            // entry with another declaration or a loaded definition.
            ensure!(
                matches!(
                    names.insert(name.clone(), function_kind),
                    None | Some(FunctionKind::Extern)
                ),
                "duplicate function {name}"
            );
            while !c.take(")") {
                ensure!(c.current() != &Token::End, "unterminated parameter list");
                c.p += 1;
            }
            if function_kind != FunctionKind::Script {
                c.expect(";")?;
            } else {
                c.function_body_start()?;
                let mut depth = 1;
                while depth != 0 {
                    ensure!(
                        c.current() != &Token::End,
                        "{}",
                        c.error("unterminated function body")
                    );
                    ensure!(
                        !matches!(c.current(), Token::Definition(_, _)),
                        "{}",
                        c.error("function-scope #define is not supported")
                    );
                    if c.is_static_handler() {
                        static_handlers.push(c.static_handler()?);
                    } else if c.take("{") {
                        depth += 1;
                    } else if c.take("}") {
                        depth -= 1;
                    } else {
                        c.p += 1;
                    }
                }
            }
            headers.push((start, c.p));
        } else {
            ensure!(
                function_kind == FunctionKind::Script && kind != Type::Void,
                "{}",
                c.error("invalid global declaration")
            );
            let mut name = name;
            loop {
                let elements = c.elements()?;
                let initial = c.initializer(kind, elements)?;
                ensure!(
                    !c.globals.contains_key(&name) && inherited.global(&name).is_none(),
                    "duplicate global {name}"
                );
                let index = if let Some(index) = inherited.global_index(&name) {
                    index
                } else {
                    ensure!(c.next_global < c.limits.globals, "script global limit");
                    let index = c.next_global;
                    c.next_global += 1;
                    index
                };
                c.globals.insert(name.clone(), Slot::Global(index, kind));
                c.declarations.push(GlobalDeclaration {
                    name,
                    index,
                    declaration: Declaration {
                        kind,
                        elements,
                        initial,
                    },
                });
                if !c.take(",") {
                    break;
                }
                name = c.word()?;
            }
            c.expect(";")?;
        }
    }
    let mut functions = BTreeMap::new();
    for (start, end) in headers {
        c.p = start;
        let kind = c.function_kind();
        let returns = c.kind()?;
        let name = c.word()?;
        c.expect("(")?;
        let mut parameters = 0;
        if !c.take(")") {
            loop {
                let kind = c.kind()?;
                ensure!(kind != Type::Void, "void parameter");
                let name = c.word()?;
                c.local(name, kind, 1)?;
                parameters += 1;
                if c.take(")") {
                    break;
                }
                c.expect(",")?;
            }
        }
        if kind != FunctionKind::Script {
            c.expect(";")?;
        } else {
            c.function_body_start()?;
            c.register_locals(end - 1)?;
            c.block()?;
            c.emit(Instruction::Return(false, 0));
        }
        functions.insert(
            name,
            Rc::new(Function {
                parameters,
                returns,
                kind,
                locals: std::mem::take(&mut c.types),
                code: std::mem::take(&mut c.code),
            }),
        );
        c.locals.clear();
    }
    // The original compiler emits these statements outside any function.
    // Startup begins at Main's function offset, leaving them unreachable.
    for start in top_level_calls {
        c.p = start;
        c.atom()?;
        c.mark_statement_call();
        c.expect(";")?;
    }
    let bodies = functions
        .iter()
        .map(|(name, function)| (name.as_str(), function.code.as_slice()))
        .chain(std::iter::once(("<top level>", c.code.as_slice())));
    for (name, code) in bodies {
        for instruction in code {
            if let Instruction::Call(callee, arguments, site) = &instruction.op {
                let (expected, defer_arity) = if let Some(count) = builtins::arity(callee) {
                    (count, false)
                } else {
                    let function = functions
                        .get(callee)
                        .map(|function| function.signature())
                        .or_else(|| inherited.signature(callee))
                        .with_context(|| {
                            format!(
                                "{name} {}:{}:{}: undeclared function {callee}",
                                instruction.file, instruction.line, instruction.column
                            )
                        })?;
                    // CompileExpression_L7 only enforces the empty argument
                    // list for zero-parameter functions. The Rust VM checks
                    // nonzero parameter lists before entering a frame.
                    let defer = *site == CallSite::Expression
                        && function.kind == FunctionKind::Script
                        && function.parameters != 0;
                    (function.parameters, defer)
                };
                ensure!(
                    if crate::events::signature(callee).is_some() {
                        crate::events::arguments_valid(callee, *arguments)
                    } else if crate::entity_api::signature(callee).is_some() {
                        crate::entity_api::arguments_valid(callee, *arguments)
                    } else {
                        defer_arity || *arguments == expected
                    },
                    "{name} {}:{}:{}: {callee} expects {} arguments, got {arguments}",
                    instruction.file,
                    instruction.line,
                    instruction.column,
                    expected
                );
            }
        }
    }
    let mut initial_handlers = Vec::new();
    let mut registered = crate::events::Handlers::with_limits(c.limits.into());
    for (event, function, origin) in static_handlers {
        let location = format!(
            "{}:{}:{}: #event_handler",
            origin.file, origin.line, origin.column
        );
        let event = match event {
            Token::Literal(Value::String(event)) => event,
            Token::Word(name) => match c.constant(&name) {
                Some((Value::String(event), _)) => event.clone(),
                _ => anyhow::bail!("{location}: event name requires a string define"),
            },
            _ => unreachable!(),
        };
        let handler = functions
            .get(&function)
            .map(|function| function.signature())
            .or_else(|| inherited.signature(&function))
            .with_context(|| format!("{location}: unknown event handler function {function}"))?;
        ensure!(
            handler.parameters == 0,
            "{location}: event handler must take no parameters"
        );
        registered
            .register_static(&event, &function, 0)
            .with_context(|| location.clone())?;
        initial_handlers.push((event, function));
    }
    Ok(Program {
        functions,
        globals: c.declarations,
        defines: c
            .constants
            .into_iter()
            .map(|(name, (value, _))| (name, value))
            .collect(),
        sources: sources.files,
        static_handlers: initial_handlers,
        libraries: sources.libraries,
        entry: sources.entry,
        bytes: sources.bytes,
        raw_tokens: sources.raw_tokens,
        limits: sources.limits,
    })
}

impl Compiler<'_> {
    fn function_body_start(&mut self) -> Result<()> {
        // SEPARATOR leaves the original function declaration open until its
        // body starts, as in GetColonyByIndex in the upstream game.
        while self.take(";") {}
        self.expect("{")
    }

    fn function_kind(&mut self) -> FunctionKind {
        if self.take("extern") {
            FunctionKind::Extern
        } else if self.take("native") {
            FunctionKind::Native
        } else {
            FunctionKind::Script
        }
    }

    fn is_static_handler(&self) -> bool {
        matches!(self.current(), Token::Directive(name) if name == "event_handler")
    }
    fn static_handler(&mut self) -> Result<(Token, String, Lexeme)> {
        let origin = self.tokens[self.p].clone();
        self.p += 1;
        self.expect("(")?;
        let event = self.current().clone();
        ensure!(
            matches!(event, Token::Literal(Value::String(_)) | Token::Word(_)),
            "{}",
            self.error("event name requires a string literal or define")
        );
        self.p += 1;
        self.expect(",")?;
        let Token::Literal(Value::String(function)) = self.current() else {
            return Err(self.error("handler function requires a string literal"));
        };
        let function = function.to_ascii_lowercase();
        self.p += 1;
        self.expect(")")?;
        Ok((event, function, origin))
    }
    fn current(&self) -> &Token {
        &self.tokens[self.p].token
    }
    fn error(&self, message: &str) -> anyhow::Error {
        let t = &self.tokens[self.p];
        anyhow::anyhow!(
            "{}:{}:{}: {message}; found {:?}",
            t.file,
            t.line,
            t.column,
            t.token
        )
    }
    fn is(&self, word: &str) -> bool {
        matches!(self.current(), Token::Word(s) | Token::Symbol(s) if s == word)
    }
    fn take(&mut self, word: &str) -> bool {
        if self.is(word) {
            self.p += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, word: &str) -> Result<()> {
        if !self.take(word) {
            return Err(self.error(&format!("expected '{word}'")));
        }
        Ok(())
    }
    fn word(&mut self) -> Result<String> {
        let Token::Word(s) = self.current() else {
            return Err(self.error("expected identifier"));
        };
        ensure!(
            ![
                "int", "bool", "float", "string", "void", "native", "return", "if", "else",
                "while", "break", "continue", "for", "object", "ref", "aref", "ptr", "switch",
                "goto", "call", "makeref", "makearef", "extern", "case"
            ]
            .contains(&s.as_str()),
            "{}",
            self.error("reserved identifier")
        );
        let result = s.clone();
        self.p += 1;
        Ok(result)
    }
    fn kind(&mut self) -> Result<Type> {
        let result = if self.take("int") || self.take("bool") {
            Type::Int
        } else if self.take("float") {
            Type::Float
        } else if self.take("ptr") {
            Type::Ptr
        } else if self.take("string") {
            Type::String
        } else if self.take("object") {
            Type::Object
        } else if self.take("ref") {
            Type::Reference
        } else if self.take("aref") {
            Type::AReference
        } else if self.take("void") {
            Type::Void
        } else {
            return Err(self.error("unsupported declaration type"));
        };
        Ok(result)
    }
    fn emit(&mut self, op: Instruction) -> usize {
        let token = &self.tokens[self.p.saturating_sub(1)];
        let index = self.code.len();
        self.code.push(Located {
            op,
            file: token.file.clone(),
            line: token.line,
            column: token.column,
        });
        index
    }
    fn patch(&mut self, index: usize) {
        let target = self.code.len();
        match &mut self.code[index].op {
            Instruction::Jump(t)
            | Instruction::JumpFalse(t, _)
            | Instruction::JumpIfExpressionNonZero(t)
            | Instruction::Short(t, _) => *t = target,
            _ => unreachable!(),
        }
    }
    fn local(&mut self, name: String, kind: Type, elements: usize) -> Result<usize> {
        ensure!(self.types.len() < self.limits.locals, "script local limit");
        ensure!(
            !self.locals.contains_key(&name),
            "{}",
            self.error("duplicate local")
        );
        let index = self.types.len();
        self.locals.insert(name, index);
        self.types.push(Declaration {
            kind,
            elements,
            initial: Vec::new(),
        });
        Ok(index)
    }
    fn local_declarator(&mut self) -> Result<(String, usize)> {
        let name = self.word()?;
        let elements = self.elements()?;
        Ok((name, elements))
    }
    fn register_locals(&mut self, end: usize) -> Result<()> {
        // Native AddFuncVar runs before CompileBlock. Initializers remain at
        // their own execution sites, including unreachable nested branches.
        let start = self.p;
        while self.p < end {
            if !self.is_local_type_token() {
                self.p += 1;
                continue;
            }
            let kind = self.kind()?;
            loop {
                let (name, elements) = self.local_declarator()?;
                self.local(name, kind, elements)?;
                if self.take("=") {
                    self.skip_local_initializer(end);
                }
                if !self.take(",") {
                    break;
                }
            }
        }
        self.p = start;
        Ok(())
    }
    fn is_local_type_token(&self) -> bool {
        self.starts_local_declaration()
            && !matches!(&self.tokens[self.p - 1].token, Token::Symbol(symbol) if symbol == ".")
    }
    fn skip_local_initializer(&mut self, end: usize) {
        let mut depth = 0usize;
        while self.p < end {
            if self.is(";")
                || self.is("{")
                || self.is("}")
                || depth == 0 && (self.is(",") || self.is_local_type_token())
            {
                break;
            }
            if self.is("(") || self.is("[") {
                depth += 1;
            } else if self.is(")") || self.is("]") {
                depth = depth.saturating_sub(1);
            }
            self.p += 1;
        }
    }
    fn enter(&mut self) -> Result<()> {
        self.nesting += 1;
        ensure!(
            self.nesting <= MAX_NESTING,
            "{}",
            self.error("script nesting limit")
        );
        Ok(())
    }
    fn block(&mut self) -> Result<()> {
        loop {
            while self.is_static_handler() {
                self.static_handler()?;
            }
            if self.take("}") {
                return Ok(());
            }
            ensure!(
                self.current() != &Token::End,
                "{}",
                self.error("unterminated block")
            );
            self.statement(StatementScope::Braced)?;
        }
    }
    fn statement(&mut self, scope: StatementScope) -> Result<()> {
        self.enter()?;
        self.statement_inner(scope)?;
        self.nesting -= 1;
        Ok(())
    }
    fn control_body(&mut self) -> Result<()> {
        if self.is("{") {
            return self.statement(StatementScope::Braced);
        }
        // The native compiler recursively compiles each nested control
        // statement. Its separator belongs to that nested body. Only a
        // separator read by this CompileBlock invocation ends this body.
        loop {
            while self.is_static_handler() {
                self.static_handler()?;
            }
            if self.take(";") {
                return Ok(());
            }
            if self.take(")") {
                continue;
            }
            ensure!(
                self.current() != &Token::End && !self.is("}"),
                "{}",
                self.error("expected ';' ending unbraced body")
            );
            self.statement(StatementScope::Unbraced)?;
        }
    }
    fn statement_inner(&mut self, scope: StatementScope) -> Result<()> {
        let in_block = scope == StatementScope::Braced;
        let allow_handoff = scope != StatementScope::ForInitializer;
        // Directives emit no instruction and do not supply a control-flow body.
        // Preserve the following semicolon as the actual empty statement.
        while self.is_static_handler() {
            self.static_handler()?;
        }
        if self.take("{") {
            return self.block();
        }
        if self.take(";") {
            return Ok(());
        }
        if self.take("call") {
            self.dynamic_call(false)?;
            self.statement_end(allow_handoff)?;
            self.emit(Instruction::Drop);
            return Ok(());
        }
        if self.is("makeref") || self.is("makearef") {
            let attribute = self.take("makearef");
            if !attribute {
                self.expect("makeref")?;
            }
            self.expect("(")?;
            let destination = self.target()?;
            self.expect(",")?;
            self.target()?;
            if attribute {
                ensure!(
                    matches!(
                        self.slot_type(destination),
                        Type::Reference | Type::AReference
                    ),
                    "makearef requires aref/ref destination"
                );
                let path = self.path()?;
                self.emit(Instruction::Attribute(path, AttributeMode::Bind));
                self.emit(Instruction::MakeARef);
            } else {
                ensure!(
                    self.slot_type(destination) == Type::Reference,
                    "makeref requires ref destination"
                );
                self.emit(Instruction::MakeRef);
            }
            self.expect(")")?;
            return self.statement_end(allow_handoff);
        }
        if self.take("return") {
            let value = !self.is(";");
            if value {
                self.expression(1)?;
                // FUNCTION_RETURN emits the return before handing its
                // lookahead back to CompileBlock. Unmatched closing tokens
                // after that value are unreachable in a surrounding block.
                if in_block {
                    while self.take(")") {}
                }
            }
            let switches = self
                .loops
                .iter()
                .filter(|control| control.start.is_none())
                .count();
            self.emit(Instruction::Return(value, switches));
            // The original emits FUNCTION_RETURN before handing '=' back
            // to CompileBlock. It compiles the MOVE after the return.
            if value && self.take("=") {
                return self.previous_assignment(allow_handoff);
            }
            if !(value && in_block && (self.is("}") || self.is("break"))) {
                self.statement_end(false)?;
            }
            return Ok(());
        }
        if self.take("if") {
            self.expect("(")?;
            self.expression(1)?;
            self.expect(")")?;
            // IF keeps the original condition while the body can change EX.
            // Use a per-frame slot so break and continue can bypass restoration
            // without leaving extra values on the operand stack.
            let slot = self.nesting - 1;
            let conditional = self.emit(Instruction::JumpFalse(0, Some(slot)));
            self.control_body()?;
            self.patch(conditional);
            self.emit(Instruction::RestoreExpression(slot));
            return Ok(());
        }
        if self.take("else") {
            return self.else_body();
        }
        if self.take("while") {
            let start = self.code.len();
            self.expect("(")?;
            self.expression(1)?;
            self.expect(")")?;
            let end = self.emit(Instruction::JumpFalse(0, None));
            self.loops.push(Loop {
                start: Some(start),
                breaks: Vec::new(),
            });
            self.control_body()?;
            self.emit(Instruction::Jump(start));
            self.patch(end);
            for index in self.loops.pop().unwrap().breaks {
                self.patch(index);
            }
            return Ok(());
        }
        if self.take("for") {
            return self.for_statement();
        }
        if self.take("switch") {
            return self.switch_statement();
        }
        if self.is("break") || self.is("continue") {
            let breaking = self.take("break");
            if !breaking {
                self.expect("continue")?;
            }
            ensure!(
                !self.loops.is_empty(),
                "{}",
                self.error("control statement outside loop or switch")
            );
            // Native CASE_COMMAND stops its body as soon as BREAK_COMMAND
            // emits the exit jump, leaving any separator to the outer block.
            let switch_break = breaking
                && in_block
                && self
                    .loops
                    .last()
                    .is_some_and(|control| control.start.is_none())
                && (self.is("case") || self.is("default") || self.is("}"));
            if !switch_break {
                self.statement_end(allow_handoff)?;
            }
            if breaking {
                let index = self.emit(Instruction::Jump(0));
                self.loops.last_mut().unwrap().breaks.push(index);
            } else {
                let index = self
                    .loops
                    .iter()
                    .rposition(|control| control.start.is_some())
                    .with_context(|| self.error("continue outside loop"))?;
                let target = self.loops[index].start.unwrap();
                // Native switch propagates continue fixups to the enclosing
                // loop. Our private VM must also release retained operands.
                for _ in index + 1..self.loops.len() {
                    self.emit(Instruction::Drop);
                }
                self.emit(Instruction::Jump(target));
            }
            return Ok(());
        }
        if self.starts_local_declaration() {
            let kind = self.kind()?;
            loop {
                let (name, elements) = self.local_declarator()?;
                let index = *self.locals.get(&name).with_context(|| {
                    self.error(&format!("unregistered local declaration {name}"))
                })?;
                ensure!(
                    self.types[index].kind == kind && self.types[index].elements == elements,
                    "{}",
                    self.error("local declaration differs from its first pass")
                );
                if self.take("=") {
                    ensure!(elements <= 1, "local array initializer is not supported");
                    self.emit(Instruction::Select(Slot::Local(index)));
                    self.expression(1)?;
                    self.emit(Instruction::Store(true));
                }
                if !self.take(",") {
                    return self.statement_end(allow_handoff);
                }
            }
        }
        if let Token::Word(name) = self.current() {
            let next = &self.tokens[self.p + 1].token;
            if self.lookup(name).is_some()
                && matches!(next, Token::Symbol(s) if ["[", ".", "=", "+=", "-=", "*=", "/=", "++", "--"].contains(&s.as_str()))
            {
                let checkpoint = (self.p, self.code.len());
                self.target()?;
                let path = if self.is(".") {
                    Some(self.path()?)
                } else {
                    None
                };
                if path.is_some() {
                    // CompileBlock keeps the left operand across surplus
                    // closing parentheses after an attribute path.
                    while self.take(")") {}
                }
                if !["=", "+=", "-=", "*=", "/=", "++", "--"]
                    .iter()
                    .any(|s| self.is(s))
                {
                    self.p = checkpoint.0;
                    self.code.truncate(checkpoint.1);
                    self.expression(1)?;
                    self.statement_end(false)?;
                    self.emit(Instruction::Drop);
                    return Ok(());
                }
                let plain_assignment = self.is("=");
                if let Some(path) = path {
                    self.emit(Instruction::Attribute(path, AttributeMode::Write));
                    self.expect("=")?;
                    self.expression(1)?;
                    self.emit(Instruction::WriteAttribute);
                } else if self.take("++") || self.take("--") {
                    let decrement =
                        matches!(&self.tokens[self.p-1].token, Token::Symbol(s) if s == "--");
                    self.emit(Instruction::Increment(decrement));
                } else {
                    let op = if self.take("+=") {
                        Some(Op::Add)
                    } else if self.take("-=") {
                        Some(Op::Sub)
                    } else if self.take("*=") {
                        Some(Op::Mul)
                    } else if self.take("/=") {
                        Some(Op::Div)
                    } else {
                        self.expect("=")?;
                        None
                    };
                    if op.is_some() {
                        self.emit(Instruction::Load(true));
                    }
                    self.expression(1)?;
                    if let Some(op) = op {
                        self.emit(Instruction::Binary(op));
                        while self.take(")") {}
                    }
                    self.emit(Instruction::Store(op.is_none()));
                }
                return self.statement_end(allow_handoff && plain_assignment);
            }
        }
        let starts_with_call = self.starts_direct_call();
        self.expression(1)?;
        let direct_call = starts_with_call && self.mark_statement_call();
        if direct_call {
            while self.take(")") {}
            if self.take("=") {
                // CompileBlock emits the call and discards its result, then
                // compiles '=' as a MOVE to the frame's current left operand.
                self.emit(Instruction::Drop);
                return self.previous_assignment(allow_handoff);
            }
        }
        self.statement_end(allow_handoff && direct_call)?;
        self.emit(Instruction::Drop);
        Ok(())
    }
    fn previous_assignment(&mut self, allow_handoff: bool) -> Result<()> {
        self.expression(1)?;
        self.statement_end(allow_handoff)?;
        self.emit(Instruction::StorePrevious);
        Ok(())
    }
    fn statement_end(&mut self, allow_statement_boundary: bool) -> Result<()> {
        // Native declarations, assignments and direct calls can leave the
        // next statement token for the enclosing CompileBlock.
        // The enclosing block or for header owns the separator.
        if allow_statement_boundary {
            while self.take(")") {}
            // LABEL emits no bytecode in a braced CompileBlock. Some
            // original calls and assignments leave ':' at this boundary.
            if self.take(":")
                || self.starts_local_declaration()
                || self.starts_direct_call()
                || self.starts_plain_assignment()
                || [
                    "if", "else", "while", "for", "switch", "return", "break", "continue", "}",
                ]
                .iter()
                .any(|token| self.is(token))
            {
                return Ok(());
            }
        }
        ensure!(self.is(";"), "{}", self.error("expected ';'"));
        Ok(())
    }
    fn starts_direct_call(&self) -> bool {
        matches!(self.current(), Token::Word(_))
            && matches!(&self.tokens[self.p + 1].token, Token::Symbol(symbol) if symbol == "(")
    }
    fn mark_statement_call(&mut self) -> bool {
        let Some(Instruction::Call(_, _, site)) = self.code.last_mut().map(|entry| &mut entry.op)
        else {
            return false;
        };
        *site = CallSite::Statement;
        true
    }
    fn starts_plain_assignment(&self) -> bool {
        if !matches!(self.current(), Token::Word(name) if self.lookup(name).is_some()) {
            return false;
        }
        let token = |index| self.tokens.get(index).map(|lexeme: &Lexeme| &lexeme.token);
        let symbol =
            |index, expected: &str| matches!(token(index), Some(Token::Symbol(s)) if s == expected);
        let mut cursor = self.p + 1;
        if symbol(cursor, "[") {
            let mut depth = 1;
            cursor += 1;
            while depth != 0 {
                match token(cursor) {
                    Some(Token::Symbol(s)) if s == "[" => depth += 1,
                    Some(Token::Symbol(s)) if s == "]" => depth -= 1,
                    Some(Token::Symbol(s)) if matches!(s.as_str(), ";" | "{" | "}") => {
                        return false;
                    }
                    Some(Token::End) | None => return false,
                    _ => {}
                }
                cursor += 1;
            }
        }
        let has_path = symbol(cursor, ".");
        while symbol(cursor, ".") {
            cursor += 1;
            match token(cursor) {
                Some(Token::Word(_) | Token::AttributeName(_)) => cursor += 1,
                Some(Token::Symbol(s))
                    if s == "("
                        && matches!(token(cursor + 1), Some(Token::Word(_)))
                        && symbol(cursor + 2, ")") =>
                {
                    cursor += 3;
                }
                _ => return false,
            }
        }
        while has_path && symbol(cursor, ")") {
            cursor += 1;
        }
        symbol(cursor, "=")
    }
    fn else_body(&mut self) -> Result<()> {
        let end = self.emit(Instruction::JumpIfExpressionNonZero(0));
        if self.is("if") {
            // Native ELSE_BLOCK uses IF_BLOCK as the bound for else-if.
            self.statement(StatementScope::Unbraced)?;
        } else {
            self.control_body()?;
        }
        self.patch(end);
        Ok(())
    }
    fn starts_local_declaration(&self) -> bool {
        matches!(
            self.current(),
            Token::Word(name) if matches!(
                name.as_str(), "int" | "bool" | "float" | "string" | "object" | "ref" | "aref" | "ptr"
            )
        )
    }
    fn for_statement(&mut self) -> Result<()> {
        self.expect("(")?;
        ensure!(
            self.is(";")
                || matches!(self.current(), Token::Word(_))
                    && ![
                        "if", "else", "while", "for", "switch", "return", "break", "continue"
                    ]
                    .iter()
                    .any(|keyword| self.is(keyword)),
            "{}",
            self.error("invalid for initializer")
        );
        if !self.take(";") {
            self.statement(StatementScope::ForInitializer)?;
            self.expect(";")?;
        }
        let condition = self.code.len();
        self.expression(1)?;
        // FOR_BLOCK advances past the condition terminator before reading
        // the update variable. The island loader also uses a comma there.
        if !self.take(";") {
            self.expect(",")?;
        }
        let end = self.emit(Instruction::JumpFalse(0, None));
        let first_body = self.emit(Instruction::Jump(0));

        // Native FOR_BLOCK accepts a variable followed by postfix ++ or --.
        // Continue targets this update as well as ordinary body completion.
        let update = self.code.len();
        let slot = self.variable()?;
        let decrement = if self.take("--") {
            true
        } else {
            self.expect("++")?;
            false
        };
        self.expect(")")?;
        self.emit(Instruction::Select(slot));
        self.emit(Instruction::Increment(decrement));
        self.emit(Instruction::Jump(condition));
        self.patch(first_body);

        self.loops.push(Loop {
            start: Some(update),
            breaks: Vec::new(),
        });
        self.control_body()?;
        self.emit(Instruction::Jump(update));
        self.patch(end);
        for index in self.loops.pop().unwrap().breaks {
            self.patch(index);
        }
        Ok(())
    }

    fn switch_statement(&mut self) -> Result<()> {
        self.expect("(")?;
        self.expression(1)?;
        self.expect(")")?;
        self.expect("{")?;
        self.loops.push(Loop {
            start: None,
            breaks: Vec::new(),
        });
        let mut default_seen = false;
        while !self.take("}") {
            if self.take(";") {
                continue;
            }
            if self.is("break") {
                // A break outside any case exits the current switch on the
                // path where its preceding guards all missed.
                self.statement(StatementScope::Braced)?;
                continue;
            }
            ensure!(
                !default_seen,
                "{}",
                self.error("only a trailing default label is supported")
            );
            let guard = if self.take("case") {
                // Original first-pass CASE_COMMAND accepts one token followed
                // by ':'. Case expressions/fallthrough require a separate port.
                let start = self.p;
                ensure!(
                    matches!(self.current(), Token::Literal(_) | Token::Boolean(_))
                        || matches!(self.current(), Token::Word(name) if self.constant(name).is_some()),
                    "{}",
                    self.error("case requires a literal or define")
                );
                self.expression(1)?;
                ensure!(
                    self.p == start + 1,
                    "{}",
                    self.error("case requires one token")
                );
                self.expect(":")?;
                self.emit(Instruction::CompareSwitch);
                Some(self.emit(Instruction::JumpFalse(0, None)))
            } else {
                self.expect("default")?;
                self.expect(":")?;
                default_seen = true;
                None
            };
            loop {
                let end = self.is("break");
                ensure!(
                    !self.is("}")
                        && !self.is("case")
                        && !self.is("default")
                        && self.current() != &Token::End,
                    "{}",
                    self.error("switch branch requires an explicit final break")
                );
                self.statement(StatementScope::Braced)?;
                if end {
                    break;
                }
            }
            if let Some(guard) = guard {
                self.patch(guard);
            }
        }
        for index in self.loops.pop().unwrap().breaks {
            self.patch(index);
        }
        self.emit(Instruction::Drop);
        Ok(())
    }
    fn operator(&self) -> Option<(Op, u8)> {
        let Token::Symbol(s) = self.current() else {
            return None;
        };
        Some(match s.as_str() {
            "&&" => (Op::And, 1),
            "||" => (Op::Or, 1),
            "==" => (Op::Eq, 2),
            "!=" => (Op::Ne, 2),
            "<" => (Op::Lt, 2),
            "<=" => (Op::Le, 2),
            ">" => (Op::Gt, 2),
            ">=" => (Op::Ge, 2),
            "+" => (Op::Add, 3),
            "-" => (Op::Sub, 3),
            "*" => (Op::Mul, 4),
            "/" => (Op::Div, 4),
            "%" => (Op::Mod, 4),
            "^" => (Op::Pow, 5),
            _ => return None,
        })
    }
    fn lookup(&self, name: &str) -> Option<Slot> {
        self.locals
            .get(name)
            .map(|i| Slot::Local(*i))
            .or_else(|| self.globals.get(name).copied())
            .or_else(|| self.inherited.global(name))
    }
    fn constant(&self, name: &str) -> Option<(&Value, Option<usize>)> {
        self.constants
            .get(name)
            .map(|(value, position)| (value, Some(*position)))
            .or_else(|| self.inherited.define(name).map(|value| (value, None)))
    }
    fn variable(&mut self) -> Result<Slot> {
        let name = self.word()?;
        self.lookup(&name)
            .with_context(|| format!("{}", self.error("undeclared variable")))
    }
    fn slot_type(&self, slot: Slot) -> Type {
        match slot {
            Slot::Local(i) => self.types[i].kind,
            Slot::Global(_, kind) => kind,
        }
    }
    fn target(&mut self) -> Result<Slot> {
        let slot = self.variable()?;
        self.emit(Instruction::Select(slot));
        self.index()?;
        Ok(slot)
    }
    fn index(&mut self) -> Result<()> {
        if self.take("[") {
            self.expression(1)?;
            self.expect("]")?;
            self.emit(Instruction::Index);
        }
        Ok(())
    }
    fn elements(&mut self) -> Result<usize> {
        if !self.take("[") {
            return Ok(1);
        }
        let n = match self.current() {
            Token::Literal(Value::Int(n)) => *n,
            Token::Word(name) => {
                let (value, position) = self
                    .constant(name)
                    .with_context(|| format!("{}", self.error("unknown array dimension")))?;
                ensure!(
                    position.is_none_or(|position| position < self.p),
                    "{}",
                    self.error("array dimension define must precede declaration")
                );
                let Value::Int(n) = value else {
                    return Err(self.error("array dimension requires int define"));
                };
                *n
            }
            _ => return Err(self.error("array size requires integer literal or earlier define")),
        };
        ensure!(
            n >= 0 && n as usize <= MAX_ELEMENTS,
            "{}",
            self.error("script array size limit")
        );
        let n = n as usize;
        self.p += 1;
        self.expect("]")?;
        Ok(n)
    }
    fn initial_literal(&mut self, kind: Type, scalar: bool) -> Result<Option<Value>> {
        let negative = self.take("-");
        let value = match self.current() {
            Token::Literal(value) => value.clone(),
            Token::Boolean(value) if scalar => {
                // The original TRUE_CONST/FALSE_CONST branches ignore the
                // parsed sign and only assign to integer globals.
                let value = *value;
                self.p += 1;
                return Ok((kind == Type::Int).then_some(Value::Int(i32::from(value))));
            }
            Token::Word(name) if scalar && self.constant(name).is_some() => {
                // The original scalar initializer consumes UNKNOWN without
                // assigning a value, even when the name is an existing define.
                self.p += 1;
                return Ok(None);
            }
            _ => return Err(self.error("unsupported global initializer literal")),
        };
        if scalar && value.kind() != kind {
            // Native scalar initialization skips writes for other types.
            self.p += 1;
            return Ok(None);
        }
        ensure!(
            value.kind() == kind,
            "{}",
            self.error(&format!(
                "global initializer type mismatch (expected {kind:?}, got {:?})",
                value.kind()
            ))
        );
        ensure!(
            !negative || matches!(kind, Type::Int | Type::Float),
            "invalid initializer sign"
        );
        self.p += 1;
        Ok(Some(if negative { value.negate()? } else { value }))
    }
    fn initializer(&mut self, kind: Type, elements: usize) -> Result<Vec<Value>> {
        if !self.take("=") {
            return Ok(Vec::new());
        }
        if elements <= 1 {
            return Ok(self.initial_literal(kind, true)?.into_iter().collect());
        }
        self.expect("{")?;
        let mut values = Vec::new();
        if !self.take("}") {
            loop {
                ensure!(values.len() < elements, "too many array initializers");
                values.extend(self.initial_literal(kind, false)?);
                if self.take("}") {
                    break;
                }
                self.expect(",")?;
                if self.take("}") {
                    break;
                }
            }
        }
        Ok(values)
    }
    fn path(&mut self) -> Result<Vec<Access>> {
        let mut result = Vec::new();
        while self.take(".") {
            ensure!(result.len() < 128, "attribute expression depth limit");
            let access = if self.take("(") {
                let index = self.variable()?;
                self.expect(")")?;
                Access::Dynamic(index)
            } else {
                let (Token::Word(name) | Token::AttributeName(name)) = self.current() else {
                    return Err(self.error("expected attribute word or dynamic variable"));
                };
                let name = name.clone();
                self.p += 1;
                Access::Literal(name)
            };
            result.push(access);
        }
        Ok(result)
    }
    fn expression(&mut self, minimum: u8) -> Result<()> {
        self.enter()?;
        // Retained CompileExpression handles '&' before L0. It accepts one
        // variable/array element as the complete expression, never a raw pointer.
        if minimum == 1 && self.take("&") {
            let slot = self.variable()?;
            self.emit(Instruction::Select(slot));
            if self.take("[") {
                self.expression(3)?; // Original address indices start at L2.
                self.expect("]")?;
                self.emit(Instruction::Index);
            }
            self.emit(Instruction::Address);
            self.nesting -= 1;
            return Ok(());
        }
        let sign = if self.take("-") {
            Some(false)
        } else if self.take("!") {
            Some(true)
        } else {
            self.take("+");
            None
        };
        self.atom()?;
        if let Some(not) = sign {
            self.emit(if not {
                Instruction::Not
            } else {
                Instruction::Negate
            });
        }
        let mut power_seen = false;
        while let Some((op, precedence)) = self.operator() {
            if precedence < minimum {
                break;
            }
            ensure!(
                !(op == Op::Pow && power_seen),
                "{}",
                self.error("chained power requires explicit grouping")
            );
            power_seen = op == Op::Pow;
            self.p += 1;
            let short = if matches!(op, Op::And | Op::Or) {
                Some(self.emit(Instruction::Short(0, op == Op::Or)))
            } else {
                None
            };
            self.expression(precedence + 1)?;
            self.emit(Instruction::Binary(op));
            if let Some(index) = short {
                self.patch(index);
            }
        }
        self.nesting -= 1;
        Ok(())
    }
    fn atom(&mut self) -> Result<()> {
        if self.take("call") {
            return self.dynamic_call(true);
        }
        if let Token::Boolean(value) = self.current() {
            let value = Value::Int(i32::from(*value));
            self.p += 1;
            self.emit(Instruction::Push(value));
            return Ok(());
        }
        if let Token::Literal(value) = self.current() {
            let value = value.clone();
            self.p += 1;
            self.emit(Instruction::Push(value));
            return Ok(());
        }
        if self.take("(") {
            // The retained compiler's grouped expressions start at L1, not L0.
            self.expression(2)?;
            return self.expect(")");
        }
        let name = self.word()?;
        if self.take("(") {
            let arguments = self.call_arguments(name == "getentity")?;
            self.emit(Instruction::Call(name, arguments, CallSite::Expression));
        } else {
            let slot = if let Some(slot) = self.lookup(&name) {
                slot
            } else if let Some((value, _)) = self.constant(&name) {
                ensure!(
                    value.kind() != Type::Void,
                    "{}",
                    self.error("empty define has no scalar value")
                );
                self.emit(Instruction::Push(value.clone()));
                return Ok(());
            } else {
                return Err(self.error("undeclared variable or constant"));
            };
            self.emit(Instruction::Select(slot));
            self.index()?;
            if self.is(".") {
                let path = self.path()?;
                self.emit(Instruction::Attribute(path, AttributeMode::Read));
            } else {
                self.emit(Instruction::Load(false));
            }
        }
        Ok(())
    }
    fn dynamic_call(&mut self, in_expression: bool) -> Result<()> {
        let slot = self.variable()?;
        ensure!(
            !in_expression || self.slot_type(slot) == Type::String,
            "{}",
            self.error("dynamic call expressions require a declared string variable")
        );
        self.expect("(")?;
        let arguments = self.call_arguments(false)?;
        self.emit(Instruction::DynamicCall(slot, arguments));
        Ok(())
    }
    fn call_arguments(&mut self, first_by_reference: bool) -> Result<usize> {
        if self.take(")") {
            return Ok(0);
        }
        let mut count = 0;
        loop {
            let start = self.code.len();
            self.expression(1)?;
            if first_by_reference
                && count == 0
                && let Some(instruction) = self.code[start..].last_mut()
                && matches!(instruction.op, Instruction::Load(false))
            {
                instruction.op = Instruction::Address;
            }
            count += 1;
            ensure!(count <= self.limits.locals, "script argument limit");
            if self.take(")") {
                return Ok(count);
            }
            self.expect(",")?;
        }
    }
}
