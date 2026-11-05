//! The retained two-pass frame scheduler, including cancellation and cleanup quirks.
use super::{check_message, event_name};
use anyhow::{Result, ensure};
use storm_core::{Message, Value};

const MAX_POSTS: usize = 256;
const MAX_PAYLOAD_BYTES: usize = 1_048_576;

pub(super) struct Queued {
    pub name: String,
    pub elapsed: u32,
    pub period: u32,
    pub processed: bool,
    pub invalid: bool,
    pub message: Option<Message>,
    bytes: usize,
}
impl Queued {
    fn process_time(&mut self, delta: u32) -> bool {
        self.elapsed = self.elapsed.wrapping_add(delta);
        self.processed = true;
        self.period == 0 || self.elapsed >= self.period
    }
}
#[derive(Default)]
pub(super) struct Queue {
    pub entries: Vec<Queued>,
    bytes: usize,
    pub total: u64,
}
impl Queue {
    pub fn post(&mut self, name: &str, delay: i32, message: Option<Message>) -> Result<()> {
        event_name(name)?;
        if let Some(message) = &message {
            check_message(message)?;
        }
        let bytes = name.len()
            + message.as_ref().map_or(0, |m| {
                m.format().len()
                    + m.values()
                        .iter()
                        .map(|v| {
                            std::mem::size_of::<Value>()
                                + if let Value::String(s) = v { s.len() } else { 0 }
                        })
                        .sum::<usize>()
            });
        ensure!(self.entries.len() < MAX_POSTS, "posted event count budget");
        ensure!(
            self.bytes + bytes <= MAX_PAYLOAD_BYTES,
            "posted event payload budget"
        );
        self.entries.push(Queued {
            name: name.into(),
            elapsed: 0,
            period: delay as u32,
            processed: false,
            invalid: false,
            message,
            bytes,
        });
        self.bytes += bytes;
        self.total = self.total.saturating_add(1);
        Ok(())
    }
    pub fn ready(&mut self, index: usize, delta: u32) -> Option<(String, Option<Message>)> {
        let event = &mut self.entries[index];
        if event.invalid {
            return None;
        }
        let first = !event.processed;
        let due = event.process_time(delta);
        if first || !due {
            return None;
        }
        event.invalid = true;
        // The original retains the owned message until end-of-frame cleanup.
        Some((event.name.clone(), event.message.clone()))
    }
    fn remove(&mut self, index: usize) {
        self.bytes -= self.entries.remove(index).bytes;
    }
    pub fn cancel(&mut self, name: &str) {
        let mut index = 0;
        while index < self.entries.len() {
            // COMPILER::DelEventHandler calls ProcessTime(0) on EVERY entry,
            // including unrelated names. Due (and zero-period) entries survive.
            if !self.entries[index].process_time(0)
                && self.entries[index].name.eq_ignore_ascii_case(name)
            {
                self.remove(index);
            } else {
                index += 1;
            }
        }
    }
    pub fn invalidate_all(&mut self) {
        for event in &mut self.entries {
            event.invalid = true;
        }
    }
    pub fn cleanup(&mut self) {
        let mut index = 0;
        while index < self.entries.len() {
            if self.entries[index].invalid {
                self.remove(index);
                // POSTEVENTS_LIST::RemoveInvalidated resets n=0, then ++n.
                index = 1;
            } else {
                index += 1;
            }
        }
    }
    pub fn pending(&self) -> usize {
        self.entries.iter().filter(|e| !e.invalid).count()
    }
}
