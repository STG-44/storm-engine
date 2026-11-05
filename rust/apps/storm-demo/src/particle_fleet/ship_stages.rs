//! Native ship stage selection and priorities around retained numeric bodies.
//! Input/output copies bridge the existing preview; original model draw is absent.
use super::*;
use storm_core::{EntityManager, LayerType, Stage};
use storm_sim::ship_stages::{self, Priorities, StagePort};

#[derive(Clone)]
pub(super) struct Frame {
    pub number: u32,
    pub body: storm_sim::ship_motion::ShipMotion,
    pub rocking: ShipRocking,
    pub limits: FlatMovement,
    pub presented: Option<RootPose>,
}
pub(super) struct Slot {
    pub id: EntityId,
    frame: Frame,
    ready: bool,
    error: Option<String>,
    execute_calls: u32,
    realize_calls: u32,
    destroyed: bool,
    records: Vec<Json>,
    cleanup: Option<ship_cleanup::Binding>,
}
struct Actor {
    slot: Rc<RefCell<Slot>>,
    sea: Rc<RefCell<storm_sim::sea::Sea>>,
}
impl StagePort for Actor {
    fn execute(&mut self, delta: u32) -> Result<()> {
        let mut slot = self.slot.borrow_mut();
        ensure!(slot.ready, "ship Execute has no prepared numeric inputs");
        slot.ready = false;
        let mut frame = slot.frame.clone();
        let sea = self.sea.borrow();
        let revision = sea.revision();
        let camera = sea.surface().camera.map(f32::to_bits);
        let mut surface = sea.surface().clone();
        drop(sea);
        frame.rocking.parameters.offsets = frame.body.offsets;
        let mut port = RockingMovement {
            limits: &frame.limits,
            rocking: &mut frame.rocking,
            sea: &mut surface,
            reflect: false,
        };
        step_body(&mut frame.body, frame.presented.as_mut(), delta, &mut port)?;
        slot.execute_calls += 1;
        slot.records.push(json!({"frame":frame.number,"stage":"execute","delta_ms":delta,"sea_revision":revision,"camera_bits":camera,"position_bits":frame.body.state.position.map(f32::to_bits)}));
        slot.frame = frame;
        Ok(())
    }
    fn realize(&mut self, delta: u32) -> Result<()> {
        let mut slot = self.slot.borrow_mut();
        let sea = self.sea.borrow();
        let frame = slot.frame.number;
        let position = slot.frame.body.state.position.map(f32::to_bits);
        slot.realize_calls += 1;
        slot.records.push(json!({"frame":frame,"stage":"realize_bridge","delta_ms":delta,"sea_revision":sea.revision(),"camera_bits":sea.surface().camera.map(f32::to_bits),"position_bits":position}));
        Ok(())
    }
}
impl Entity for Actor {
    fn destroy(&mut self, context: &mut storm_core::EntityDestroy<'_>) -> Result<()> {
        let binding = self.slot.borrow().cleanup.clone();
        if let Some(binding) = binding {
            binding.destroy(context)?;
        }
        Ok(())
    }
    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        let binding = self.slot.borrow().cleanup.clone();
        if matches!(
            message.current_value(),
            Some(Value::Long(storm_sim::fire::MSG_SHIP_SAFE_DELETE))
        ) && let Some(binding) = binding
        {
            return binding.safe_delete(message);
        }
        Ok(0)
    }
    fn process_stage(&mut self, stage: Stage, delta: u32) {
        // Keep the first callback error until the frame owner observes it.
        let healthy = self.slot.borrow().error.is_none();
        if healthy && let Err(error) = ship_stages::dispatch(stage, delta, self) {
            self.slot.borrow_mut().error = Some(error.to_string());
        }
    }
}
impl Drop for Actor {
    fn drop(&mut self) {
        let mut slot = self.slot.borrow_mut();
        slot.destroyed = true;
        if let Some(binding) = &slot.cleanup {
            binding.dropped(slot.id);
        }
    }
}
pub(super) struct Ships {
    slots: Vec<Rc<RefCell<Slot>>>,
    cleanup: Option<Rc<RefCell<ship_cleanup::World>>>,
}
impl Ships {
    pub fn new(
        manager: &mut EntityManager,
        sea: Rc<RefCell<storm_sim::sea::Sea>>,
        frames: Vec<Frame>,
    ) -> Result<Self> {
        ensure!(frames.len() == 2, "ship stage fixture requires two owners");
        manager.set_layer_type(1, LayerType::Execute)?;
        let mut slots = Vec::new();
        for frame in frames {
            let slot = Rc::new(RefCell::new(Slot {
                id: EntityId::INVALID,
                frame,
                ready: false,
                error: None,
                execute_calls: 0,
                realize_calls: 0,
                destroyed: false,
                records: Vec::new(),
                cleanup: None,
            }));
            let id = manager.insert(
                "ship",
                Actor {
                    slot: slot.clone(),
                    sea: sea.clone(),
                },
            )?;
            slot.borrow_mut().id = id;
            Priorities::from_script(None, None).register(manager, id, 1, 0)?;
            slots.push(slot);
        }
        Ok(Self {
            slots,
            cleanup: None,
        })
    }
    pub fn prepare(&mut self, manager: &EntityManager, frames: Vec<Option<Frame>>) -> Result<()> {
        ensure!(frames.len() == self.slots.len(), "ship stage input count");
        for (slot, frame) in self.slots.iter().zip(frames) {
            let mut slot = slot.borrow_mut();
            ensure!(
                manager.is_valid(slot.id) == frame.is_some(),
                "ship visibility and entity validity diverge"
            );
            if let Some(frame) = frame {
                slot.frame = frame;
                slot.ready = true;
            }
        }
        Ok(())
    }
    pub fn outputs(&mut self, manager: &EntityManager) -> Result<Vec<Option<Frame>>> {
        self.check_errors()?;
        self.slots
            .iter()
            .map(|s| {
                let s = s.borrow();
                ensure!(
                    !s.ready,
                    "ship Execute layer did not consume numeric inputs"
                );
                Ok(manager.is_valid(s.id).then(|| s.frame.clone()))
            })
            .collect()
    }
    pub fn check_errors(&mut self) -> Result<()> {
        for slot in &self.slots {
            if let Some(error) = slot.borrow_mut().error.take() {
                anyhow::bail!(error);
            }
        }
        Ok(())
    }
    pub fn erase(&mut self, manager: &mut EntityManager, index: usize) -> Result<()> {
        let slot = self
            .slots
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("invalid ship owner"))?
            .borrow();
        let (id, cleanup) = (slot.id, slot.cleanup.is_some());
        drop(slot);
        if cleanup {
            manager.send_message(
                id,
                &mut Message::new(
                    "l",
                    vec![Value::Long(storm_sim::fire::MSG_SHIP_SAFE_DELETE)],
                )?,
            )?;
        }
        manager.erase(id);
        Ok(())
    }
    pub fn enable_cleanup(
        &mut self,
        manager: &mut EntityManager,
        controllers: Vec<std::rc::Weak<RefCell<Controller>>>,
    ) -> Result<()> {
        ensure!(self.cleanup.is_none(), "cleanup already enabled");
        let ids = self.slots.iter().map(|s| s.borrow().id).collect();
        let (world, bindings) = ship_cleanup::create(manager, ids, controllers)?;
        for (slot, binding) in self.slots.iter().zip(bindings) {
            slot.borrow_mut().cleanup = Some(binding);
        }
        self.cleanup = Some(world);
        Ok(())
    }
    pub fn cleanup_world(&self) -> Option<Rc<RefCell<ship_cleanup::World>>> {
        self.cleanup.clone()
    }
    pub fn report(&self, manager: &EntityManager) -> Json {
        let mut report = json!({"dispatcher":"shared_core_entity_manager","execute_layer":1,"realize_layer":0,"priority_execute":2,"priority_realize":31,
            "numeric_input_output_bridge":true,"full_ship_entity":false,"original_model_draw":false,"original_mount_services":false,
            "bodies":self.slots.iter().enumerate().map(|(index,slot)|{let s=slot.borrow();json!({"preview_owner":index+1,"entity":s.id.raw(),"valid":manager.is_valid(s.id),"lookup":manager.get(s.id).is_some(),"destroyed":s.destroyed,"execute_calls":s.execute_calls,"realize_calls":s.realize_calls,"trace":s.records})}).collect::<Vec<_>>()});
        if let Some(world) = &self.cleanup {
            report["cleanup"] = world.borrow().report(manager);
        }
        report
    }
}

