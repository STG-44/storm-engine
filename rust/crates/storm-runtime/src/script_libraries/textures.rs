use crate::{GameDirectory, host::NativeFunction};
use anyhow::{Context, Result, bail, ensure};
use std::{cell::RefCell, rc::Rc};
use storm_formats::texture::Texture;
use storm_render::texture_paths::TexturePaths;
use storm_script::Value;

pub(crate) const RENDERER_LIBRARY: &str = "dx9render_script_libriary";
const MAX_SLOTS: usize = 10240; // MAX_STEXTURES in the original renderer.
const MAX_BYTES: usize = 512 * 1024 * 1024;

struct Slot {
    key: String,
    references: u32,
    texture: Texture,
}

struct Textures {
    game: GameDirectory,
    slots: Vec<Option<Slot>>,
    decoded_bytes: usize,
}

impl Textures {
    fn acquire(&mut self, paths: &TexturePaths, name: &str) -> Result<i32> {
        for candidate in paths.candidates(name)? {
            if let Some((id, slot)) = self.slots.iter_mut().enumerate().find_map(|(id, slot)| {
                slot.as_mut()
                    .filter(|slot| slot.key == candidate.key)
                    .map(|slot| (id, slot))
            }) {
                slot.references = slot
                    .references
                    .checked_add(1)
                    .context("native texture reference count overflow")?;
                return Ok(id as i32);
            }
            let path = format!("RESOURCE/Textures/{}", candidate.file);
            let Some(bytes) = self.game.read_optional(&path, 256 * 1024 * 1024)? else {
                continue;
            };
            let Ok(texture) = Texture::read(&bytes) else {
                continue;
            };
            let id = self
                .slots
                .iter()
                .position(Option::is_none)
                .unwrap_or(self.slots.len());
            ensure!(id < MAX_SLOTS, "native texture slots exhausted");
            let reserved = self.decoded_bytes + texture_size(&texture);
            ensure!(reserved <= MAX_BYTES, "native texture memory exhausted");
            let slot = Some(Slot {
                key: candidate.key,
                references: 1,
                texture,
            });
            if id == self.slots.len() {
                self.slots.push(slot);
            } else {
                self.slots[id] = slot;
            }
            self.decoded_bytes = reserved;
            return Ok(id as i32);
        }
        Ok(-1)
    }

    fn release(&mut self, id: i32) -> Result<()> {
        if id == -1 {
            return Ok(());
        }
        let slot = usize::try_from(id)
            .ok()
            .and_then(|id| self.slots.get_mut(id))
            .context("invalid native texture ID")?;
        let texture = slot.as_mut().context("released native texture ID")?;
        texture.references -= 1;
        if texture.references == 0 {
            self.decoded_bytes -= texture_size(&texture.texture);
            *slot = None;
        }
        Ok(())
    }
}

fn texture_size(texture: &Texture) -> usize {
    texture.mips.iter().map(|mip| mip.rgba.len()).sum()
}

pub(crate) fn functions(
    game: GameDirectory,
    paths: Rc<RefCell<TexturePaths>>,
) -> [(&'static str, NativeFunction); 3] {
    let textures = Rc::new(RefCell::new(Textures {
        game,
        slots: Vec::new(),
        decoded_bytes: 0,
    }));
    let set_paths = paths.clone();
    let get_textures = textures.clone();
    [
        (
            "settexturepath",
            NativeFunction::new(move |args| {
                let [Value::Int(level), Value::String(prefix)] = args else {
                    bail!("SetTexturePath expects a level and path");
                };
                let changed = set_paths
                    .try_borrow_mut()
                    .context("reentrant renderer texture paths")?
                    .set(*level, prefix)?;
                Ok(Value::Int(i32::from(changed)))
            }),
        ),
        (
            "gettexture",
            NativeFunction::new(move |args| {
                let [Value::String(name)] = args else {
                    bail!("GetTexture expects a file name");
                };
                let paths = paths
                    .try_borrow()
                    .context("reentrant renderer texture paths")?;
                let id = get_textures
                    .try_borrow_mut()
                    .context("reentrant native texture cache")?
                    .acquire(&paths, name)?;
                Ok(Value::Int(id))
            }),
        ),
        (
            "releasetexture",
            NativeFunction::new(move |args| {
                let [Value::Int(id)] = args else {
                    bail!("ReleaseTexture expects a texture ID");
                };
                textures
                    .try_borrow_mut()
                    .context("reentrant native texture cache")?
                    .release(*id)?;
                Ok(Value::Void)
            }),
        ),
    ]
}
