use crate::{Type, Value, objects::AttributeReference, value::Op};
use anyhow::{Result, bail, ensure};

impl Value {
    pub fn resolved(&self) -> Result<Self> {
        if let Self::Reference(reference) = self {
            reference.value()
        } else {
            Ok(self.clone())
        }
    }
    pub fn attributes(&self) -> Result<AttributeReference> {
        match self.resolved()? {
            Self::Object(object) => Ok(object.root()),
            Self::AReference(Some(reference)) => Ok(reference),
            _ => bail!("uninitialized aref or value has no object attributes"),
        }
    }
    pub(crate) fn convert(self, kind: Type) -> Result<Self> {
        if self.kind() == kind {
            return Ok(self);
        }
        if let Self::AReference(Some(reference)) = self {
            return Self::String(reference.scalar_value()?).convert(kind);
        }
        Ok(match (self, kind) {
            (value, Type::Int) => Self::Int(value.integer()?),
            (Self::Int(n), Type::Ptr) => Self::Ptr(n as u64),
            (Self::Int(n), Type::Float) => Self::Float(n as f32),
            (Self::String(s), Type::Float) => Self::Float(parse_float(&s)),
            (Self::Int(n), Type::String) => Self::String(n.to_string()),
            (Self::Float(_), Type::String) => {
                bail!("gcvt float-to-string conversion is not ported yet")
            }
            _ => bail!("unsupported scalar/object conversion"),
        })
    }
    pub(crate) fn attribute_text(self) -> Result<String> {
        // MOVE's attribute branch uses %.7f, not DATA::Convert's gcvt(10).
        match self {
            Self::Reference(reference) => reference.convert(Type::String)?.path_text(),
            Self::Float(n) if n.is_finite() => Ok(format!("{:.7}", f64::from(n))),
            Self::Float(n) => Ok(storm_core::format_float(n)),
            value => match value.convert(Type::String)? {
                Self::String(s) => Ok(s),
                _ => unreachable!(),
            },
        }
    }
    pub(crate) fn path_text(self) -> Result<String> {
        let value = if let Self::Reference(reference) = self {
            reference.convert(Type::String)?
        } else {
            self.convert(Type::String)?
        };
        match value {
            Self::String(s) => Ok(s),
            _ => unreachable!(),
        }
    }
    pub(crate) fn condition_integer(&self) -> Result<i32> {
        if let Self::Reference(reference) = self {
            reference.convert(Type::Int)?.integer()
        } else {
            self.integer()
        }
    }
    pub(crate) fn bool_convert(&self) -> Result<Self> {
        let boolean = Self::Int(i32::from(self.resolved()?.truth()?));
        if let Self::Reference(reference) = self {
            reference.store(boolean)?;
            Ok(self.clone())
        } else {
            Ok(boolean)
        }
    }
    pub(crate) fn apply_binary(self, op: Op, rhs: Self, fuel: &mut u64) -> Result<Self> {
        ensure!(
            !(op == Op::Pow && matches!(rhs, Self::Reference(_))),
            "power requires an integer exponent, not ref"
        );
        if matches!(op, Op::And | Op::Or) {
            self.bool_convert()?;
            return Self::Int(i32::from(self.resolved()?.truth()?)).binary(
                op,
                rhs.resolved()?,
                fuel,
            );
        }
        if matches!(op, Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge)
            && let Self::Reference(reference) = &self
            && matches!(reference.value()?, Self::Ptr(_))
        {
            reference.convert(Type::Int)?;
        }
        let value = self.resolved()?.binary(op, rhs.resolved()?, fuel)?;
        if matches!(
            op,
            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Mod | Op::Pow
        ) && let Self::Reference(reference) = &self
        {
            reference.store(value)?;
            return Ok(self);
        }
        Ok(value)
    }
    pub(crate) fn apply_negate(self) -> Result<Self> {
        let value = self.resolved()?.negate()?;
        if let Self::Reference(reference) = &self {
            reference.store(value)?;
            Ok(self)
        } else {
            Ok(value)
        }
    }
}

// fast_float::from_chars general decimal prefix: no initial whitespace or '+'.
// Invalid prefixes leave zero. The retained 3.4.0 dependency writes infinities
// and signed zero on decimal overflow/underflow; the VM ignores its status.
fn parse_float(value: &str) -> f32 {
    let bytes = value.as_bytes();
    let start = usize::from(bytes.first() == Some(&b'-'));
    let special = value.get(start..).unwrap_or("");
    if special
        .get(..3)
        .is_some_and(|s| s.eq_ignore_ascii_case("inf"))
    {
        return if start == 1 {
            f32::NEG_INFINITY
        } else {
            f32::INFINITY
        };
    }
    if special
        .get(..3)
        .is_some_and(|s| s.eq_ignore_ascii_case("nan"))
    {
        return if start == 1 { -f32::NAN } else { f32::NAN };
    }
    let mut end = start;
    let mut digits = 0;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
        digits += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return 0.0;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut exponent = end + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        let first = exponent;
        while bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
            exponent += 1;
        }
        if exponent > first {
            end = exponent;
        }
    }
    value[..end].parse::<f32>().unwrap_or(0.0)
}