impl Fleet {
    fn ship_stage_frames(&self, number: u32) -> Vec<Option<Frame>> {
        let motion = self.native_motion.as_ref().unwrap();
        self.visible
            .iter()
            .enumerate()
            .map(|(i, &visible)| {
                visible.then(|| Frame {
                    number,
                    body: motion.bodies[i].clone(),
                    rocking: motion.rocking.as_ref().unwrap().bodies[i].clone(),
                    limits: motion.inputs[i],
                    presented: motion.presentation.as_ref().map(|p| p[i]),
                })
            })
            .collect()
    }
    pub(super) fn ship_staged(&self) -> bool {
        self.native_motion
            .as_ref()
            .and_then(|m| m.rocking.as_ref())
            .and_then(|r| r.native.as_ref())
            .is_some_and(sea::State::ship_staged)
    }
    pub(super) fn enable_native_ship_stages(&mut self) -> Result<()> {
        ensure!(
            self.sea_staged() && self.next_frame == 0,
            "ship stages require a fresh staged sea"
        );
        let frames = self
            .ship_stage_frames(0)
            .into_iter()
            .map(Option::unwrap)
            .collect();
        self.native_motion
            .as_mut()
            .unwrap()
            .rocking
            .as_mut()
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .enable_ship_stages(frames)
    }
    pub(super) fn execute_ship_stages(&mut self, number: u32, delta: u32) -> Result<()> {
        let frames = self.ship_stage_frames(number);
        let motion = self.native_motion.as_mut().unwrap();
        let output = motion
            .rocking
            .as_mut()
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .execute_ships(delta, frames)?;
        for (index, frame) in output.into_iter().enumerate() {
            if let Some(frame) = frame {
                motion.bodies[index] = frame.body;
                motion.rocking.as_mut().unwrap().bodies[index] = frame.rocking;
                if let Some(poses) = motion.presentation.as_mut() {
                    poses[index] = frame.presented.unwrap();
                }
                if motion.presentation.is_some() {
                    let binding = motion.placement.as_mut().unwrap();
                    ship_presentation::publish_character(
                        &mut binding.attributes,
                        binding.characters[index],
                        &motion.bodies[index],
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
