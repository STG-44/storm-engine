//! PC control bindings and frame-based input, following PCS_CONTROLS.
mod tree;

use anyhow::{Result, bail, ensure};
use std::collections::BTreeMap;
use storm_script::Value;
use tree::Tree;

const MOUSE_X: usize = 256;
const MOUSE_Y: usize = 257;
const WHEEL_UP: usize = 258;
const WHEEL_DOWN: usize = 259;
const BUTTON_COUNT: usize = MOUSE_X;
const MAX_CONTROLS: usize = 4096;
const MAX_EDIT_KEYS: usize = 1024;
const USE_AXIS_AS_BUTTON: u32 = 1;
const INVERSE_CONTROL: u32 = 4;

/// Desktop input expressed with the key codes used by the original scripts.
#[derive(Clone, Copy, Debug)]
pub enum ControlInput {
    Button { code: u8, pressed: bool },
    Text(char),
    NumpadEnter(bool),
    MouseMotion { x: f64, y: f64 },
    MouseWheel(i32),
    FocusLost,
}

#[derive(Clone, Copy)]
pub(crate) enum EditKey {
    Character(char),
    Backspace,
    Delete,
    Home,
    End,
    Left,
    Right,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum State {
    #[default]
    Inactive,
    Active,
    Deactivated,
    Activated,
}

impl State {
    fn from_held(was_held: bool, held: bool) -> Self {
        match (was_held, held) {
            (false, false) => Self::Inactive,
            (false, true) => Self::Activated,
            (true, false) => Self::Deactivated,
            (true, true) => Self::Active,
        }
    }

    fn held(self) -> bool {
        matches!(self, Self::Activated | Self::Active)
    }
}

struct Hardware {
    buttons: [bool; BUTTON_COUNT],
    numpad_enter: bool,
    motion: [f64; 2],
    wheel: i32,
    previous: [i32; WHEEL_DOWN + 1],
    current: [i32; WHEEL_DOWN + 1],
    mouse_sensitivity: [f32; 2],
    axes: [f32; 2],
}

impl Default for Hardware {
    fn default() -> Self {
        Self {
            buttons: [false; BUTTON_COUNT],
            numpad_enter: false,
            motion: [0.0; 2],
            wheel: 0,
            previous: [0; WHEEL_DOWN + 1],
            current: [0; WHEEL_DOWN + 1],
            mouse_sensitivity: [1.0; 2],
            axes: [0.0; 2],
        }
    }
}

impl Hardware {
    fn input(&mut self, input: ControlInput) {
        match input {
            ControlInput::Button { code, pressed } => self.buttons[usize::from(code)] = pressed,
            ControlInput::NumpadEnter(pressed) => self.numpad_enter = pressed,
            ControlInput::Text(_) => {}
            ControlInput::MouseMotion { x, y } if x.is_finite() && y.is_finite() => {
                self.motion[0] += x;
                self.motion[1] += y;
            }
            ControlInput::MouseMotion { .. } => {}
            ControlInput::MouseWheel(steps) => self.wheel = self.wheel.saturating_add(steps),
            ControlInput::FocusLost => {
                self.buttons.fill(false);
                self.numpad_enter = false;
                self.motion = [0.0; 2];
                self.wheel = 0;
            }
        }
    }

