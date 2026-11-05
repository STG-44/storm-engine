//! Retained TIMER scalar state with an explicit monotonic elapsed-time provider.
//! Core time scaling, script clock globals and profiling remain separate ports.
use anyhow::{Result, ensure};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub ring: bool,
    pub fixed: bool,
    pub fixed_value: u32,
    pub delta_ms: u32,
    pub high_precision_bits: u32,
    pub real_delta_ms: u32,
    pub fps: u32,
    pub fps_count: u32,
    pub fps_time_ms: u32,
}
#[derive(Clone, Debug)]
pub struct Timer {
    state: Snapshot,
}
impl Default for Timer {
    fn default() -> Self {
        Self {
            state: Snapshot {
                ring: false,
                fixed: false,
                fixed_value: 0,
                delta_ms: 20,
                high_precision_bits: 0.0f32.to_bits(),
                real_delta_ms: 20,
                fps: 0,
                fps_count: 0,
                fps_time_ms: 0,
            },
        }
    }
}
impl Timer {
    pub fn snapshot(&self) -> Snapshot {
        self.state
    }
    pub fn delta_ms(&self) -> u32 {
        if self.state.fixed {
            self.state.fixed_value
        } else {
            self.state.delta_ms
        }
    }
    pub fn set_delta(&mut self, delta: i32) {
        if delta <= 0 {
            self.state.fixed = false;
        } else {
            self.state.fixed_value = delta as u32;
            self.state.fixed = true;
        }
    }
    pub fn advance(&mut self, elapsed: Duration) -> Result<u32> {
        // chrono::duration<float,milli> with a nanosecond provider converts the
        // integer count to float before dividing, then TIMER casts to uint32.
        let millis = elapsed.as_nanos() as f32 / 1_000_000.0;
        ensure!(
            millis.is_finite() && millis < 4294967296.0,
            "elapsed clock interval exceeds native unsigned conversion"
        );
        let state = &mut self.state;
        state.delta_ms = (millis as u32).min(100);
        state.real_delta_ms = state.delta_ms;
        state.high_precision_bits = (state.delta_ms as f32).to_bits();
        state.fps_count = state.fps_count.wrapping_add(1);
        state.fps_time_ms = state.fps_time_ms.wrapping_add(state.delta_ms);
        if state.fps_time_ms >= 1000 {
            state.fps_time_ms -= 1000;
            state.fps = state.fps_count;
            state.fps_count = 0;
            state.ring = true;
        } else {
            state.ring = false;
        }
        // Native FPS accounting precedes the one-millisecond minimum.
        if state.delta_ms == 0 {
            state.delta_ms = 1;
            state.real_delta_ms = 1;
            state.high_precision_bits = 1.0f32.to_bits();
        }
        Ok(self.delta_ms())
    }
}
#[cfg(test)]
mod tests;
