use super::{MAX_CONTROLS, State};
use anyhow::{Result, ensure};
use std::collections::BTreeMap;

#[derive(Default)]
struct Node {
    base: String,
    output: String,
    children: Vec<usize>,
    depth: usize,
    timeout: f32,
    elapsed: f32,
    active: bool,
    wait_for_release: bool,
}

pub(super) struct Tree {
    nodes: Vec<Node>,
    output: BTreeMap<String, State>,
}

impl Default for Tree {
    fn default() -> Self {
        Self {
            nodes: vec![Node::default()],
            output: BTreeMap::new(),
        }
    }
}

impl Tree {
    pub fn parent(&self, parent: i32) -> Result<usize> {
        // The implicit root has no script ID. Invalid parents attach to it.
        let parent = usize::try_from(parent)
            .ok()
            .map(|id| id + 1)
            .filter(|&id| id < self.nodes.len())
            .unwrap_or(0);
        ensure!(self.nodes.len() <= MAX_CONTROLS, "control tree size limit");
        ensure!(self.nodes[parent].depth < 64, "control tree depth limit");
        Ok(parent)
    }

    pub fn add(&mut self, parent: usize, base: &str, output: &str, timeout: f32) -> i32 {
        let index = self.nodes.len();
        let depth = self.nodes[parent].depth + 1;
        self.nodes[parent].children.push(index);
        self.nodes.push(Node {
            base: base.to_owned(),
            output: output.to_owned(),
            depth,
            timeout,
            ..Node::default()
        });
        (index - 1) as i32
    }

    pub fn state(&self, name: &str) -> State {
        self.output.get(name).copied().unwrap_or_default()
    }

    pub fn advance(&mut self, delta_ms: u32, sample: impl Fn(&Self, &str) -> State) {
        for state in self.output.values_mut() {
            *state = State::from_held(state.held(), state.held());
        }
        self.process(0, delta_ms as f32 * 0.001, &sample);
        self.output.retain(|_, state| *state != State::Inactive);
    }

    fn process(&mut self, index: usize, delta: f32, sample: &impl Fn(&Self, &str) -> State) {
        if self.nodes[index].base.is_empty() {
            self.nodes[index].active = true;
            self.process_children(index, delta, sample);
            return;
        }

        let held = sample(self, &self.nodes[index].base).held();
        let node = &mut self.nodes[index];
        if node.active {
            if node.elapsed <= node.timeout {
                node.elapsed += delta;
            }
        } else if held {
            node.active = true;
            node.wait_for_release = false;
            node.elapsed = 0.0;
            for position in 0..self.nodes[index].children.len() {
                let child = self.nodes[index].children[position];
                self.nodes[child].active = false;
                self.nodes[child].wait_for_release = false;
            }
            let base = self.nodes[index].base.clone();
            self.control_in_action(0, &base, self.nodes[index].depth);
        } else {
            return;
        }

        let node = &mut self.nodes[index];
        if !node.wait_for_release && node.elapsed >= node.timeout {
            node.active = Self::emit(&mut self.output, &node.output, held);
            return;
        }

        let child_active = self.process_children(index, delta, sample);
        let node = &mut self.nodes[index];
        node.wait_for_release |= child_active;
        if node.wait_for_release && !child_active && !held {
            node.active = false;
        }
    }

    fn process_children(
        &mut self,
        index: usize,
        delta: f32,
        sample: &impl Fn(&Self, &str) -> State,
    ) -> bool {
        let mut active = false;
        for position in 0..self.nodes[index].children.len() {
            let child = self.nodes[index].children[position];
            self.process(child, delta, sample);
            active |= self.nodes[child].active;
        }
        active
    }

    fn control_in_action(&mut self, index: usize, name: &str, depth: usize) -> bool {
        let node = &mut self.nodes[index];
        if depth <= node.depth {
            return false;
        }
        if node.base == name {
            if self.output.get(&node.output) == Some(&State::Activated) {
                self.output.remove(&node.output);
            }
            node.wait_for_release = true;
            node.active = true;
            node.elapsed = 0.0;
        }
        if node.active {
            for position in 0..self.nodes[index].children.len() {
                let child = self.nodes[index].children[position];
                let child_consumed = self.control_in_action(child, name, depth);
                self.nodes[index].wait_for_release |= child_consumed;
            }
        }
        self.nodes[index].wait_for_release
    }

    fn emit(output: &mut BTreeMap<String, State>, name: &str, held: bool) -> bool {
        if name.is_empty() {
            return false;
        }
        let state = match output.get_mut(name) {
            Some(state) => {
                *state = State::from_held(state.held(), held);
                *state
            }
            None => {
                // A short tap still fires once when the timeout expires.
                output.insert(name.to_owned(), State::Activated);
                State::Activated
            }
        };
        state.held()
    }
}
