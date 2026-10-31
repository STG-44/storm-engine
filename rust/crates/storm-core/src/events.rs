use crate::{Message, Value};
use anyhow::{Result, ensure};
use std::collections::BTreeMap;

type Handler = Box<dyn FnMut(&mut Message) -> Result<Option<Value>>>;

/// Native callbacks in registration order. The script compiler, delayed events,
/// handler deletion and nested script execution are separate future work.
#[derive(Default)]
pub struct EventDispatcher {
    handlers: BTreeMap<String, Vec<Handler>>,
}
impl EventDispatcher {
    pub fn register(
        &mut self,
        name: &str,
        handler: impl FnMut(&mut Message) -> Result<Option<Value>> + 'static,
    ) -> Result<()> {
        ensure!(
            !name.is_empty() && name.is_ascii() && !name.contains('\0'),
            "invalid engine event name"
        );
        self.handlers
            .entry(name.to_ascii_lowercase())
            .or_default()
            .push(Box::new(handler));
        Ok(())
    }
    /// COMPILER::ProcessEvent rewinds the message before every handler and
    /// retains the last non-void result. Event names compare without ASCII case.
    pub fn emit(&mut self, name: &str, message: &mut Message) -> Result<Option<Value>> {
        let mut result = None;
        if let Some(handlers) = self.handlers.get_mut(&name.to_ascii_lowercase()) {
            for handler in handlers {
                message.move_to_start();
                if let Some(value) = handler(message)? {
                    result = Some(value);
                }
            }
        }
        Ok(result)
    }
}
