use crate::{ControlInput, NativeHost};
use anyhow::{Result, ensure};
use storm_core::{LayerType, Message, Stage, Value as MessageValue};
use storm_render::Frame;
use storm_script::{Limits, Runtime, Value};

type ScreenshotSource<'a> = dyn FnMut(&NativeHost) -> Result<Option<Frame>> + 'a;

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
    startup_instructions: u64,
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
            startup_instructions: Limits::default().instructions,
            phase: Phase::Ready,
            exit_requested: false,
            exit_event_emitted: false,
        }
    }
    pub(crate) fn with_startup_instructions(mut self, instructions: u64) -> Self {
        self.startup_instructions = instructions;
        self
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
    pub fn control_input(&mut self, input: ControlInput) {
        self.host.controls.input(input);
    }
    pub fn mouse_wheel(&mut self, steps: i32) -> Result<()> {
        self.control_input(ControlInput::MouseWheel(steps));
        if self.phase == Phase::Running && steps != 0 {
            let mut message = Message::new("l", vec![MessageValue::Long(i32::from(steps as i16))])?;
            self.emit_event("evMouseWeel", &mut message)?;
        }
        Ok(())
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
    fn process_controls(&mut self, delta_ms: u32) -> Result<()> {
        self.host.controls.advance(delta_ms);
        let mut id = 0;
        while id < self.host.controls.len() {
            if let Some((event, name)) = self.host.controls.edge(id) {
                let mut message = Message::new("s", vec![MessageValue::String(name.to_owned())])?;
                self.runtime
                    .emit(event, &mut message, &mut self.host, self.limits)?;
            }
            id += 1;
        }
        Ok(())
    }
    fn execute_entities(&mut self, delta_ms: u32) -> Result<()> {
        for id in self.host.entities.layer_ids(LayerType::Execute) {
            if let Some(ships) = self.host.ships.clone()
                && self.host.entities.is_valid(id)
                && ships.owns(id)
            {
                let world = storm_script::Host::world(&self.host);
                if let Some(mut message) =
                    world.read(|store| ships.update_event(id, delta_ms, store))??
                {
                    self.runtime.emit(
                        "Shp_UpdParams",
                        &mut message,
                        &mut self.host,
                        self.limits,
                    )?;
                }
                world
                    .update(|store| ships.execute(id, delta_ms, &mut self.host.entities, store))?;
            }
            if let Some(flags) = self.host.flags.clone() {
                for request in flags.take_requests(id) {
                    let reply = if let Some(mut message) = request.message(&self.host.entities)? {
                        self.runtime.emit(
                            "GetRiggingData",
                            &mut message,
                            &mut self.host,
                            self.limits,
                        )?
                    } else {
                        None
                    };
                    flags.apply_reply(request, reply)?;
                }
            }
            if self.host.controls.activated("BIFastCommand") {
                let action = self
                    .host
                    .log_and_actions_state(id)
                    .filter(|state| state.visible)
                    .and_then(|state| state.active_name.clone());
                if let Some(action) = action {
                    let mut message = Message::new("s", vec![MessageValue::String(action)])?;
                    self.runtime.emit(
                        "BI_FastCommand",
                        &mut message,
                        &mut self.host,
                        self.limits,
                    )?;
                }
            }
            self.host
                .entities
                .process_entity_stage(id, Stage::Execute, delta_ms)?;
            if let Some(scenes) = &self.host.back_scenes
                && let Some(notices) = scenes.execute(id, &self.host.controls)?
            {
                for command in notices.commands {
                    self.runtime.post_native_event(
                        "backgroundcommand",
                        1,
                        Message::new("s", vec![MessageValue::String(command)])?,
                    )?;
                }
                if let Some((position, angles)) = notices.camera {
                    let mut message = Message::new(
                        "ffffff",
                        position
                            .into_iter()
                            .chain(angles)
                            .map(MessageValue::Float)
                            .collect(),
                    )?;
                    // SetCamera with an explicit FOV calls the two-argument form first.
                    for _ in 0..2 {
                        message.move_to_start();
                        self.runtime.emit(
                            "CameraPosAng",
                            &mut message,
                            &mut self.host,
                            self.limits,
                        )?;
                    }
                }
                if notices.selection_changed {
                    let mut message = Message::new("l", vec![MessageValue::Long(2)])?;
                    self.runtime
                        .emit("ISound", &mut message, &mut self.host, self.limits)?;
                }
            }
            if let Some(hour) = self
                .host
                .weather
                .as_ref()
                .and_then(|weather| weather.take_time_update(id))
            {
                let mut message = Message::new("f", vec![MessageValue::Float(hour)])?;
                self.runtime.emit(
                    "WeatherTimeUpdate",
                    &mut message,
                    &mut self.host,
                    self.limits,
                )?;
            }
            if let Some(notifications) = self
                .host
                .interfaces
                .as_ref()
                .and_then(|interfaces| interfaces.take_fader_notifications(id))
            {
                if notifications.erase {
                    self.host.entities.erase(id);
                }
                for event in notifications.events() {
                    let message = Message::new(
                        "li",
                        vec![
                            MessageValue::Long(i32::from(notifications.fade_in)),
                            MessageValue::Entity(id),
                        ],
                    )?;
                    self.runtime.post_native_event(event, 0, message)?;
                }
            }
            if self.host.entities.is_valid(id)
                && let Some(event) = self
                    .host
                    .interfaces
                    .as_ref()
                    .and_then(|interfaces| interfaces.take_video_event(id))
            {
                self.event(event)?;
            }
        }
        Ok(())
    }
    fn realize_entities(
        &mut self,
        delta_ms: u32,
        screenshot: &mut ScreenshotSource<'_>,
    ) -> Result<()> {
        let astronomy = self.host.entities.first_id("Astronomy");
        let mut astronomy_realized = false;
        for id in self.host.entities.layer_ids(LayerType::Realize) {
            if Some(id) != astronomy || !astronomy_realized {
                self.host
                    .entities
                    .process_entity_stage(id, Stage::Realize, delta_ms)?;
                astronomy_realized |= Some(id) == astronomy;
            }
            if self.host.entities.is_valid(id)
                && let Some(interfaces) = &self.host.interfaces
                && interfaces.dialog_active(id)
            {
                storm_script::Host::world(&self.host)
                    .read(|store| interfaces.realize_text(id, store))?;
            }
            if let Some((fog, celestial)) = self
                .host
                .skies
                .as_ref()
                .and_then(|sky| sky.stage_request(id))
            {
                if fog.borrow().dirty {
                    let colors = self.sky_fog_colors()?;
                    let mut fog = fog.borrow_mut();
                    fog.dirty = false;
                    fog.colors = colors;
                }
                if celestial
                    && !astronomy_realized
                    && let Some(astronomy) = astronomy
                {
                    self.host
                        .entities
                        .process_entity_stage(astronomy, Stage::Realize, delta_ms)?;
                    astronomy_realized = true;
                }
            }
            if let Some(event) = self
                .host
                .interfaces
                .as_ref()
                .and_then(|interfaces| interfaces.take_timer_event(id))
            {
                self.event(&event)?;
            }
            if self.host.entities.is_valid(id)
                && let Some(interfaces) = &self.host.interfaces
                && interfaces.screenshot_pending(id)
            {
                // Original SCRSHOTER flushes the scene at this point in Realize
                // and sends the event before later layers and control edges.
                let captured = screenshot(&self.host).and_then(|frame| {
                    let Some(frame) = frame else {
                        return Ok(false);
                    };
                    interfaces.finish_screenshot(id, frame)?;
                    Ok(true)
                });
                match captured {
                    Ok(true) => {}
                    Ok(false) => storm_script::Host::trace(
                        &mut self.host,
                        "ERROR!!! screen shot create error",
                    )?,
                    Err(error) => {
                        storm_script::Host::trace(
                            &mut self.host,
                            &format!("ERROR!!! screen shot create error: {error:#}"),
                        )?;
                    }
                }
                self.event("makescrshot")?;
            }
        }
        Ok(())
    }

    fn sky_fog_colors(&mut self) -> Result<Option<std::rc::Rc<[[u8; 4]]>>> {
        let positions = storm_render::model::sky_fog_positions();
        let mut colors = Vec::with_capacity(positions.len());
        for &[x, y, z] in positions {
            let mut message = Message::new(
                "fff",
                [x, y, z].into_iter().map(MessageValue::Float).collect(),
            )?;
            let result =
                self.runtime
                    .emit("WhrCalcFogColor", &mut message, &mut self.host, self.limits)?;
            let color = match result {
                Some(Value::Int(color)) => color.to_be_bytes(),
                None if colors.is_empty() => return Ok(None),
                _ => anyhow::bail!("WhrCalcFogColor must return a packed color"),
            };
            let [a, r, g, b] = color;
            colors.push([r, g, b, a]);
        }
        Ok(Some(colors.into()))
    }
    /// Retains the supported CoreImpl::Run order. State loading, clock
    /// globals, Steam callbacks and dynamic service creation remain separate ports.
    pub fn frame(&mut self, delta_ms: u32) -> Result<FrameResult> {
        self.frame_with_screenshot(delta_ms, &mut |_| Ok(None))
    }
    pub fn frame_with_screenshot(
        &mut self,
        delta_ms: u32,
        screenshot: &mut ScreenshotSource<'_>,
    ) -> Result<FrameResult> {
        self.frame_with_deltas(delta_ms, None, screenshot)
    }
    /// The native TIMER's fixed delta overrides script time scaling while
    /// input still advances by the measured interval.
    pub fn frame_with_timer(&mut self, timer: &crate::clock::Timer) -> Result<FrameResult> {
        self.frame_with_timer_and_screenshot(timer, &mut |_| Ok(None))
    }
    pub fn frame_with_timer_and_screenshot(
        &mut self,
        timer: &crate::clock::Timer,
        screenshot: &mut ScreenshotSource<'_>,
    ) -> Result<FrameResult> {
        let clock = timer.snapshot();
        self.frame_with_deltas(
            clock.real_delta_ms,
            clock.fixed.then_some(clock.fixed_value),
            screenshot,
        )
    }
    fn frame_with_deltas(
        &mut self,
        real_delta_ms: u32,
        fixed_delta: Option<u32>,
        screenshot: &mut ScreenshotSource<'_>,
    ) -> Result<FrameResult> {
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
        let scale = self.host.time_scale.get();
        let delta_ms = match fixed_delta {
            Some(delta) => delta,
            None if scale == 1.0 => real_delta_ms,
            None => (real_delta_ms as f32 * scale) as u32,
        };
        let result = self.frame_inner(delta_ms, real_delta_ms, screenshot);
        if result.is_err() {
            self.phase = Phase::Faulted;
        }
        result
    }
    fn frame_inner(
        &mut self,
        delta_ms: u32,
        real_delta_ms: u32,
        screenshot: &mut ScreenshotSource<'_>,
    ) -> Result<FrameResult> {
        self.host.delta_ms = delta_ms;
        if let Some(interfaces) = &self.host.interfaces {
            interfaces.begin_frame();
        }
        if let Some(seas) = &self.host.seas {
            seas.begin_frame();
        }
        if let Some(foams) = &self.host.coast_foams {
            foams.begin_frame();
        }
        if let Some(rain) = &self.host.rain {
            rain.clear_draws();
        }
        if let Some(sun) = &self.host.sun_glow {
            sun.clear_draws();
        }
        if let Some(astronomy) = &self.host.astronomy {
            astronomy.begin_frame();
        }
        if let Some(sky) = &self.host.skies {
            sky.begin_frame();
        }
        if self.phase == Phase::Ready {
            if self.host.interfaces.is_some()
                && let Some(Value::Object(render)) =
                    self.runtime.global("Render", &mut self.host)?
                && let Some(interfaces) = &self.host.interfaces
            {
                interfaces.initialize_render_attributes(&render)?;
            }
            let limits = Limits {
                instructions: self.startup_instructions,
                ..self.limits
            };
            self.runtime.call("Main", &[], &mut self.host, limits)?;
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
        self.execute_entities(delta_ms)?;
        self.services(Section::Execute, false, &mut context)?;
        self.services(Section::Realize, true, &mut context)?;
        self.realize_entities(delta_ms, screenshot)?;
        self.services(Section::Realize, false, &mut context)?;
        self.process_controls(real_delta_ms)?;
        self.host.entities.new_lifecycle()?;
        if let Some(ship_lights) = &self.host.ship_lights {
            ship_lights.retain_owners(|id| self.host.entities.is_valid(id));
        }
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
        if let Some(interfaces) = &self.host.interfaces {
            interfaces.clear_draws();
        }
        if let Some(seas) = &self.host.seas {
            seas.begin_frame();
        }
        if let Some(foams) = &self.host.coast_foams {
            foams.begin_frame();
        }
        if let Some(rain) = &self.host.rain {
            rain.clear_draws();
        }
        if let Some(sun) = &self.host.sun_glow {
            sun.clear_draws();
        }
        if let Some(astronomy) = &self.host.astronomy {
            astronomy.begin_frame();
        }
        if let Some(sky) = &self.host.skies {
            sky.begin_frame();
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
