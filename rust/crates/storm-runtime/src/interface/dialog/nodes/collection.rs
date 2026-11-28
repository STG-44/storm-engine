use super::{
    Atlas, InterfaceScreen, NEUTRAL, Nodes, Setting, Shared, color, ini_color, ini_integer, tuple,
};
use anyhow::{Result, bail, ensure};
use storm_core::Message;
use storm_render::interface_text::{InterfaceDraw, PictureColor, PictureDraw};

const MAX_IMAGES: usize = 1024;

pub(super) struct Collection {
    images: Vec<Image>,
    group: String,
    atlas: Atlas,
    offset: [i32; 2],
    origin: [i32; 2],
}

struct Image {
    name: String,
    rect: [i32; 4],
    uv: [f32; 4],
    rgba: [u8; 4],
}

impl Collection {
    pub(super) fn load<'a>(
        nodes: &mut Nodes,
        shared: &Shared,
        setting: &Setting<'_>,
        screen: &InterfaceScreen,
        pictures: impl Iterator<Item = &'a str>,
    ) -> Result<Self> {
        let group = setting("groupName").unwrap_or_default().to_owned();
        let atlas = if group.is_empty() {
            Atlas::default()
        } else {
            nodes.atlas(shared, &group)?
        };
        let offset = setting("offset").map_or(Ok([0; 2]), tuple)?;
        let absolute = setting("bAbsoluteRectangle").map_or(Ok(0), ini_integer)? != 0;
        let origin = if absolute {
            [0; 2]
        } else {
            [screen.rect[0], screen.rect[1]]
        };
        let mut collection = Self {
            images: Vec::new(),
            group,
            atlas,
            offset,
            origin,
        };
        for picture in pictures {
            let (name, fields) = picture.split_once(',').unwrap_or((picture, ""));
            let name = name.trim_ascii();
            // Edit sections group existing pictures in the interface editor.
            if name
                .get(..12)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("editsection:"))
            {
                continue;
            }
            let rgba = ini_color(tag(fields, "col:{"), NEUTRAL)?;
            let rect = tag(fields, "pos:{").map_or(Ok([0; 4]), tuple)?;
            collection.push(name.to_owned(), rect, rgba)?;
        }
        Ok(collection)
    }

    pub(super) fn group(&self) -> &str {
        &self.group
    }

    pub(super) fn set_atlas(&mut self, group: String, atlas: Atlas) {
        self.group = group;
        self.atlas = atlas;
        // The native message swaps the bound texture without rebuilding UVs.
    }

    pub(super) fn send(&mut self, code: i32, message: &mut Message) -> Result<()> {
        match code {
            0 => {
                let name = message.string()?.to_owned();
                let rgba = color(message.long()?);
                let rect = [
                    message.long()?,
                    message.long()?,
                    message.long()?,
                    message.long()?,
                ];
                self.push(name, translated(rect, self.offset), rgba)?;
                self.update_uvs();
            }
            2 => self.images.clear(),
            3 | 4 => {
                let index = message.long()? as usize;
                if code == 3 {
                    let rgba = color(message.long()?);
                    if let Some(image) = self.images.get_mut(index) {
                        image.rgba = rgba;
                        self.update_uvs();
                    }
                } else {
                    let name = message.string()?;
                    if let Some(image) = self.images.get_mut(index) {
                        image.name = name.to_owned();
                        self.update_uvs();
                    }
                }
            }
            _ => bail!("unported XINTERFACE image-collection message {code}"),
        }
        Ok(())
    }

    fn push(&mut self, name: String, rect: [i32; 4], rgba: [u8; 4]) -> Result<()> {
        ensure!(
            self.images.len() < MAX_IMAGES,
            "interface image collection budget"
        );
        self.images.push(Image {
            uv: self.atlas.part(Some(&name)).0,
            name,
            rect,
            rgba,
        });
        Ok(())
    }

    fn update_uvs(&mut self) {
        for image in &mut self.images {
            image.uv = self.atlas.part(Some(&image.name)).0;
        }
    }

    pub(super) fn draw(&self, canvas: [i32; 2], out: &mut Vec<InterfaceDraw>) {
        if let Some(texture) = &self.atlas.texture {
            out.extend(self.images.iter().map(|image| {
                InterfaceDraw::Picture(PictureDraw {
                    texture: texture.clone(),
                    canvas,
                    rect: translated(image.rect, self.origin).map(|value| value as f32),
                    uv: image.uv,
                    rgba: image.rgba,
                    color: PictureColor::Modulate,
                })
            }));
        }
    }
}

fn tag<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.split_once(prefix)?
        .1
        .split_once('}')
        .map(|(value, _)| value)
}

fn translated([left, top, right, bottom]: [i32; 4], [x, y]: [i32; 2]) -> [i32; 4] {
    [
        left.saturating_add(x),
        top.saturating_add(y),
        right.saturating_add(x),
        bottom.saturating_add(y),
    ]
}
