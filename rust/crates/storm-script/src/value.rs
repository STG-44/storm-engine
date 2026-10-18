use anyhow::{Result, bail, ensure};

pub(crate) const MAX_STRING: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Int,
    Float,
    String,
    Void,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i32),
    Float(f32),
    String(String),
    Void,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl Type {
    pub(crate) fn initial(self) -> Value {
        match self {
            Self::Int => Value::Int(0),
            Self::Float => Value::Float(0.0),
            Self::String => Value::String(String::new()),
            Self::Void => Value::Void,
        }
    }
}

impl Value {
    pub fn kind(&self) -> Type {
        match self {
            Self::Int(_) => Type::Int,
            Self::Float(_) => Type::Float,
            Self::String(_) => Type::String,
            Self::Void => Type::Void,
        }
    }

    pub fn integer(&self) -> Result<i32> {
        match self {
            Self::Int(v) => Ok(*v),
            Self::Float(v) => {
                let truncated = v.trunc() as f64;
                ensure!(
                    truncated >= i32::MIN as f64 && truncated <= i32::MAX as f64,
                    "float outside portable integer conversion range"
                );
                Ok(*v as i32)
            }
            // DATA::Convert uses from_chars: no leading whitespace or '+'.
            Self::String(s) => {
                let start = usize::from(s.starts_with('-'));
                let end = start
                    + s.as_bytes()[start..]
                        .iter()
                        .take_while(|c| c.is_ascii_digit())
                        .count();
                Ok(s[..end].parse().unwrap_or(0))
            }
            Self::Void => bail!("void value in integer conversion"),
        }
    }

    pub fn text(&self) -> Result<&str> {
        match self {
            Self::String(s) => Ok(s),
            _ => bail!("expected string, received {:?}", self.kind()),
        }
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if let Self::String(s) = self {
            ensure!(
                s.len() <= MAX_STRING && !s.contains('\0'),
                "script string limit or NUL"
            );
        }
        Ok(())
    }

    pub(crate) fn truth(&self) -> Result<bool> {
        Ok(match self {
            Self::Int(v) => *v != 0,
            Self::Float(v) => *v != 0.0,
            Self::String(s) => !s.is_empty(),
            Self::Void => bail!("void condition"),
        })
    }

    pub(crate) fn negate(self) -> Result<Self> {
        Ok(match self {
            Self::Int(v) => Self::Int(v.wrapping_neg()),
            Self::Float(v) => Self::Float(-v),
            // DATA::Inverse leaves non-numeric scalars unchanged.
            Self::String(_) => self,
            Self::Void => bail!("void unary operand"),
        })
    }

    pub(crate) fn not(self) -> Result<Self> {
        // DATA::Neg's string conversion overwrites its intended emptiness
        // result. Preserve that bug; floats truncate before testing.
        if matches!(self, Self::String(_)) {
            return Ok(Self::Int(self.integer()?));
        }
        Ok(Self::Int(i32::from(self.integer()? == 0)))
    }

