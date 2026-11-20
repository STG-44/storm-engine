//! The original Windows CRT stream, shared by the native script RNG calls.
use anyhow::{Result, bail, ensure};
use std::time::{SystemTime, UNIX_EPOCH};
use storm_script::Value;

const RAND_MAX: u32 = 0x7fff;

pub(crate) struct Random {
    state: u32,
}

impl Random {
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    pub fn clock_seed() -> u32 {
        // COMPILER seeds the CRT with Unix time in milliseconds, narrowed to unsigned.
        SystemTime::now().duration_since(UNIX_EPOCH).map_or_else(
            |before| (before.duration().as_millis() as u32).wrapping_neg(),
            |duration| duration.as_millis() as u32,
        )
    }

    pub(crate) fn next(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(214013).wrapping_add(2531011);
        (self.state >> 16) & RAND_MAX
    }
    pub(crate) fn unit(&mut self) -> f32 {
        self.next() as f32 / RAND_MAX as f32
    }
    pub(crate) fn range(&mut self, min: f32, max: f32) -> f32 {
        // Native RRnd rounds the scale before multiplying by a CRT draw.
        min + self.next() as f32 * ((max - min) * (1.0 / RAND_MAX as f32))
    }

    pub fn script_call(&mut self, name: &str, arguments: &[Value]) -> Option<Result<Value>> {
        Some(match name {
            "rand" => self.rand(arguments),
            "frnd" => self.frnd(arguments),
            _ => return None,
        })
    }

    fn rand(&mut self, arguments: &[Value]) -> Result<Value> {
        let [limit] = arguments else {
            bail!("Rand expects one integer argument")
        };
        let Value::Int(limit) = limit.resolved()? else {
            bail!("Rand expects one integer argument")
        };
        // The inclusive modulus consumes a draw even for zero. Unsigned
        // magnitude also defines the endpoints where the C++ int overflowed.
        let value = (self.next() % (limit.unsigned_abs() + 1)) as i32;
        Ok(Value::Int(if limit < 0 { -value } else { value }))
    }

    fn frnd(&mut self, arguments: &[Value]) -> Result<Value> {
        ensure!(arguments.is_empty(), "frnd expects no arguments");
        Ok(Value::Float(self.unit()))
    }
}
