//! Explicit source loading. Includes append complete files, not inline text.
use crate::CompileLimits;
use crate::compile::{Lexeme, Token, lex};
use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::{BTreeSet, VecDeque},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    rc::Rc,
};

pub(crate) const MAX_FILE: usize = 262144;
pub(crate) const MAX_TOTAL: usize = 1048576;
pub(crate) const MAX_FILES: usize = 64;

/// The embedding application explicitly supplies source text for normalized,
/// root-relative ASCII paths. Script execution itself cannot read files.
pub trait SourceLoader {
    fn load(&mut self, path: &str) -> Result<String>;
}

/// A named native script-library request, never a filesystem or FFI path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryRequest {
    pub name: String,
    pub file: String,
    pub line: usize,
    pub column: usize,
}

/// UTF-8 source files below one explicit directory. Lookup folds ASCII case on
/// every platform and accepts Windows separators. Ambiguous names fail.
pub struct DirectorySource {
    root: PathBuf,
    file_bytes: usize,
}
impl DirectorySource {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        Self::with_limits(root, CompileLimits::default())
    }
    pub fn with_limits(root: impl AsRef<Path>, limits: CompileLimits) -> Result<Self> {
        limits.validate()?;
        let root = root.as_ref().canonicalize().context("open script root")?;
        ensure!(root.is_dir(), "script root is not a directory");
        Ok(Self {
            root,
            file_bytes: limits.file_bytes,
        })
    }
}
impl SourceLoader for DirectorySource {
    fn load(&mut self, path: &str) -> Result<String> {
        let path = normalized(path)?;
        let mut current = self.root.clone();
        for part in path.split('/') {
            let mut found = None;
            for (count, entry) in std::fs::read_dir(&current)
                .context("read script directory")?
                .enumerate()
            {
                ensure!(count < 65536, "script directory entry limit");
                let entry = entry?;
                if entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(part))
                {
                    ensure!(found.is_none(), "ambiguous script path: {path}");
                    found = Some(entry.path());
                }
            }
            current = found
                .with_context(|| format!("script file not found: {path}"))?
                .canonicalize()?;
            ensure!(
                current.starts_with(&self.root),
                "script path escapes root: {path}"
            );
        }
        ensure!(
            std::fs::metadata(&current)?.is_file(),
            "script source is not a regular file: {path}"
        );
        let file = File::open(&current).with_context(|| format!("open script {path}"))?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && metadata.len() <= self.file_bytes as u64,
            "invalid script file or source limit: {path}"
        );
        let mut text = String::new();
        file.take(self.file_bytes as u64 + 1)
            .read_to_string(&mut text)
            .with_context(|| format!("read UTF-8 script {path}"))?;
        ensure!(text.len() <= self.file_bytes, "script source limit: {path}");
        Ok(text)
    }
}

pub(crate) fn normalized(path: &str) -> Result<String> {
    ensure!(
        !path.is_empty()
            && path.len() <= 256
            && path.is_ascii()
            && !path.bytes().any(|b| b.is_ascii_control() || b == b':'),
        "invalid script path"
    );
    let path = path.replace('\\', "/");
    ensure!(!path.starts_with('/'), "absolute script path");
    let mut parts = Vec::new();
    for part in path.split('/') {
        ensure!(
            !part.is_empty() && part != "..",
            "invalid script path component"
        );
        if part != "." {
            parts.push(part.to_ascii_lowercase());
        }
    }
    ensure!(!parts.is_empty(), "empty script path");
    Ok(parts.join("/"))
}

pub(crate) struct Sources {
    pub tokens: Vec<Lexeme>,
    pub files: Vec<String>,
    pub libraries: Vec<LibraryRequest>,
    pub limits: CompileLimits,
    pub entry: Option<String>,
    pub bytes: usize,
    pub raw_tokens: usize,
}
pub(crate) fn memory(text: &str) -> Result<Sources> {
    memory_with_limits(text, CompileLimits::default())
}
pub(crate) fn memory_with_limits(text: &str, limits: CompileLimits) -> Result<Sources> {
    limits.validate()?;
    let mut builder = Builder {
        limits,
        ..Builder::default()
    };
    builder.add("<source>", text)?;
    ensure!(
        builder.pending.is_empty(),
        "#include requires an explicit SourceLoader"
    );
    Ok(builder.finish(None))
}
pub(crate) fn load(entry: &str, loader: &mut impl SourceLoader) -> Result<Sources> {
    load_with_limits(entry, loader, CompileLimits::default())
}
pub(crate) fn load_with_limits(
    entry: &str,
    loader: &mut impl SourceLoader,
    limits: CompileLimits,
) -> Result<Sources> {
    limits.validate()?;
    let mut builder = Builder {
        limits,
        ..Builder::default()
    };
    builder.queue(entry, None)?;
    while let Some((path, origin)) = builder.pending.pop_front() {
        let text = loader
            .load(&path)
            .with_context(|| origin.unwrap_or_else(|| format!("load script {path}")))?;
        builder.add(&path, &text)?;
    }
    Ok(builder.finish(Some(entry.to_owned())))
}

