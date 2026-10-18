//! ATTRIBUTES tree semantics, with checked handles in place of C++ pointers.
use crate::class_hash;
use anyhow::{Result, anyhow, ensure};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

mod numbers;
pub use numbers::format_float;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
const MAX_DEPTH: usize = 256;
const MAX_NODES: usize = 1_000_000;
const MAX_NAME: usize = 1023;

/// Zero represents a null attribute pointer. IDs are never reused, including
/// between stores; stale and foreign handles cannot alias another node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AttributeId(u64);
impl AttributeId {
    pub const INVALID: Self = Self(0);
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug)]
pub struct AttributeNode {
    name_code: u32,
    value: Option<String>,
    parent: Option<AttributeId>,
    children: Vec<AttributeId>,
    break_enabled: bool,
}
impl AttributeNode {
    pub fn name_code(&self) -> u32 {
        self.name_code
    }
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
    pub fn parent(&self) -> Option<AttributeId> {
        self.parent
    }
    pub fn children(&self) -> &[AttributeId] {
        &self.children
    }
}

struct Name {
    text: String,
    hash: u32,
}
struct StringCodec {
    buckets: [Vec<Name>; 512],
    count: usize,
}
impl Default for StringCodec {
    fn default() -> Self {
        Self {
            buckets: std::array::from_fn(|_| Vec::new()),
            count: 0,
        }
    }
}
impl StringCodec {
    fn intern(&mut self, name: &str) -> Result<u32> {
        validate_name(name)?;
        let hash = class_hash(name);
        let bucket_index = (hash & 511) as usize;
        let bucket = &mut self.buckets[bucket_index];
        let index = if let Some(index) = bucket
            .iter()
            .position(|n| n.hash == hash && n.text.eq_ignore_ascii_case(name))
        {
            index
        } else {
            ensure!(bucket.len() < 65536, "attribute name bucket exhausted");
            bucket.push(Name {
                text: name.into(),
                hash,
            });
            self.count += 1;
            bucket.len() - 1
        };
        Ok(((bucket_index as u32) << 16) | index as u32)
    }
    fn name(&self, code: u32) -> Option<&str> {
        self.buckets
            .get((code >> 16) as usize)?
            .get((code & 65535) as usize)
            .map(|n| n.text.as_str())
    }
}

