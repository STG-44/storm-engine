use crate::NativeHost;
use anyhow::{Result, ensure};
use storm_core::{Message, Stage};
use storm_script::{Limits, Runtime, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    All,
    Execute,
    Realize,
}
pub struct ServiceFrame {
    pub delta_ms: u32,
    pub skip_frame: bool,
}
pub trait Service {
    fn section(&self) -> Section;
    fn run_start(&mut self, frame: &mut ServiceFrame) -> Result<()>;
    fn run_end(&mut self, frame: &mut ServiceFrame) -> Result<()>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Ready,
    Running,
    Faulted,
    Stopped,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameResult {
    pub continued: bool,
    pub skipped: bool,
    pub posted_events: usize,
}

/// Common script/entity frame owner. Window, controls, state loading and game
/// services attach here; no preview class or diagnostic shader is registered.
pub struct Engine {
    pub host: NativeHost,
    runtime: Runtime,
    services: Vec<Box<dyn Service>>,
    limits: Limits,
    phase: Phase,
    exit_requested: bool,
    exit_event_emitted: bool,
}
impl Engine {
    pub fn from_runtime(runtime: Runtime, host: NativeHost) -> Self {
        Self {
            runtime,
            host,
            services: Vec::new(),
            limits: Limits::default(),
            phase: Phase::Ready,
            exit_requested: false,
            exit_event_emitted: false,
        }
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn sources(&self) -> &[String] {
        self.runtime.sources()
    }
    pub fn register_service(&mut self, service: impl Service + 'static) -> Result<()> {
        ensure!(
            self.phase == Phase::Ready,
            "register services before first frame"
        );
        ensure!(self.services.len() < 256, "native service budget");
        self.services.push(Box::new(service));
        Ok(())
    }
    pub fn request_exit(&mut self) {
        self.exit_requested = true;
    }
    /// Platform events arrive between frames, after Main has initialized the
    /// program. Native stage-to-script reentrancy remains a separate boundary.
    pub fn emit_event(&mut self, name: &str, message: &mut Message) -> Result<Option<Value>> {
        ensure!(
            self.phase == Phase::Running,
            "platform events require a running engine"
        );
        let result = self
            .runtime
            .emit(name, message, &mut self.host, self.limits);
        if result.is_err() {
            self.phase = Phase::Faulted;
        }
        result
    }
    fn services(&mut self, section: Section, start: bool, frame: &mut ServiceFrame) -> Result<()> {
        // SERVICES_LIST adds at its head: newest services are visited first.
        for service in self.services.iter_mut().rev() {
            if service.section() == section {
                if start {
                    service.run_start(frame)?;
                } else {
                    service.run_end(frame)?;
                }
            }
        }
        Ok(())
    }
    fn event(&mut self, name: &str) -> Result<()> {
        self.runtime.emit(
            name,
            &mut Message::new("", vec![])?,
            &mut self.host,
            self.limits,
        )?;
        Ok(())
    }
    /// Retains the supported CoreImpl::Run order. Input/state loading, clock
    /// globals, Steam and dynamic service creation remain separate ports.
    pub fn frame(&mut self, delta_ms: u32) -> Result<FrameResult> {
        ensure!(
            matches!(self.phase, Phase::Ready | Phase::Running),
            "engine cannot run after fault or shutdown"
        );
        if self.exit_requested {
            return Ok(FrameResult {
                continued: false,
                skipped: false,
                posted_events: 0,
            });
        }
        let result = self.frame_inner(delta_ms);
        if result.is_err() {
            self.phase = Phase::Faulted;
        }
        result
    }
    fn frame_inner(&mut self, delta_ms: u32) -> Result<FrameResult> {
        self.host.delta_ms = delta_ms;
        if self.phase == Phase::Ready {
            self.runtime
                .call("Main", &[], &mut self.host, self.limits)?;
            self.phase = Phase::Running;
        }
        let posted_events = self
            .runtime
            .advance_frame(delta_ms, &mut self.host, self.limits)?;
        self.event("frame")?;
        let mut context = ServiceFrame {
            delta_ms,
            skip_frame: false,
        };
        self.services(Section::All, true, &mut context)?;
        if context.skip_frame {
            return Ok(FrameResult {
                continued: true,
                skipped: true,
                posted_events,
            });
        }
        self.services(Section::Execute, true, &mut context)?;
        self.host.entities.process_stage(Stage::Execute, delta_ms)?;
        self.services(Section::Execute, false, &mut context)?;
        self.services(Section::Realize, true, &mut context)?;
        self.host.entities.process_stage(Stage::Realize, delta_ms)?;
        self.services(Section::Realize, false, &mut context)?;
        self.host.entities.new_lifecycle()?;
        self.services(Section::All, false, &mut context)?;
        Ok(FrameResult {
            continued: true,
            skipped: false,
            posted_events,
        })
    }
    /// ExitApplication is the original application event. Entity cleanup here
    /// uses the checked deferred-destruction API, not native EraseAll semantics.
    pub fn shutdown(&mut self) -> Result<()> {
        if self.phase == Phase::Stopped {
            return Ok(());
        }
        let event = if self.phase != Phase::Ready
            && self.runtime.is_initialized()
            && !self.exit_event_emitted
        {
            self.exit_event_emitted = true;
            self.event("ExitApplication")
        } else {
            Ok(())
        };
        for id in self.host.entities.all_ids() {
            self.host.entities.erase(id);
        }
        let cleanup = self.host.entities.new_lifecycle();
        let cleanup = cleanup.and_then(|()| {
            ensure!(
                self.host.entities.all_ids().is_empty(),
                "entities were created during shutdown"
            );
            Ok(())
        });
        self.phase = if cleanup.is_ok() {
            Phase::Stopped
        } else {
            Phase::Faulted
        };
        event.and(cleanup)
    }
}
