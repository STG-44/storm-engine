use crate::{Access, Function, Instruction, Located, Program, Type, Value, builtins, value::Op};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

const MAX_TOKENS: usize = 16384;
const MAX_NESTING: usize = 64;
const MAX_LOCALS: usize = 128;

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Word(String),
    Literal(Value),
    Symbol(String),
    End,
}

#[derive(Clone)]
struct Lexeme {
    token: Token,
    line: usize,
    column: usize,
}

fn lex(source: &str) -> Result<Vec<Lexeme>> {
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
                "true" => Token::Literal(Value::Int(1)),
                "false" => Token::Literal(Value::Int(0)),
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
                    source[start..p]
                        .parse()
                        .with_context(|| format!("{line}:{column}: float literal"))?,
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
                    b"{}();.,+-*/%^=!<>".contains(&bytes[p]),
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
                line: location.0,
                column: location.1,
            });
        }
    }
    result.push(Lexeme {
        token: Token::End,
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
    types: Vec<Type>,
    loops: Vec<Loop>,
    nesting: usize,
}

pub(crate) fn compile(source: &str) -> Result<Program> {
    let mut c = Compiler {
        tokens: lex(source)?,
        p: 0,
        code: Vec::new(),
        locals: BTreeMap::new(),
        types: Vec::new(),
        loops: Vec::new(),
        nesting: 0,
    };
    let mut functions = BTreeMap::new();
    while c.current() != &Token::End {
        ensure!(functions.len() < 128, "script function limit");
        let native = c.take("native");
        let returns = c.kind()?;
        let name = c.word()?;
        ensure!(
            builtins::signature(&name).is_none(),
            "builtin function cannot be redeclared: {name}"
        );
        ensure!(!functions.contains_key(&name), "duplicate function {name}");
        c.expect("(")?;
        let mut parameters = 0;
        if !c.take(")") {
            loop {
                let kind = c.kind()?;
                ensure!(kind != Type::Void, "void parameter");
                let name = c.word()?;
                c.local(name, kind)?;
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
                                "{name} {}:{}: undeclared function {callee}",
                                instruction.line, instruction.column
                            )
                        })?
                        .parameters
                };
                ensure!(
                    *arguments == expected,
                    "{name} {}:{}: {callee} expects {} arguments, got {arguments}",
                    instruction.line,
                    instruction.column,
                    expected
                );
            }
        }
    }
    Ok(Program { functions })
}

impl Compiler {
    fn current(&self) -> &Token {
        &self.tokens[self.p].token
    }
    fn error(&self, message: &str) -> anyhow::Error {
        let t = &self.tokens[self.p];
        anyhow::anyhow!("{}:{}: {message}; found {:?}", t.line, t.column, t.token)
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
        let token = &self.tokens[self.p];
        let index = self.code.len();
        self.code.push(Located {
            op,
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
    fn local(&mut self, name: String, kind: Type) -> Result<usize> {
        ensure!(self.types.len() < MAX_LOCALS, "script local limit");
        ensure!(
            !self.locals.contains_key(&name),
            "{}",
            self.error("duplicate local")
        );
        let index = self.types.len();
        self.locals.insert(name, index);
        self.types.push(kind);
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
        while !self.take("}") {
            ensure!(
                self.current() != &Token::End,
                "{}",
                self.error("unterminated block")
            );
            self.statement()?;
        }
        Ok(())
    }
    fn statement(&mut self) -> Result<()> {
        self.enter()?;
        self.statement_inner()?;
        self.nesting -= 1;
        Ok(())
    }
    fn statement_inner(&mut self) -> Result<()> {
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
            let destination = self.variable()?;
            self.expect(",")?;
            let source = self.variable()?;
            if attribute {
                ensure!(
                    matches!(self.types[destination], Type::Reference | Type::AReference),
                    "makearef requires aref/ref destination"
                );
                let path = self.path()?;
                self.emit(Instruction::Attribute(source, path, true));
                self.emit(Instruction::MakeARef(destination));
            } else {
                ensure!(
                    self.types[destination] == Type::Reference,
                    "makeref requires ref destination"
                );
                self.emit(Instruction::MakeRef(destination, source));
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
                let index = self.local(name, kind)?;
                if self.take("=") {
                    self.expression(1)?;
                    self.emit(Instruction::Store(index));
                }
                if !self.take(",") {
                    break;
                }
            }
            return self.expect(";");
        }
        if let Token::Word(name) = self.current() {
            let name = name.clone();
            let next = &self.tokens[self.p + 1].token;
            if matches!(next, Token::Symbol(s) if s == ".") {
                let index = self.variable()?;
                let path = self.path()?;
                self.emit(Instruction::Attribute(index, path, true));
                self.expect("=")?;
                self.expression(1)?;
                self.emit(Instruction::WriteAttribute);
                return self.expect(";");
            }
            if matches!(next, Token::Symbol(s) if ["=", "+=", "-=", "*=", "/=", "++", "--"].contains(&s.as_str()))
            {
                let index = *self
                    .locals
                    .get(&name)
                    .with_context(|| format!("{}", self.error("undeclared local")))?;
                self.p += 1;
                if self.take("++") || self.take("--") {
                    let decrement =
                        matches!(&self.tokens[self.p-1].token, Token::Symbol(s) if s == "--");
                    self.emit(Instruction::Increment(index, decrement));
                } else {
                    let op = match self.current() {
                        Token::Symbol(s) => match s.as_str() {
                            "+=" => Some(Op::Add),
                            "-=" => Some(Op::Sub),
                            "*=" => Some(Op::Mul),
                            "/=" => Some(Op::Div),
                            _ => None,
                        },
                        _ => unreachable!(),
                    };
                    self.p += 1;
                    if op.is_some() {
                        self.emit(Instruction::Load(index));
                    }
                    self.expression(1)?;
                    if let Some(op) = op {
                        self.emit(Instruction::Binary(op));
                    }
                    self.emit(if op.is_some() {
                        Instruction::StoreDynamic(index)
                    } else {
                        Instruction::Store(index)
                    });
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
    fn variable(&mut self) -> Result<usize> {
        let name = self.word()?;
        self.locals
            .get(&name)
            .copied()
            .with_context(|| format!("{}", self.error("undeclared local")))
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
            let index = *self
                .locals
                .get(&name)
                .with_context(|| format!("{}", self.error("undeclared local")))?;
            if self.is(".") {
                let path = self.path()?;
                self.emit(Instruction::Attribute(index, path, false));
            } else {
                self.emit(Instruction::Load(index));
            }
        }
        Ok(())
    }
}