#[derive(Default)]
pub struct AttributeStore {
    nodes: BTreeMap<AttributeId, AttributeNode>,
    codec: StringCodec,
    break_notifications: u64,
}
impl AttributeStore {
    pub fn get(&self, id: AttributeId) -> Option<&AttributeNode> {
        self.nodes.get(&id)
    }
    pub fn name(&self, id: AttributeId) -> Option<&str> {
        self.codec.name(self.get(id)?.name_code)
    }
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    pub fn intern_name(&mut self, name: &str) -> Result<u32> {
        self.codec.intern(name)
    }
    pub fn name_count(&self) -> usize {
        self.codec.count
    }
    pub fn break_notifications(&self) -> u64 {
        self.break_notifications
    }
    pub fn create_root(&mut self) -> Result<AttributeId> {
        let name = self.codec.intern("root")?;
        self.allocate(name, None, None)
    }
    fn allocate(
        &mut self,
        code: u32,
        parent: Option<AttributeId>,
        value: Option<&str>,
    ) -> Result<AttributeId> {
        ensure!(
            self.nodes.len() < MAX_NODES,
            "attribute node limit exceeded"
        );
        ensure!(
            self.codec.name(code).is_some(),
            "unknown attribute name code"
        );
        if let Some(parent) = parent {
            ensure!(
                self.depth(parent)? < MAX_DEPTH,
                "attribute depth limit exceeded"
            );
        }
        let raw = NEXT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| anyhow!("attribute IDs exhausted"))?;
        let id = AttributeId(raw);
        self.nodes.insert(
            id,
            AttributeNode {
                name_code: code,
                value: value.map(str::to_owned),
                parent,
                children: Vec::new(),
                break_enabled: false,
            },
        );
        if let Some(parent) = parent {
            self.nodes
                .get_mut(&parent)
                .expect("checked parent")
                .children
                .push(id);
        }
        Ok(id)
    }
    fn depth(&self, id: AttributeId) -> Result<usize> {
        let mut depth = 0;
        let mut current = Some(id);
        while let Some(id) = current {
            let node = self
                .get(id)
                .ok_or_else(|| anyhow!("invalid attribute handle"))?;
            depth += 1;
            current = node.parent;
        }
        Ok(depth)
    }
    /// Always appends, including duplicate names; None and Some("") differ.
    pub fn create_attribute(
        &mut self,
        parent: AttributeId,
        name: &str,
        value: Option<&str>,
    ) -> Result<AttributeId> {
        self.depth(parent)?;
        let code = self.codec.intern(name)?;
        self.allocate(code, Some(parent), value)
    }
    pub fn child(&self, parent: AttributeId, name: &str) -> Option<AttributeId> {
        self.get(parent)?
            .children
            .iter()
            .copied()
            .find(|id| self.name(*id).is_some_and(|n| n.eq_ignore_ascii_case(name)))
    }
    pub fn child_by_code(&self, parent: AttributeId, code: u32) -> Option<AttributeId> {
        self.get(parent)?
            .children
            .iter()
            .copied()
            .find(|id| self.get(*id).is_some_and(|n| n.name_code == code))
    }
    pub fn child_at(&self, parent: AttributeId, index: usize) -> Option<AttributeId> {
        self.get(parent)?.children.get(index).copied()
    }
    /// VerifyAttributeClass creates an empty STRING, unlike CreateSubAClass.
    pub fn verify_attribute(&mut self, parent: AttributeId, name: &str) -> Result<AttributeId> {
        match self.child(parent, name) {
            Some(id) => Ok(id),
            None => self.create_attribute(parent, name, Some("")),
        }
    }
    /// SetAttribute returns a zero-based index, and bypasses SetValue/SetBreak.
    pub fn set_attribute(
        &mut self,
        parent: AttributeId,
        name: &str,
        value: Option<&str>,
    ) -> Result<usize> {
        self.depth(parent)?;
        let code = self.codec.intern(name)?;
        if let Some(id) = self.child_by_code(parent, code) {
            self.nodes.get_mut(&id).expect("existing child").value = value.map(str::to_owned);
            return Ok(self
                .get(parent)
                .expect("checked parent")
                .children
                .iter()
                .position(|child| *child == id)
                .expect("child index"));
        }
        self.allocate(code, Some(parent), value)?;
        Ok(self.get(parent).expect("checked parent").children.len() - 1)
    }
    pub fn set_value(&mut self, id: AttributeId, value: Option<&str>) -> Result<()> {
        let node = self
            .nodes
            .get_mut(&id)
            .ok_or_else(|| anyhow!("invalid attribute handle"))?;
        node.value = value.map(str::to_owned);
        if node.break_enabled {
            self.break_notifications = self.break_notifications.saturating_add(1);
        }
        Ok(())
    }
    pub fn set_break(&mut self, id: AttributeId, enabled: bool) -> Result<()> {
        self.nodes
            .get_mut(&id)
            .ok_or_else(|| anyhow!("invalid attribute handle"))?
            .break_enabled = enabled;
        Ok(())
    }
    pub fn set_name(&mut self, id: AttributeId, name: &str) -> Result<()> {
        ensure!(self.get(id).is_some(), "invalid attribute handle");
        let code = self.codec.intern(name)?;
        self.nodes
            .get_mut(&id)
            .expect("checked attribute")
            .name_code = code;
        Ok(())
    }
    /// Literal attribute lookup: dots in `name` have no special meaning.
    pub fn value(&self, id: AttributeId, name: Option<&str>) -> Option<&str> {
        self.get(match name {
            Some(name) => self.child(id, name)?,
            None => id,
        })?
        .value()
    }
    pub fn as_dword(&self, id: AttributeId, name: Option<&str>, default: u32) -> u32 {
        self.value(id, name).map_or(default, numbers::parse_dword)
    }
    pub fn as_float(&self, id: AttributeId, name: Option<&str>, default: f32) -> f32 {
        self.value(id, name).map_or(default, numbers::parse_float)
    }
    /// Preserves the original named setter's index-to-bool quirk: writing child
    /// zero returns false, even though the value was successfully written.
    pub fn set_dword(&mut self, id: AttributeId, name: Option<&str>, value: u32) -> Result<bool> {
        let text = value.to_string();
        match name {
            Some(name) => Ok(self.set_attribute(id, name, Some(&text))? != 0),
            None => {
                self.set_value(id, Some(&text))?;
                Ok(true)
            }
        }
    }
    pub fn set_float(&mut self, id: AttributeId, name: Option<&str>, value: f32) -> Result<bool> {
        let text = format_float(value);
        match name {
            Some(name) => Ok(self.set_attribute(id, name, Some(&text))? != 0),
            None => {
                self.set_value(id, Some(&text))?;
                Ok(true)
            }
        }
    }
    pub fn create_path(&mut self, root: AttributeId, path: &str) -> Result<AttributeId> {
        let parts = validate_path(path)?;
        ensure!(
            self.depth(root)? + parts.len() <= MAX_DEPTH,
            "attribute depth limit exceeded"
        );
        // Preflight node capacity, so a rejected path does not leave partial nodes.
        let mut at = Some(root);
        let mut missing = 0;
        for part in &parts {
            at = at.and_then(|id| self.child(id, part));
            if at.is_none() {
                missing += 1;
            }
        }
        ensure!(
            self.nodes.len() + missing <= MAX_NODES,
            "attribute node limit exceeded"
        );
        let codes = parts
            .iter()
            .map(|part| self.codec.intern(part))
            .collect::<Result<Vec<_>>>()?;
        let mut current = root;
        for code in codes {
            current = match self.child_by_code(current, code) {
                Some(id) => id,
                None => self.allocate(code, Some(current), None)?,
            };
        }
        Ok(current)
    }
    /// FindAClass interns queried names even on a miss. An empty remainder
    /// returns the current node, including after a trailing dot.
    pub fn find_path(&mut self, root: AttributeId, path: &str) -> Result<Option<AttributeId>> {
        let mut parts = validate_path(path)?;
        if self.get(root).is_none() {
            return Ok(None);
        }
        if parts.last() == Some(&"") {
            parts.pop();
        }
        let mut current = root;
        for part in parts {
            let code = self.codec.intern(part)?;
            let Some(id) = self.child_by_code(current, code) else {
                return Ok(None);
            };
            current = id;
        }
        Ok(Some(current))
    }
    /// Read-only counterpart for native consumers; does not intern misses.
    pub fn lookup_path(&self, root: AttributeId, path: &str) -> Result<Option<AttributeId>> {
        let mut parts = validate_path(path)?;
        if self.get(root).is_none() {
            return Ok(None);
        }
        if parts.last() == Some(&"") {
            parts.pop();
        }
        let mut current = root;
        for part in parts {
            let Some(id) = self.child(current, part) else {
                return Ok(None);
            };
            current = id;
        }
        Ok(Some(current))
    }
    /// Deep copy retains names/values/order and clears all break flags, as Copy.
    /// Parent handles are repaired, rather than retaining C++ move temporaries.
    pub fn copy(&mut self, source: AttributeId) -> Result<AttributeId> {
        let node = self
            .get(source)
            .ok_or_else(|| anyhow!("invalid attribute handle"))?
            .clone();
        let mut stack = vec![source];
        let mut count = 0;
        while let Some(id) = stack.pop() {
            count += 1;
            stack.extend(self.get(id).expect("live child").children.iter().copied());
        }
        ensure!(
            self.nodes.len() + count <= MAX_NODES,
            "attribute node limit exceeded"
        );
        let root = self.allocate(node.name_code, None, node.value.as_deref())?;
        let mut stack = node
            .children
            .iter()
            .rev()
            .map(|id| (*id, root))
            .collect::<Vec<_>>();
        while let Some((old, parent)) = stack.pop() {
            let node = self.get(old).expect("live source child").clone();
            let new = self.allocate(node.name_code, Some(parent), node.value.as_deref())?;
            stack.extend(node.children.iter().rev().map(|id| (*id, new)));
        }
        Ok(root)
    }
    /// DeleteAttributeClassX(root, root) clears children, keeps the root/value,
    /// and returns false. Deleting a proper descendant returns true.
    pub fn delete_class(&mut self, root: AttributeId, target: AttributeId) -> Result<bool> {
        ensure!(self.get(root).is_some(), "invalid attribute root");
        if root == target {
            let children =
                std::mem::take(&mut self.nodes.get_mut(&root).expect("checked root").children);
            for child in children {
                self.remove_subtree(child);
            }
            return Ok(false);
        }
        let mut current = self.get(target).and_then(|n| n.parent);
        while let Some(id) = current {
            if id == root {
                let parent = self
                    .get(target)
                    .and_then(|n| n.parent)
                    .expect("descendant parent");
                self.nodes
                    .get_mut(&parent)
                    .expect("live parent")
                    .children
                    .retain(|id| *id != target);
                self.remove_subtree(target);
                return Ok(true);
            }
            current = self.get(id).and_then(|n| n.parent);
        }
        Ok(false)
    }
    /// Explicit owner release. Entity bindings borrow roots and do not own them.
    pub fn drop_root(&mut self, root: AttributeId) -> Result<()> {
        let node = self
            .get(root)
            .ok_or_else(|| anyhow!("invalid attribute root"))?;
        ensure!(node.parent.is_none(), "release requires an attribute root");
        self.remove_subtree(root);
        Ok(())
    }
    fn remove_subtree(&mut self, root: AttributeId) {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if let Some(node) = self.nodes.remove(&id) {
                if node.break_enabled {
                    self.break_notifications = self.break_notifications.saturating_add(1);
                }
                stack.extend(node.children);
            }
        }
    }
}

fn validate_name(name: &str) -> Result<()> {
    ensure!(
        name.len() <= MAX_NAME && name.is_ascii() && !name.contains('\0'),
        "attribute names require at most 1023 ASCII bytes without NUL"
    );
    Ok(())
}
fn validate_path(path: &str) -> Result<Vec<&str>> {
    ensure!(
        path.len() <= MAX_NAME * MAX_DEPTH,
        "attribute path too long"
    );
    let parts = path.split('.').collect::<Vec<_>>();
    ensure!(parts.len() <= MAX_DEPTH, "attribute path too deep");
    for part in &parts {
        validate_name(part)?;
    }
    Ok(parts)
}

#[cfg(test)]
mod tests;
