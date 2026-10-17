use crate::EntityId;
use anyhow::{Result, bail, ensure};

/// Owned, platform-independent subset of MESSAGE::MessageParam.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Byte(u8),
    Word(u16),
    Long(i32),
    Dword(u32),
    Float(f32),
    Double(f64),
    Entity(EntityId),
    Vector([f32; 3]),
    String(String),
}

impl Value {
    pub fn format(&self) -> char {
        match self {
            Self::Byte(_) => 'b',
            Self::Word(_) => 'w',
            Self::Long(_) => 'l',
            Self::Dword(_) => 'u',
            Self::Float(_) => 'f',
            Self::Double(_) => 'd',
            Self::Entity(_) => 'i',
            Self::Vector(_) => 'c',
            Self::String(_) => 's',
        }
    }
}

#[derive(Clone, Debug)]
pub struct Message {
    format: String,
    values: Vec<Value>,
    cursor: usize,
}

impl Message {
    pub fn new(format: &str, values: Vec<Value>) -> Result<Self> {
        ensure!(
            format.len() == values.len(),
            "message argument count mismatch"
        );
        for (index, (code, value)) in format.chars().zip(&values).enumerate() {
            ensure!(
                matches!(code, 'b' | 'w' | 'l' | 'u' | 'f' | 'd' | 'i' | 'c' | 's'),
                "unsupported message format {code:?} at {index}; pointer formats require a runtime handle registry"
            );
            ensure!(
                code == value.format(),
                "message argument {index}: expected {code}, got {}",
                value.format()
            );
        }
        Ok(Self {
            format: format.into(),
            values,
            cursor: 0,
        })
    }
    pub fn format(&self) -> &str {
        &self.format
    }
    pub fn values(&self) -> &[Value] {
        &self.values
    }
    pub fn move_to_start(&mut self) {
        self.cursor = 0;
    }
    pub fn current_type(&self) -> Option<char> {
        self.values.get(self.cursor).map(Value::format)
    }
    pub fn param_valid(&self) -> bool {
        self.cursor < self.values.len()
    }

    fn read(&mut self, code: char) -> Result<&Value> {
        let Some(value) = self.values.get(self.cursor) else {
            bail!("read past end of message");
        };
        ensure!(
            value.format() == code,
            "incorrect message data at {}: expected {code}, got {}",
            self.cursor,
            value.format()
        );
        self.cursor += 1;
        Ok(value)
    }
    pub fn byte(&mut self) -> Result<u8> {
        match self.read('b')? {
            Value::Byte(v) => Ok(*v),
            _ => unreachable!(),
        }
    }
    pub fn word(&mut self) -> Result<u16> {
        match self.read('w')? {
            Value::Word(v) => Ok(*v),
            _ => unreachable!(),
        }
    }
    pub fn long(&mut self) -> Result<i32> {
        match self.read('l')? {
            Value::Long(v) => Ok(*v),
            _ => unreachable!(),
        }
    }
    /// MESSAGE::Dword returns the original uint32 bit pattern as int32.
    pub fn dword(&mut self) -> Result<i32> {
        match self.read('u')? {
            Value::Dword(v) => Ok(*v as i32),
            _ => unreachable!(),
        }
    }
    pub fn float(&mut self) -> Result<f32> {
        match self.read('f')? {
            Value::Float(v) => Ok(*v),
            _ => unreachable!(),
        }
    }
    pub fn double(&mut self) -> Result<f64> {
        match self.read('d')? {
            Value::Double(v) => Ok(*v),
            _ => unreachable!(),
        }
    }
    pub fn entity(&mut self) -> Result<EntityId> {
        match self.read('i')? {
            Value::Entity(v) => Ok(*v),
            _ => unreachable!(),
        }
    }
    pub fn vector(&mut self) -> Result<[f32; 3]> {
        match self.read('c')? {
            Value::Vector(v) => Ok(*v),
            _ => unreachable!(),
        }
    }
    pub fn string(&mut self) -> Result<&str> {
        match self.read('s')? {
            Value::String(v) => Ok(v),
            _ => unreachable!(),
        }
    }
}