    fn advance(&mut self) {
        self.previous = self.current;
        for (code, pressed) in self.buttons.iter().copied().enumerate() {
            self.current[code] = i32::from(pressed);
        }
        self.current[13] |= i32::from(self.numpad_enter);
        // Virtual Shift, Control and Alt each represent either physical key.
        for (generic, left, right) in [(16, 160, 161), (17, 162, 163), (18, 164, 165)] {
            self.current[generic] |= self.current[left] | self.current[right];
        }
        if self.current[18] != 0 {
            self.current[9] = 0; // PCS_CONTROLS suppresses Alt+Tab.
        }
        for (index, delta) in self.motion.iter_mut().enumerate() {
            let whole = *delta as i32;
            self.current[MOUSE_X + index] = whole;
            self.axes[index] = whole as f32 * self.mouse_sensitivity[index];
            // Preserve fractional desktop motion until it can be represented in counts.
            *delta = if delta.is_finite() && delta.abs() <= f64::from(i32::MAX) {
                *delta - f64::from(whole)
            } else {
                0.0
            };
        }
        self.current[WHEEL_UP] = self.wheel.max(0);
        self.current[WHEEL_DOWN] = self.wheel.saturating_neg().max(0);
        self.wheel = 0;
    }
}

struct Binding {
    name: String,
    system: Option<usize>,
    flags: u32,
    locked: bool,
    from_tree: bool,
}

impl Binding {
    fn state(&self, hardware: &Hardware, tree: &Tree) -> State {
        if self.locked {
            return State::Inactive;
        }
        if self.from_tree {
            return tree.state(&self.name);
        }
        let Some(code) = self.system else {
            return State::Inactive;
        };
        let inverted = self.flags & INVERSE_CONTROL != 0;
        let as_button = self.flags & USE_AXIS_AS_BUTTON != 0;
        if (MOUSE_X..=MOUSE_Y).contains(&code) && !as_button {
            // A mouse axis has a value but never sends a button edge by itself.
            return if inverted {
                State::Active
            } else {
                State::Inactive
            };
        }
        let held = |value: i32| (value > 0) != inverted;
        State::from_held(held(hardware.previous[code]), held(hardware.current[code]))
    }
}

#[derive(Default)]
pub(crate) struct Controls {
    hardware: Hardware,
    bindings: Vec<Binding>,
    names: BTreeMap<String, usize>,
    tree: Tree,
    locked: bool,
    edit_keys: Vec<EditKey>,
    queued_edit_keys: Vec<EditKey>,
}

impl Controls {
    pub fn input(&mut self, input: ControlInput) {
        let edit = match input {
            ControlInput::Text(c) if !c.is_control() => Some(EditKey::Character(c)),
            ControlInput::Button {
                code: 8,
                pressed: true,
            } => Some(EditKey::Backspace),
            ControlInput::Button {
                code: 46,
                pressed: true,
            } => Some(EditKey::Delete),
            ControlInput::Button {
                code: 36,
                pressed: true,
            } => Some(EditKey::Home),
            ControlInput::Button {
                code: 35,
                pressed: true,
            } => Some(EditKey::End),
            ControlInput::Button {
                code: 37,
                pressed: true,
            } => Some(EditKey::Left),
            ControlInput::Button {
                code: 39,
                pressed: true,
            } => Some(EditKey::Right),
            ControlInput::FocusLost => {
                self.edit_keys.clear();
                self.queued_edit_keys.clear();
                None
            }
            _ => None,
        };
        if let Some(edit) = edit
            && self.queued_edit_keys.len() < MAX_EDIT_KEYS
        {
            self.queued_edit_keys.push(edit);
        }
        self.hardware.input(input);
    }

    pub fn advance(&mut self, delta_ms: u32) {
        self.hardware.advance();
        self.edit_keys.clear();
        std::mem::swap(&mut self.edit_keys, &mut self.queued_edit_keys);
        let bindings = &self.bindings;
        let names = &self.names;
        let hardware = &self.hardware;
        self.tree.advance(delta_ms, |tree, name| {
            if self.locked {
                return State::Inactive;
            }
            names
                .get(&name.to_ascii_lowercase())
                .map_or(State::Inactive, |&id| bindings[id].state(hardware, tree))
        });
    }

    pub(crate) fn edit_keys(&self) -> &[EditKey] {
        &self.edit_keys
    }

    pub fn len(&self) -> usize {
        self.bindings.len()
    }
    pub fn mouse_axis(&self, name: &str) -> Option<f32> {
        let binding = &self.bindings[*self.names.get(&name.to_ascii_lowercase())?];
        let axis = binding.system?.checked_sub(MOUSE_X)?;
        let value = *self.hardware.axes.get(axis)?;
        if self.locked || binding.locked {
            return Some(0.0);
        }
        if binding.flags & INVERSE_CONTROL == 0 {
            Some(value)
        } else if binding.flags & USE_AXIS_AS_BUTTON != 0 {
            Some(1.0 - value)
        } else {
            Some(-value)
        }
    }
    fn named_state(&self, name: &str) -> State {
        if self.locked {
            return State::Inactive;
        }
        self.names
            .get(&name.to_ascii_lowercase())
            .map_or(State::Inactive, |&id| {
                self.bindings[id].state(&self.hardware, &self.tree)
            })
    }

    pub fn activated(&self, name: &str) -> bool {
        self.named_state(name) == State::Activated
    }

    pub fn released(&self, name: &str) -> bool {
        self.named_state(name) == State::Deactivated
    }

    pub fn idle(&self, name: &str) -> bool {
        self.named_state(name) == State::Inactive
    }

    pub fn held(&self, name: &str) -> bool {
        matches!(self.named_state(name), State::Active | State::Activated)
    }

    pub fn edge(&self, id: usize) -> Option<(&'static str, &str)> {
        let binding = &self.bindings[id];
        // Core dispatches by numeric control ID. The global lock only applies
        // to the original name-based lookup, which the tree uses above.
        let event = match binding.state(&self.hardware, &self.tree) {
            State::Activated => "Control Activation",
            State::Deactivated => "Control Deactivation",
            _ => return None,
        };
        Some((event, &binding.name))
    }

