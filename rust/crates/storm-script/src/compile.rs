use crate::storage::{Declaration, MAX_ELEMENTS, Slot};
use crate::{Access, Function, Instruction, Located, Program, Type, Value, builtins, value::Op};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, rc::Rc};

pub(crate) const MAX_TOKENS: usize = 16384;
const MAX_NESTING: usize = 64;
const MAX_LOCALS: usize = 128;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Token {
    Word(String),
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

pub(crate) fn lex(source: &str, file: Rc<str>) -> Result<Vec<Lexeme>> {
    ensure!(
        source.len() <= 262144 && !source.contains('\0'),
        "script source limit or NUL"
    );
    let mut result = Vec::new();
    let bytes = source.as_bytes();
    let (mut p, mut line, mut column) = (0, 1, 1);
    let mut previous_cr = false;
    while p < bytes.len() {
        let start = p;
        let location = (line, column);
        let token = if bytes[p].is_ascii_whitespace() {
            p += 1;
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
            Some(Token::Directive(source[name..p].to_ascii_lowercase()))
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
                    b"{}[]();.,+-*/%^=!<>&".contains(&bytes[p]),
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
            ensure!(result.len() < MAX_TOKENS, "script token limit");
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
    start: usize,
    breaks: Vec<usize>,
}

struct Compiler {
    tokens: Vec<Lexeme>,
    p: usize,
    code: Vec<Located>,
    locals: BTreeMap<String, usize>,
    types: Vec<Declaration>,
    globals: BTreeMap<String, usize>,
    declarations: Vec<Declaration>,
    constants: BTreeMap<String, (Value, usize)>,
    loops: Vec<Loop>,
    nesting: usize,
}

pub(crate) fn compile(sources: crate::source::Sources) -> Result<Program> {
    let mut c = Compiler {
        tokens: sources.tokens,
        p: 0,
        code: Vec::new(),
        locals: BTreeMap::new(),
        types: Vec::new(),
        globals: BTreeMap::new(),
        declarations: Vec::new(),
        constants: BTreeMap::new(),
        loops: Vec::new(),
        nesting: 0,
    };
    // The retained compiler registers globals before executing function bodies.
    // Collect top-level declarations before resolving any function references.
    let mut headers = Vec::new();
    let mut names = BTreeMap::new();
    let mut static_handlers = Vec::new();
    while c.current() != &Token::End {
        if c.is_static_handler() {
            static_handlers.push(c.static_handler()?);
            continue;
        }
        if let Token::Definition(name, value) = c.current().clone() {
            ensure!(c.constants.len() < 1024, "script constant limit");
            ensure!(
                !c.constants.contains_key(&name),
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
        let start = c.p;
        let native = c.take("native");
        let kind = c.kind()?;
        let name = c.word()?;
        if c.take("(") {
            ensure!(headers.len() < 128, "script function limit");
            ensure!(
                builtins::signature(&name).is_none(),
                "builtin function cannot be redeclared: {name}"
            );
            ensure!(
                names.insert(name.clone(), ()).is_none(),
                "duplicate function {name}"
            );
            headers.push(start);
            while !c.take(")") {
                ensure!(c.current() != &Token::End, "unterminated parameter list");
                c.p += 1;
            }
            if native {
                c.expect(";")?;
            } else {
                c.expect("{")?;
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
        } else {
            ensure!(!native && kind != Type::Void, "invalid global declaration");
            let mut name = name;
            loop {
                ensure!(c.globals.len() < 128, "script global limit");
                let elements = c.elements()?;
                let initial = c.initializer(kind, elements)?;
                ensure!(!c.globals.contains_key(&name), "duplicate global {name}");
                c.globals.insert(name, c.declarations.len());
                c.declarations.push(Declaration {
                    kind,
                    elements,
                    initial,
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
    for start in headers {
        c.p = start;
        let native = c.take("native");
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
        if native {
            c.expect(";")?;
        } else {
            c.expect("{")?;
            c.block()?;
            c.emit(Instruction::Return(false));
        }
        functions.insert(
            name,
            Function {
                parameters,
                returns,
                native,
                locals: std::mem::take(&mut c.types),
                code: std::mem::take(&mut c.code),
            },
        );
        c.locals.clear();
    }
    for (name, function) in &functions {
        for instruction in &function.code {
            if let Instruction::Call(callee, arguments) = &instruction.op {
                let expected = if let Some((count, _)) = builtins::signature(callee) {
                    count
                } else {
                    functions
                        .get(callee)
                        .with_context(|| {
                            format!(
                                "{name} {}:{}:{}: undeclared function {callee}",
                                instruction.file, instruction.line, instruction.column
                            )
                        })?
                        .parameters
                };
                ensure!(
                    if crate::events::signature(callee).is_some() {
                        crate::events::arguments_valid(callee, *arguments)
                    } else {
                        *arguments == expected
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
    let mut registered = crate::events::Handlers::default();
    for (event, function, origin) in static_handlers {
        let location = format!(
            "{}:{}:{}: #event_handler",
            origin.file, origin.line, origin.column
        );
        let event = match event {
            Token::Literal(Value::String(event)) => event,
            Token::Word(name) => match c.constants.get(&name) {
                Some((Value::String(event), _)) => event.clone(),
                _ => anyhow::bail!("{location}: event name requires a string define"),
            },
            _ => unreachable!(),
        };
        let handler = functions
            .get(&function)
            .with_context(|| format!("{location}: unknown event handler function {function}"))?;
        ensure!(
            handler.parameters == 0,
            "{location}: event handler must take no parameters"
        );
        registered
            .register_static(&event, &function)
            .with_context(|| location.clone())?;
        initial_handlers.push((event, function));
    }
    Ok(Program {
        functions,
        globals: c.declarations,
        sources: sources.files,
        static_handlers: initial_handlers,
    })
}

impl Compiler {
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
                "goto", "call", "makeref", "makearef", "extern"
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
            Instruction::Jump(t) | Instruction::JumpFalse(t) | Instruction::Short(t, _) => {
                *t = target
            }
            _ => unreachable!(),
        }
    }
    fn local(&mut self, name: String, kind: Type, elements: usize) -> Result<usize> {
        ensure!(self.types.len() < MAX_LOCALS, "script local limit");
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
            self.statement()?;
        }
    }
    fn statement(&mut self) -> Result<()> {
        self.enter()?;
        self.statement_inner()?;
        self.nesting -= 1;
        Ok(())
    }
    fn statement_inner(&mut self) -> Result<()> {
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
                self.emit(Instruction::Attribute(path, true));
                self.emit(Instruction::MakeARef);
            } else {
                ensure!(
                    self.slot_type(destination) == Type::Reference,
                    "makeref requires ref destination"
                );
                self.emit(Instruction::MakeRef);
            }
            self.expect(")")?;
            return self.expect(";");
        }
        if self.take("return") {
            let value = !self.is(";");
            if value {
                self.expression(1)?;
            }
            self.expect(";")?;
            self.emit(Instruction::Return(value));
            return Ok(());
        }
        if self.take("if") {
            self.expect("(")?;
            self.expression(1)?;
            self.expect(")")?;
            let conditional = self.emit(Instruction::JumpFalse(0));
            self.statement()?;
            if self.take("else") {
                let end = self.emit(Instruction::Jump(0));
                self.patch(conditional);
                self.statement()?;
                self.patch(end);
            } else {
                self.patch(conditional);
            }
            return Ok(());
        }
        if self.take("while") {
            let start = self.code.len();
            self.expect("(")?;
            self.expression(1)?;
            self.expect(")")?;
            let end = self.emit(Instruction::JumpFalse(0));
            self.loops.push(Loop {
                start,
                breaks: Vec::new(),
            });
            self.statement()?;
            self.emit(Instruction::Jump(start));
            self.patch(end);
            for index in self.loops.pop().unwrap().breaks {
                self.patch(index);
            }
            return Ok(());
        }
        if self.is("break") || self.is("continue") {
            let breaking = self.take("break");
            if !breaking {
                self.expect("continue")?;
            }
            ensure!(
                !self.loops.is_empty(),
                "{}",
                self.error("loop control outside loop")
            );
            self.expect(";")?;
            let target = self.loops.last().unwrap().start;
            let index = self.emit(Instruction::Jump(if breaking { 0 } else { target }));
            if breaking {
                self.loops.last_mut().unwrap().breaks.push(index);
            }
            return Ok(());
        }
        if ["int", "bool", "float", "string", "object", "ref", "aref"]
            .iter()
            .any(|s| self.is(s))
        {
            let kind = self.kind()?;
            loop {
                let name = self.word()?;
                let elements = self.elements()?;
                let index = self.local(name, kind, elements)?;
                if self.take("=") {
                    ensure!(elements <= 1, "local array initializer is not supported");
                    self.emit(Instruction::Select(Slot::Local(index)));
                    self.expression(1)?;
                    self.emit(Instruction::Store(true));
                }
                if !self.take(",") {
                    break;
                }
            }
            return self.expect(";");
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
                if !["=", "+=", "-=", "*=", "/=", "++", "--"]
                    .iter()
                    .any(|s| self.is(s))
                {
                    self.p = checkpoint.0;
                    self.code.truncate(checkpoint.1);
                    self.expression(1)?;
                    self.expect(";")?;
                    self.emit(Instruction::Drop);
                    return Ok(());
                }
                if let Some(path) = path {
                    self.emit(Instruction::Attribute(path, true));
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
                    }
                    self.emit(Instruction::Store(op.is_none()));
                }
                return self.expect(";");
            }
        }
        self.expression(1)?;
        self.expect(";")?;
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
            .or_else(|| self.globals.get(name).map(|i| Slot::Global(*i)))
    }
    fn variable(&mut self) -> Result<Slot> {
        let name = self.word()?;
        self.lookup(&name)
            .with_context(|| format!("{}", self.error("undeclared variable")))
    }
    fn slot_type(&self, slot: Slot) -> Type {
        match slot {
            Slot::Local(i) => self.types[i].kind,
            Slot::Global(i) => self.declarations[i].kind,
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
                    .constants
                    .get(name)
                    .with_context(|| format!("{}", self.error("unknown array dimension")))?;
                ensure!(
                    *position < self.p,
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
    fn initial_literal(&mut self, kind: Type, boolean: bool) -> Result<Value> {
        let negative = self.take("-");
        let value = match self.current() {
            Token::Literal(value) => value.clone(),
            Token::Boolean(value) if boolean && kind == Type::Int && !negative => {
                Value::Int(i32::from(*value))
            }
            _ => return Err(self.error("unsupported global initializer literal")),
        };
        ensure!(value.kind() == kind, "global initializer type mismatch");
        ensure!(
            !negative || matches!(kind, Type::Int | Type::Float),
            "invalid initializer sign"
        );
        self.p += 1;
        if negative { value.negate() } else { Ok(value) }
    }
    fn initializer(&mut self, kind: Type, elements: usize) -> Result<Vec<Value>> {
        if !self.take("=") {
            return Ok(Vec::new());
        }
        if elements <= 1 {
            return Ok(vec![self.initial_literal(kind, true)?]);
        }
        self.expect("{")?;
        let mut values = Vec::new();
        if !self.take("}") {
            loop {
                ensure!(values.len() < elements, "too many array initializers");
                values.push(self.initial_literal(kind, false)?);
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
                let Token::Word(name) = self.current() else {
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
            let mut arguments = 0;
            if !self.take(")") {
                loop {
                    self.expression(1)?;
                    arguments += 1;
                    ensure!(arguments <= MAX_LOCALS, "script argument limit");
                    if self.take(")") {
                        break;
                    }
                    self.expect(",")?;
                }
            }
            self.emit(Instruction::Call(name, arguments));
        } else {
            let slot = if let Some(slot) = self.lookup(&name) {
                slot
            } else if let Some((value, _)) = self.constants.get(&name) {
                self.emit(Instruction::Push(value.clone()));
                return Ok(());
            } else {
                return Err(self.error("undeclared variable or constant"));
            };
            self.emit(Instruction::Select(slot));
            self.index()?;
            if self.is(".") {
                let path = self.path()?;
                self.emit(Instruction::Attribute(path, false));
            } else {
                self.emit(Instruction::Load(false));
            }
        }
        Ok(())
    }
}
