//! Retained rigging deletion/layout and buffer intents. Geometry fill is a port.
use anyhow::{Result, ensure};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Sail,
    Rope,
    Flag,
    Vant,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub owner: u64,
    pub deleted: bool,
    pub members: Vec<usize>,
    /// Native group sv/nv/st/nt (VANT sVert/nVert/sIndx/nIndx).
    pub span: Option<[u32; 4]>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: u32,
    pub group: usize,
    pub deleted: bool,
    pub vertices: u32,
    pub triangles: u32,
    pub start_vertex: u32,
    pub start_index: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BufferEvent {
    ReleaseVertex(i32),
    ReleaseIndex(i32),
    CreateVertex { id: i32, bytes: u32, dynamic: bool },
    CreateIndex { id: i32, bytes: u32 },
    Lock(i32),
    Unlock(i32),
    FillSail { id: u32, start: u32 },
    TextureSail { id: u32, start: u32 },
    SetVertices,
    SetIndices,
    SetTriangles,
    SetTexture,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Buffers {
    pub vertex: i32,
    pub index: i32,
    pub next_id: i32,
    pub lock_succeeds: bool,
    pub events: Vec<BufferEvent>,
}
impl Buffers {
    pub fn take_events(&mut self) -> Vec<BufferEvent> {
        std::mem::take(&mut self.events)
    }
    fn release_vertex(&mut self) {
        if self.vertex != -1 {
            self.events.push(BufferEvent::ReleaseVertex(self.vertex));
            self.vertex = -1;
        }
    }
    fn release_index(&mut self) {
        if self.index != -1 {
            self.events.push(BufferEvent::ReleaseIndex(self.index));
            self.index = -1;
        }
    }
    fn allocate(&mut self) -> Result<i32> {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("buffer handle overflow"))?;
        Ok(id)
    }
    fn create_vertex(&mut self, count: u32, stride: u32, dynamic: bool) -> Result<()> {
        let bytes = count
            .checked_mul(stride)
            .ok_or_else(|| anyhow::anyhow!("vertex byte overflow"))?;
        self.vertex = self.allocate()?;
        self.events.push(BufferEvent::CreateVertex {
            id: self.vertex,
            bytes,
            dynamic,
        });
        Ok(())
    }
    fn create_index(&mut self, triangles: u32) -> Result<()> {
        let bytes = triangles
            .checked_mul(6)
            .ok_or_else(|| anyhow::anyhow!("index byte overflow"))?;
        self.index = self.allocate()?;
        self.events.push(BufferEvent::CreateIndex {
            id: self.index,
            bytes,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub kind: Kind,
    pub groups: Vec<Group>,
    /// Only native FLAG accepts null entries in the old pointer list.
    pub items: Vec<Option<Item>>,
    pub vertices: u32,
    pub triangles: Option<u32>,
    pub active: bool,
    pub pending: bool,
    pub last: u32,
    pub vertices_need_update: bool,
    pub buffers: Buffers,
}
impl Layout {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.groups.len() <= 4096 && self.items.len() <= 65536,
            "rigging allocation budget"
        );
        ensure!(
            self.buffers.vertex >= -1 && self.buffers.index >= -1 && self.buffers.next_id >= 0,
            "invalid buffer handles"
        );
        let mut seen = std::collections::BTreeSet::new();
        for item in &self.items {
            let Some(item) = item else {
                ensure!(self.kind == Kind::Flag, "null rigging item");
                continue;
            };
            ensure!(
                item.group < self.groups.len() && seen.insert(item.id),
                "invalid rigging owner or repeated item"
            );
            ensure!(
                item.vertices <= 1_000_000 && item.triangles <= 1_000_000,
                "rigging item budget"
            );
        }
        if matches!(self.kind, Kind::Rope | Kind::Vant) {
            let mut membership = std::collections::BTreeSet::new();
            for (group, g) in self.groups.iter().enumerate() {
                ensure!(g.span.is_some(), "missing native group span");
                for &index in &g.members {
                    let item = self
                        .items
                        .get(index)
                        .and_then(Option::as_ref)
                        .ok_or_else(|| anyhow::anyhow!("invalid rigging member"))?;
                    ensure!(
                        item.group == group && membership.insert(index),
                        "inconsistent rigging membership"
                    );
                }
            }
            ensure!(
                membership.len() == self.items.len(),
                "unlisted rigging item"
            );
        }
        ensure!(
            (self.kind == Kind::Sail) == self.triangles.is_none(),
            "invalid native index-total contract"
        );
        Ok(())
    }
    /// Checked metadata is committed together; external GPU work consumes the
    /// resulting intents separately and is not covered by this transaction.
    pub fn compact(&mut self) -> Result<()> {
        self.validate()?;
        let mut next = self.clone();
        next.compact_inner()?;
        next.validate()?;
        *self = next;
        Ok(())
    }
    pub fn execute(&mut self) -> Result<bool> {
        if !self.pending {
            return Ok(false);
        }
        self.compact()?;
        Ok(true)
    }
    fn compact_inner(&mut self) -> Result<()> {
        let old_vertices = self.vertices;
        let old_items = self.items.len();
        let old_groups = self.groups.len();
        match self.kind {
            Kind::Sail => self.sails()?,
            Kind::Rope => self.ropes()?,
            Kind::Flag => self.flags()?,
            Kind::Vant => self.vants()?,
        }
        let n = self.items.len();
        let g = self.groups.len();
        let b = &mut self.buffers;
        match self.kind {
            Kind::Sail => {
                b.release_vertex();
                if self.vertices > 0 {
                    b.create_vertex(self.vertices, 48, true)?;
                    b.events.push(BufferEvent::Lock(b.vertex));
                    if b.lock_succeeds {
                        for i in self.items.iter().flatten() {
                            b.events.push(BufferEvent::FillSail {
                                id: i.id,
                                start: i.start_vertex,
                            });
                            b.events.push(BufferEvent::TextureSail {
                                id: i.id,
                                start: i.start_vertex,
                            });
                        }
                        b.events.push(BufferEvent::Unlock(b.vertex));
                    }
                }
            }
            Kind::Rope => {
                if old_vertices != self.vertices {
                    b.release_vertex();
                    b.release_index();
                    let triangles = self.triangles.unwrap();
                    if self.vertices > 0 {
                        b.create_vertex(self.vertices, 20, false)?;
                    }
                    if triangles > 0 {
                        b.create_index(triangles)?;
                    }
                    if self.vertices > 0 && triangles > 0 {
                        b.events.push(BufferEvent::SetVertices);
                        b.events.push(BufferEvent::SetIndices);
                    }
                }
            }
            Kind::Flag | Kind::Vant => {
                if n == 0 || g == 0 {
                    b.release_vertex();
                    b.release_index();
                    self.items.clear();
                    self.groups.clear();
                } else if n != old_items || g != old_groups {
                    b.release_vertex();
                    b.release_index();
                    b.create_vertex(self.vertices, 20, false)?;
                    b.create_index(self.triangles.unwrap())?;
                    if self.kind == Kind::Flag {
                        b.events.push(BufferEvent::SetTriangles);
                        self.vertices_need_update = true;
                        b.events.push(BufferEvent::SetTexture);
                    } else {
                        b.events.push(BufferEvent::SetVertices);
                        b.events.push(BufferEvent::SetIndices);
                    }
                }
            }
        }
        self.last = u32::try_from(self.items.len())?;
        self.pending = false;
        self.active = !self.items.is_empty();
        Ok(())
    }
    fn add(total: &mut u32, value: u32) -> Result<()> {
        *total = total
            .checked_add(value)
            .filter(|&n| n <= i32::MAX as u32)
            .ok_or_else(|| anyhow::anyhow!("rigging offset overflow"))?;
        Ok(())
    }
    fn sails(&mut self) -> Result<()> {
        let old = std::mem::take(&mut self.items);
        let mut items = Vec::new();
        let mut groups = Vec::new();
        let mut vertices = 0;
        for (gn, group) in self.groups.iter().enumerate() {
            if group.deleted {
                continue;
            }
            let mut g = group.clone();
            g.members.clear();
            for item in old.iter().flatten().filter(|i| i.group == gn && !i.deleted) {
                let mut i = item.clone();
                i.group = groups.len();
                i.start_vertex = vertices;
                i.start_index = 54;
                Self::add(&mut vertices, i.vertices)?;
                g.members.push(items.len());
                items.push(Some(i));
            }
            if !g.members.is_empty() {
                groups.push(g);
            }
        }
        self.items = items;
        self.groups = groups;
        self.vertices = vertices;
        Ok(())
    }
    fn ropes(&mut self) -> Result<()> {
        self.items.retain(|i| {
            i.as_ref()
                .is_some_and(|i| !i.deleted && !self.groups[i.group].deleted)
        });
        let mut groups = Vec::new();
        let mut vertices = 0;
        let mut indices = 0;
        for (gn, group) in self.groups.iter().enumerate() {
            if group.deleted {
                continue;
            }
            let mut g = group.clone();
            g.members.clear();
            let sv = vertices;
            let st = indices;
            let mut nv = 0;
            let mut nt = 0;
            for (index, item) in self.items.iter_mut().enumerate() {
                let i = item.as_mut().unwrap();
                if i.group != gn {
                    continue;
                }
                i.group = groups.len();
                i.start_vertex = vertices;
                i.start_index = indices;
                g.members.push(index);
                Self::add(&mut vertices, i.vertices)?;
                Self::add(
                    &mut indices,
                    i.triangles
                        .checked_mul(3)
                        .ok_or_else(|| anyhow::anyhow!("rigging index overflow"))?,
                )?;
                Self::add(&mut nv, i.vertices)?;
                Self::add(&mut nt, i.triangles)?;
            }
            if !g.members.is_empty() {
                g.span = Some([sv, nv, st, nt]);
                groups.push(g);
            }
        }
        self.groups = groups;
        self.vertices = vertices;
        self.triangles = Some(indices / 3);
        Ok(())
    }
    fn flags(&mut self) -> Result<()> {
        let mut groups = Vec::new();
        let mut vertices = 0;
        let mut indices = 0;
        for (gn, group) in self.groups.iter().enumerate() {
            let mut size = 0;
            for item in self.items.iter_mut().flatten().filter(|i| i.group == gn) {
                if group.deleted || item.deleted || item.vertices == 0 || item.triangles == 0 {
                    item.deleted = true;
                    continue;
                }
                item.group = groups.len();
                item.start_vertex = vertices;
                item.start_index = indices;
                Self::add(&mut vertices, item.vertices)?;
                Self::add(
                    &mut indices,
                    item.triangles
                        .checked_mul(3)
                        .ok_or_else(|| anyhow::anyhow!("rigging index overflow"))?,
                )?;
                size += 1;
            }
            if size > 0 {
                groups.push(group.clone());
            }
        }
        self.items
            .retain(|i| i.as_ref().is_some_and(|i| !i.deleted));
        self.groups = groups;
        self.vertices = vertices;
        self.triangles = Some(indices / 3);
        Ok(())
    }
    fn vants(&mut self) -> Result<()> {
        let mut items = Vec::new();
        let mut groups = Vec::new();
        let mut vertices = 0;
        let mut indices = 0;
        for group in &self.groups {
            let mut g = group.clone();
            g.members.clear();
            let sv = vertices;
            let st = indices;
            let mut nv = 0;
            let mut nt = 0;
            for &index in &group.members {
                let old = self.items[index].as_ref().unwrap();
                if group.deleted || old.deleted {
                    continue;
                }
                let mut i = old.clone();
                i.group = groups.len();
                i.start_vertex = vertices;
                i.start_index = indices;
                Self::add(&mut nv, i.vertices)?;
                Self::add(&mut nt, i.triangles)?;
                Self::add(&mut vertices, i.vertices)?;
                Self::add(
                    &mut indices,
                    i.triangles
                        .checked_mul(3)
                        .ok_or_else(|| anyhow::anyhow!("rigging index overflow"))?,
                )?;
                g.members.push(items.len());
                items.push(Some(i));
            }
            if !g.members.is_empty() {
                g.span = Some([sv, nv, st, nt]);
                groups.push(g);
            }
        }
        self.groups = groups;
        self.items = items;
        self.vertices = vertices;
        self.triangles = Some(indices / 3);
        Ok(())
    }
}