    fn create(&mut self, name: &str) -> Result<usize> {
        let key = name.to_ascii_lowercase();
        if let Some(id) = self.names.get(&key) {
            return Ok(*id);
        }
        ensure!(
            self.bindings.len() < MAX_CONTROLS,
            "control registration limit"
        );
        let id = self.bindings.len();
        self.bindings.push(Binding {
            name: name.to_owned(),
            system: None,
            flags: 0,
            locked: false,
            from_tree: false,
        });
        self.names.insert(key, id);
        Ok(id)
    }

    pub fn script_call(&mut self, function: &str, args: &[Value]) -> Option<Result<Value>> {
        let result = match function {
            "createcontrol" => self.create_control(args),
            "mapcontrol" | "setcontrolflags" | "setcontroltreshold" => {
                self.configure(function, args)
            }
            "lockcontrol" => self.lock(args),
            "addcontroltreenode" => self.add_node(args),
            "xi_controlmakeinvert" => self.set_invert(args),
            "xi_setmousesensitivity" => self.set_mouse_sensitivity(args),
            _ => return None,
        };
        Some(result)
    }

    fn set_invert(&mut self, args: &[Value]) -> Result<Value> {
        let [name, flag] = args else {
            bail!("XI_ControlMakeInvert expects a control name and flag");
        };
        let id = self.create(name.resolved()?.text()?)?;
        self.bindings[id].flags = match flag.resolved()? {
            Value::Int(flag) if flag != 0 => 0,
            _ => INVERSE_CONTROL,
        };
        Ok(Value::Void)
    }

    fn set_mouse_sensitivity(&mut self, args: &[Value]) -> Result<Value> {
        let [x, y] = args else {
            bail!("XI_SetMouseSensitivity expects two arguments");
        };
        let read = crate::script_libraries::float_or_one;
        let sensitivity = [read(x)?, read(y)?];
        ensure!(
            sensitivity.into_iter().all(f32::is_finite),
            "mouse sensitivity requires finite settings"
        );
        self.hardware.mouse_sensitivity = sensitivity;
        Ok(Value::Void)
    }

    fn create_control(&mut self, args: &[Value]) -> Result<Value> {
        let [name] = args else {
            bail!("CreateControl expects a name")
        };
        Ok(Value::Int(self.create(name.resolved()?.text()?)? as i32))
    }

    fn configure(&mut self, function: &str, args: &[Value]) -> Result<Value> {
        let [id, value] = args else {
            bail!("{function} expects two arguments")
        };
        let id = id.resolved()?.integer()?;
        let value = value.resolved()?;
        if function == "setcontroltreshold" {
            ensure!(
                matches!(value, Value::Float(_) | Value::Int(_)),
                "control threshold must be numeric"
            );
            // PCS_CONTROLS::SetControlTreshold has an empty implementation.
            return Ok(Value::Void);
        }
        let code = value.integer()?;
        if let Some(binding) = usize::try_from(id)
            .ok()
            .and_then(|id| self.bindings.get_mut(id))
        {
            if function == "mapcontrol" {
                binding.system = usize::try_from(code)
                    .ok()
                    .filter(|&code| code <= WHEEL_DOWN);
            } else {
                binding.flags = code as u32;
            }
        }
        Ok(Value::Void)
    }

    fn lock(&mut self, args: &[Value]) -> Result<Value> {
        let [name, flag] = args else {
            bail!("LockControl expects a name and flag")
        };
        let name = name.resolved()?;
        let name = name.text()?;
        let flag = flag.resolved()?.integer()? != 0;
        if name.is_empty() {
            self.locked = flag;
        } else if let Some(&id) = self.names.get(&name.to_ascii_lowercase()) {
            self.bindings[id].locked = flag;
        }
        Ok(Value::Void)
    }

    fn add_node(&mut self, args: &[Value]) -> Result<Value> {
        let [parent, base, output, timeout] = args else {
            bail!("AddControlTreeNode expects four arguments")
        };
        let Value::Float(timeout) = timeout.resolved()? else {
            bail!("control tree timeout must be float")
        };
        let parent = self.tree.parent(parent.resolved()?.integer()?)?;
        let base = base.resolved()?;
        let base = base.text()?;
        let output = output.resolved()?;
        let output = output.text()?;
        let id = self.create(output)?;
        self.bindings[id].from_tree = true;
        Ok(Value::Int(self.tree.add(parent, base, output, timeout)))
    }
}
