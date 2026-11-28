//! Load the original coast profiles and publish their wave-projected strips.
use crate::{GameDirectory, NativeHost, config::ini_section, random::Random, sea::Seas};
use anyhow::{Context, Result, ensure};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, str::FromStr};
use storm_core::{Entity, EntityDestroy, EntityId, EntityInit, Stage};
use storm_formats::texture::Texture;
use storm_render::{model::CoastFoamDraw, texture_paths::TexturePaths};
use storm_sim::coast_foam::{self, Kind, Parameters, Strip};

const MAX_PROFILE_BYTES: usize = 2 * 1024 * 1024;
const MAX_STRIPS: usize = 1024;
type Fields<'a> = BTreeMap<String, &'a str>;

#[derive(Clone, Default)]
pub(crate) struct CoastFoams(Rc<RefCell<Vec<(EntityId, CoastFoamDraw)>>>);

impl CoastFoams {
    pub(crate) fn register(host: &mut NativeHost, game: GameDirectory) -> Result<()> {
        let foams = Self::default();
        let draws = foams.clone();
        let seas = host.seas.clone();
        let random = host.random.clone();
        let paths = host.texture_paths.clone();
        if host.entities.register_class("CoastFoam", move || {
            Ok(Box::new(Actor {
                foams: draws.clone(),
                game: game.clone(),
                seas: seas.clone(),
                paths: paths.clone(),
                random: random.clone(),
                id: EntityId::INVALID,
                profile: None,
            }))
        })? {
            host.coast_foams = Some(foams);
        }
        Ok(())
    }

    pub(crate) fn begin_frame(&self) {
        self.0.borrow_mut().clear();
    }

    pub(crate) fn draws(&self) -> Vec<CoastFoamDraw> {
        self.0
            .borrow()
            .iter()
            .map(|(_, draw)| draw.clone())
            .collect()
    }
}

struct Actor {
    foams: CoastFoams,
    game: GameDirectory,
    seas: Option<Seas>,
    paths: Rc<RefCell<TexturePaths>>,
    random: Rc<RefCell<Random>>,
    id: EntityId,
    profile: Option<Profile>,
}

impl Entity for Actor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        self.id = context.id;
        if let Some(name) = context
            .attributes
            .and_then(|root| context.store.value(root, Some("id")))
            .filter(|name| !name.is_empty())
        {
            ensure!(
                !name.contains(['/', '\\', ':']) && name.len() <= 255,
                "invalid CoastFoam location id"
            );
            let path = format!("resource/foam/locations/{name}.ini");
            if let Some(bytes) = self.game.read_optional(&path, MAX_PROFILE_BYTES)? {
                self.profile = Some(
                    Profile::load(
                        &String::from_utf8_lossy(&bytes),
                        &mut self.random.borrow_mut(),
                        |name| self.game.load_texture(&self.paths.borrow(), name),
                    )
                    .with_context(|| format!("load CoastFoam profile {path}"))?,
                );
            }
        }
        Ok(true)
    }

    fn process_stage_checked(&mut self, stage: Stage, delta_ms: u32) -> Result<()> {
        if stage != Stage::Realize {
            return Ok(());
        }
        let (Some(profile), Some(_)) = (
            self.profile.as_mut(),
            self.seas.as_ref().and_then(Seas::draw),
        ) else {
            return Ok(());
        };
        let mut random = self.random.borrow_mut();
        let mut draws = self.foams.0.borrow_mut();
        for (strip, texture) in &mut profile.strips {
            strip.advance(delta_ms, &mut *random);
            let Some(texture) = texture else { continue };
            draws.push((
                self.id,
                CoastFoamDraw {
                    texture: texture.clone(),
                    strip: strip.clone(),
                    height: profile.height,
                    max_distance: profile.distance,
                },
            ));
        }
        Ok(())
    }

    fn destroy(&mut self, _: &mut EntityDestroy<'_>) -> Result<()> {
        self.foams.0.borrow_mut().retain(|(id, _)| *id != self.id);
        Ok(())
    }
}

struct Profile {
    height: f32,
    distance: f32,
    strips: Vec<(Strip, Option<Rc<Texture>>)>,
}

