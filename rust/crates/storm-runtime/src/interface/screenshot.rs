use super::{Shared, load_texture};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, fs::File, rc::Rc};
use storm_core::{Entity, EntityDestroy, EntityId, EntityInit, Message, ScriptScalar};
use storm_formats::{
    save_thumbnail::{self, RGBA_BYTES, SIDE},
    texture::Mip,
};
use storm_render::Frame;

const MSG_SCRSHOT_MAKE: i32 = 45042;
const MSG_SCRSHOT_READ: i32 = 45043;
const MSG_SCRSHOT_RELEASE: i32 = 45044;
const MAX_IMAGES: usize = 1024;
const BORDER: &str = "interfaces\\EmptyBorder.tga";

#[derive(Default)]
pub(super) struct Screenshots {
    captures: BTreeMap<u64, Option<i32>>,
    images: BTreeMap<i32, Rc<Mip>>,
    next: i32,
}

impl Screenshots {
    pub(super) fn pending(&self, entity: EntityId) -> bool {
        self.captures.get(&entity.raw()) == Some(&None)
    }

    pub(super) fn image(&self, handle: i32) -> Option<Rc<Mip>> {
        self.images.get(&handle).cloned()
    }

    pub(super) fn complete(&mut self, entity: EntityId, image: Mip) -> Result<()> {
        ensure!(self.pending(entity), "unexpected screenshot completion");
        let handle = self.insert(image)?;
        self.captures.insert(entity.raw(), Some(handle));
        Ok(())
    }

    fn insert(&mut self, image: Mip) -> Result<i32> {
        ensure!(self.images.len() < MAX_IMAGES, "screenshot texture limit");
        let handle = self.next;
        self.next = handle
            .checked_add(1)
            .context("screenshot handle overflow")?;
        self.images.insert(handle, Rc::new(image));
        Ok(handle)
    }
}

pub(super) fn finish(shared: &Shared, entity: EntityId, frame: Frame) -> Result<()> {
    ensure!(
        frame.width == SIDE && frame.height == SIDE && frame.rgba.len() == RGBA_BYTES,
        "invalid screenshot dimensions"
    );
    let mut image = Mip {
        width: SIDE,
        height: SIDE,
        rgba: frame.rgba,
    };
    for pixel in image.rgba.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
    if let Some(texture) = load_texture(shared, BORDER)?
        && let Some(border) = texture.mips.first()
    {
        composite_border(&mut image, border);
    }
    shared.screenshots.borrow_mut().complete(entity, image)
}

fn composite_border(image: &mut Mip, border: &Mip) {
    for y in 0..SIDE {
        for x in 0..SIDE {
            let by = y * border.height / SIDE;
            let bx = x * border.width / SIDE;
            let src = ((by * border.width + bx) * 4) as usize;
            let dst = ((y * SIDE + x) * 4) as usize;
            let alpha = u32::from(border.rgba[src + 3]);
            for (dst, &src) in image.rgba[dst..dst + 4]
                .iter_mut()
                .zip(&border.rgba[src..src + 4])
            {
                // battle_icons uses SrcAlpha and InvSrcAlpha in the back-buffer color space.
                *dst =
                    ((u32::from(src) * alpha + u32::from(*dst) * (255 - alpha) + 127) / 255) as u8;
            }
        }
    }
}

pub(super) struct Actor {
    shared: Rc<Shared>,
    id: EntityId,
    saves: BTreeMap<String, (i32, String)>,
}

impl Actor {
    pub(super) fn new(shared: Rc<Shared>) -> Self {
        Self {
            shared,
            id: EntityId::INVALID,
            saves: BTreeMap::new(),
        }
    }

    fn live_handle(&self) -> i32 {
        self.shared.screenshots.borrow().captures[&self.id.raw()].unwrap_or(-1)
    }

    fn open(&mut self, directory: &str, file: &str) -> Result<(i32, String)> {
        if file.is_empty() {
            return Ok((-1, String::new()));
        }
        if file.eq_ignore_ascii_case("newsave") {
            return Ok((self.live_handle(), String::new()));
        }
        let key = file.to_ascii_lowercase();
        if let Some(preview) = self.saves.get(&key) {
            return Ok(preview.clone());
        }
        let path = if directory.is_empty() {
            file.to_owned()
        } else {
            format!("{directory}/{file}")
        };
        let Some(path) = self.shared.game.resolve_optional(&path)? else {
            return Ok((-1, String::new()));
        };
        if !path.is_file() {
            return Ok((-1, String::new()));
        }
        let Ok(preview) = save_thumbnail::read(&mut File::open(path)?) else {
            return Ok((-1, String::new()));
        };
        let handle = self.shared.screenshots.borrow_mut().insert(preview.image)?;
        let entry = (handle, preview.description);
        self.saves.insert(key, entry.clone());
        Ok(entry)
    }
}

impl Entity for Actor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        self.id = context.id;
        self.shared
            .screenshots
            .borrow_mut()
            .captures
            .insert(self.id.raw(), None);
        Ok(true)
    }

    fn destroy(&mut self, _: &mut EntityDestroy<'_>) -> Result<()> {
        let mut screenshots = self.shared.screenshots.borrow_mut();
        if let Some(Some(handle)) = screenshots.captures.remove(&self.id.raw()) {
            screenshots.images.remove(&handle);
        }
        for (handle, _) in self.saves.values() {
            screenshots.images.remove(handle);
        }
        Ok(())
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        match message.long()? {
            MSG_SCRSHOT_MAKE => Ok(self.live_handle() as u64),
            MSG_SCRSHOT_READ => {
                let directory = message.string()?.to_owned();
                let file = message.string()?.to_owned();
                let output = message.script_variable()?;
                let (handle, description) = self.open(&directory, &file)?;
                output.set(ScriptScalar::String(description))?;
                Ok(handle as u64)
            }
            MSG_SCRSHOT_RELEASE => {
                if let Some((handle, _)) =
                    self.saves.remove(&message.string()?.to_ascii_lowercase())
                {
                    self.shared.screenshots.borrow_mut().images.remove(&handle);
                }
                Ok(0)
            }
            _ => Ok(0),
        }
    }
}