    pub(crate) fn binary(mut self, op: Op, rhs: Self, fuel: &mut u64) -> Result<Self> {
        if op == Op::Pow {
            let Self::Int(exponent) = rhs else {
                bail!("power requires an integer exponent")
            };
            ensure!(
                matches!(self, Self::Int(_) | Self::Float(_)),
                "numeric power base required"
            );
            let base = self.clone();
            if exponent == 0 {
                return Ok(if matches!(self, Self::Int(_)) {
                    Self::Int(1)
                } else {
                    Self::Float(1.0)
                });
            }
            // Preserve DATA::Power's negative-exponent loop, including -1.
            for _ in 1..exponent.unsigned_abs() {
                crate::spend(fuel)?;
                self = if exponent > 0 {
                    self.binary(Op::Mul, base.clone(), fuel)?
                } else {
                    let one = if matches!(base, Self::Int(_)) {
                        Self::Int(1)
                    } else {
                        Self::Float(1.0)
                    };
                    one.binary(Op::Div, base.clone(), fuel)?
                };
            }
            return Ok(self);
        }
        if op == Op::Add && (matches!(self, Self::String(_)) || matches!(rhs, Self::String(_))) {
            let stringify = |v: Self| -> Result<String> {
                match v {
                    Self::String(s) => Ok(s),
                    Self::Int(n) => Ok(n.to_string()),
                    _ => bail!("float/string concatenation is not ported yet"),
                }
            };
            let mut a = stringify(self)?;
            let b = stringify(rhs)?;
            ensure!(a.len() + b.len() <= MAX_STRING, "script string limit");
            a.push_str(&b);
            return Ok(Self::String(a));
        }
        if matches!(op, Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge) {
            if let (Self::String(a), Self::String(b)) = (&self, &rhs) {
                ensure!(
                    a.is_ascii() && b.is_ascii(),
                    "non-ASCII string comparison is not ported yet"
                );
                let a = a.to_ascii_uppercase();
                let b = b.to_ascii_uppercase();
                return Ok(Self::Int(i32::from(match op {
                    Op::Eq => a == b,
                    Op::Ne => a != b,
                    Op::Lt => a < b,
                    Op::Le => a <= b,
                    Op::Gt => a > b,
                    Op::Ge => a >= b,
                    _ => unreachable!(),
                })));
            }
            if matches!(self, Self::String(_)) && matches!(rhs, Self::Int(_)) {
                self = Self::Int(self.integer()?);
            }
        }
        if let (Self::Int(a), Self::Int(b)) = (&self, &rhs) {
            return Ok(Self::Int(match op {
                Op::Add => a.wrapping_add(*b),
                Op::Sub => a.wrapping_sub(*b),
                Op::Mul => a.wrapping_mul(*b),
                Op::Div | Op::Mod => {
                    ensure!(*b != 0, "divide by zero");
                    ensure!(!(*a == i32::MIN && *b == -1), "integer division overflow");
                    if op == Op::Div { a / b } else { a % b }
                }
                Op::Eq => i32::from(a == b),
                Op::Ne => i32::from(a != b),
                Op::Lt => i32::from(a < b),
                Op::Le => i32::from(a <= b),
                Op::Gt => i32::from(a > b),
                Op::Ge => i32::from(a >= b),
                Op::And => i32::from(*a != 0 && *b != 0),
                Op::Or => i32::from(*a != 0 || *b != 0),
                Op::Pow => unreachable!(),
            }));
        }
        let number = |v: &Self| -> Result<f32> {
            match v {
                Self::Int(n) => Ok(*n as f32),
                Self::Float(n) => Ok(*n),
                _ => bail!("unsupported operand {:?}", v.kind()),
            }
        };
        let a = number(&self)?;
        let b = number(&rhs)?;
        let value = match op {
            Op::Add => a + b,
            Op::Sub => a - b,
            Op::Mul => a * b,
            Op::Div => {
                ensure!(b != 0.0, "divide by zero");
                a / b
            }
            Op::Mod => {
                ensure!(b != 0.0, "divide by zero");
                if let Self::Int(n) = self {
                    let divisor = Self::Float(b.floor()).integer()?;
                    return Self::Int(n).binary(Op::Mod, Self::Int(divisor), fuel);
                }
                (a / b).floor()
            }
            Op::Eq => return Ok(Self::Int(i32::from(a == b))),
            Op::Ne => return Ok(Self::Int(i32::from(a != b))),
            Op::Lt => return Ok(Self::Int(i32::from(a < b))),
            Op::Le => return Ok(Self::Int(i32::from(a <= b))),
            Op::Gt => return Ok(Self::Int(i32::from(a > b))),
            Op::Ge => return Ok(Self::Int(i32::from(a >= b))),
            Op::And => return Ok(Self::Int(i32::from(a != 0.0 && b != 0.0))),
            Op::Or => return Ok(Self::Int(i32::from(a != 0.0 || b != 0.0))),
            Op::Pow => unreachable!(),
        };
        Ok(Self::Float(value))
    }
}
