use crate::host::NativeFunction;
use anyhow::{Context, Result, bail};
use std::time::Instant;
use storm_script::Value;

pub(crate) const MATH_LIBRARY: &str = "script_libriary_test";

pub(crate) fn math_functions() -> [(&'static str, NativeFunction); 6] {
    let origin = Instant::now();
    [
        ("bring2range", Box::new(|args| range(args, true))),
        ("bring2rangenocheck", Box::new(|args| range(args, false))),
        (
            "degree2radian",
            Box::new(|args| {
                let [degrees] = floats(args)?;
                Ok(Value::Float(std::f32::consts::TAU * degrees / 360.0))
            }),
        ),
        (
            "clampf",
            Box::new(|args| {
                let [value] = floats(args)?;
                Ok(Value::Float(value.clamp(0.0, 1.0)))
            }),
        ),
        (
            "rdtsc_b",
            // The original returns the low 32 bits of a performance counter.
            // Use ticks of a monotonic nanosecond clock on all targets.
            Box::new(move |args| {
                if !args.is_empty() {
                    bail!("rdtsc_b expects no arguments");
                }
                Ok(Value::Int(ticks(origin) as i32))
            }),
        ),
        (
            "rdtsc_e",
            Box::new(move |args| {
                let [Value::Int(start)] = args else {
                    bail!("rdtsc_e expects an integer counter");
                };
                Ok(Value::Int(ticks(origin).wrapping_sub(*start as u32) as i32))
            }),
        ),
    ]
}

fn ticks(origin: Instant) -> u32 {
    origin.elapsed().as_nanos() as u32
}

fn floats<const N: usize>(args: &[Value]) -> Result<[f32; N]> {
    let args: &[Value; N] = args
        .try_into()
        .ok()
        .context("wrong number of float arguments")?;
    let mut values = [0.0; N];
    for (value, arg) in values.iter_mut().zip(args) {
        let Value::Float(number) = arg else {
            bail!("native math argument requires float");
        };
        *value = *number;
    }
    Ok(values)
}

fn range(args: &[Value], checked: bool) -> Result<Value> {
    let [out_min, out_max, in_min, in_max, mut value] = floats(args)?;
    // math_inlines.h clamps in this order even for inverted bounds.
    if checked {
        if value < in_min {
            value = in_min;
        }
        if value > in_max {
            value = in_max;
        }
    }
    let delta = (value - in_min) / (in_max - in_min);
    Ok(Value::Float(out_min + delta * (out_max - out_min)))
}