#[derive(Default)]
struct Builder {
    limits: CompileLimits,
    seen: BTreeSet<String>,
    pending: VecDeque<(String, Option<String>)>,
    files: Vec<String>,
    tokens: Vec<Lexeme>,
    end: Option<Lexeme>,
    bytes: usize,
    raw_tokens: usize,
    libraries: Vec<LibraryRequest>,
}
impl Builder {
    fn queue(&mut self, path: &str, origin: Option<String>) -> Result<()> {
        let path = normalized(path)?;
        if !self.seen.contains(&path) {
            ensure!(
                self.seen.len() < self.limits.files,
                "script file count limit"
            );
            self.seen.insert(path.clone());
            self.pending.push_back((path, origin));
        }
        Ok(())
    }
    fn add(&mut self, path: &str, text: &str) -> Result<()> {
        ensure!(
            text.len() <= self.limits.file_bytes
                && self.bytes + text.len() <= self.limits.total_bytes,
            "script source byte limit: {path}"
        );
        self.bytes += text.len();
        let mut tokens =
            lex(text, Rc::from(path), self.limits).with_context(|| format!("source {path}"))?;
        self.end = tokens.pop();
        self.raw_tokens += tokens.len();
        ensure!(
            self.raw_tokens <= self.limits.tokens,
            "script token limit across files"
        );
        self.files.push(path.into());
        let mut input = tokens.into_iter().peekable();
        while let Some(mut item) = input.next() {
            if let Token::Directive(command) = &item.token {
                let context = format!("{}:{}:{}: #{command}", item.file, item.line, item.column);
                match command.as_str() {
                    "libriary" => {
                        let argument = input
                            .next()
                            .with_context(|| format!("{context}: missing name"))?;
                        let Token::Literal(crate::Value::String(name)) = argument.token else {
                            bail!("{context}: expected quoted native library name");
                        };
                        ensure!(
                            !name.is_empty()
                                && name.len() <= 128
                                && name.is_ascii()
                                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
                            "{context}: invalid native library name"
                        );
                        if !self
                            .libraries
                            .iter()
                            .any(|library| library.name.eq_ignore_ascii_case(&name))
                        {
                            ensure!(
                                self.libraries.len() < 64,
                                "{context}: native library request limit"
                            );
                            self.libraries.push(LibraryRequest {
                                name,
                                file: item.file.to_string(),
                                line: item.line,
                                column: item.column,
                            });
                        }
                        continue;
                    }
                    "include" => {
                        let argument = input
                            .next()
                            .with_context(|| format!("{context}: missing path"))?;
                        let Token::Literal(crate::Value::String(path)) = argument.token else {
                            bail!("{context}: expected quoted script path");
                        };
                        self.queue(&path, Some(context.clone()))
                            .with_context(|| context.clone())?;
                        continue;
                    }
                    "define" => {
                        let name = input
                            .next()
                            .with_context(|| format!("{context}: missing name"))?;
                        let definition_line = name.line;
                        let name = match name.token {
                            Token::Word(name) => name,
                            Token::Boolean(true) => "true".to_owned(),
                            Token::Boolean(false) => "false".to_owned(),
                            _ => bail!("{context}: expected constant name"),
                        };
                        if input.peek().is_none_or(|next| next.line > definition_line) {
                            // Native empty header guards reserve the name with
                            // a non-scalar token type (usually DEBUG_LINEFEED).
                            item.token = Token::Definition(name, crate::Value::Void);
                            self.tokens.push(item);
                            continue;
                        }
                        let mut value = input
                            .next()
                            .with_context(|| format!("{context}: missing value"))?;
                        let negative = matches!(&value.token, Token::Symbol(s) if s == "-");
                        if negative {
                            value = input
                                .next()
                                .with_context(|| format!("{context}: missing negative literal"))?;
                        }
                        let Token::Literal(value) = value.token else {
                            bail!("{context}: expected int, float or string literal");
                        };
                        ensure!(
                            !negative
                                || matches!(value, crate::Value::Int(_) | crate::Value::Float(_)),
                            "{context}: invalid literal sign"
                        );
                        let value = if negative { value.negate()? } else { value };
                        item.token = Token::Definition(name, value);
                        if input
                            .peek()
                            .is_some_and(|t| matches!(&t.token, Token::Symbol(s) if s == ";"))
                        {
                            input.next();
                        }
                    }
                    "event_handler" => {}
                    _ => bail!("{context}: unsupported directive"),
                }
            }
            self.tokens.push(item);
        }
        Ok(())
    }
    fn finish(mut self, entry: Option<String>) -> Sources {
        self.tokens.push(self.end.expect("a source was lexed"));
        Sources {
            tokens: self.tokens,
            files: self.files,
            libraries: self.libraries,
            limits: self.limits,
            entry,
            bytes: self.bytes,
            raw_tokens: self.raw_tokens,
        }
    }
}