impl Profile {
    fn load(
        text: &str,
        random: &mut Random,
        mut texture: impl FnMut(&str) -> Result<Option<Texture>>,
    ) -> Result<Self> {
        let mut sections: BTreeMap<String, Fields<'_>> = BTreeMap::new();
        let mut section = sections.entry(String::new()).or_default();
        for line in text
            .trim_start_matches('\u{feff}')
            .lines()
            .map(str::trim_ascii)
        {
            if let Some(name) = ini_section(line) {
                section = sections.entry(name.to_ascii_lowercase()).or_default();
            } else if !line.starts_with(';')
                && let Some((key, value)) = line.split_once('=')
            {
                section
                    .entry(key.trim_ascii().to_ascii_lowercase())
                    .or_insert_with(|| value.trim_ascii());
            }
        }
        let root = &sections[""];
        let count = field(root, "numfoams", 0_i32)?.max(0) as usize;
        let height = field(root, "foamdeltay", 0.2_f32)?;
        let distance = field(root, "maxfoamdistance", 1000.0_f32)?;
        let divisions = field(root, "foamdivides", 4_usize)?;
        ensure!(
            count <= MAX_STRIPS && height.is_finite() && distance.is_finite(),
            "invalid CoastFoam profile limits"
        );
        let mut strips = Vec::new();
        let mut budget = 0;
        let mut textures = BTreeMap::new();
        for index in 0..count {
            let Some(fields) = sections.get(&format!("foam_{index}")) else {
                coast_foam::skip_initialization(random);
                continue;
            };
            let parameters = Parameters {
                kind: if field(fields, "type", 2_i32)? == 1 {
                    Kind::Type1
                } else {
                    Kind::Type2
                },
                pulses: if field(fields, "numfoams", 2_i32)? == 2 {
                    2
                } else {
                    1
                },
                alpha: numbers(fields, "alpha", [148.0, 196.0])?,
                speed: numbers(fields, "speed", [0.2, 0.3])?,
                appear: numbers(fields, "appear", [0.0, 0.2])?,
                texture_scale: field(fields, "texscalex", 0.05)?,
            };
            let count = field(fields, "numparts", 0_i32)?;
            let mut keys = Vec::new();
            for index in 0..if count == 0 {
                coast_foam::MAX_COLUMNS + 1
            } else {
                (count.max(0) as usize).min(coast_foam::MAX_COLUMNS + 1)
            } {
                let key = format!("key_{index}");
                if fields.get(&key).is_none_or(|value| value.is_empty()) {
                    break;
                }
                let [x1, z1, x2, z2] = numbers(fields, &key, [0.0; 4])?;
                keys.push([[x1, z1], [x2, z2]]);
            }
            let Some(mut strip) = Strip::new(&keys, divisions, parameters)? else {
                coast_foam::skip_initialization(random);
                continue;
            };
            budget += strip.vertex_budget();
            ensure!(
                budget <= coast_foam::MAX_DRAW_VERTICES,
                "CoastFoam profile vertex budget"
            );
            strip.initialize(random);
            let name = fields.get("texture").copied().unwrap_or("foam.tga");
            let texture = match textures.entry(name.to_ascii_lowercase()) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(texture(&format!("weather/coastfoam/{name}"))?.map(Rc::new))
                }
            };
            strips.push((strip, texture.clone()));
        }
        Ok(Self {
            height,
            distance,
            strips,
        })
    }
}

fn field<T: FromStr>(fields: &Fields<'_>, key: &str, default: T) -> Result<T> {
    fields.get(key).map_or(Ok(default), |value| {
        value
            .parse()
            .ok()
            .with_context(|| format!("invalid CoastFoam {key}: {value}"))
    })
}

fn numbers<const N: usize>(
    fields: &Fields<'_>,
    key: &str,
    mut values: [f32; N],
) -> Result<[f32; N]> {
    if let Some(text) = fields.get(key) {
        let mut tokens = text.split(',');
        for value in &mut values {
            *value = tokens
                .next()
                .and_then(|token| token.trim_ascii().parse().ok())
                .with_context(|| format!("invalid CoastFoam {key}: {text}"))?;
        }
        ensure!(tokens.next().is_none(), "invalid CoastFoam {key}: {text}");
    }
    Ok(values)
}

impl coast_foam::Random for Random {
    fn scaled(&mut self, maximum: f32) -> f32 {
        Random::range(self, 0.0, maximum)
    }

    fn range(&mut self, minimum: f32, maximum: f32) -> f32 {
        Random::range(self, minimum, maximum)
    }
}
