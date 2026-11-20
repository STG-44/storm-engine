//! Script-owned ShipLights catalog and native locator/oscillator state.
use crate::{GameDirectory, NativeHost, random::Random};
use anyhow::{Context, Result, ensure};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
use storm_core::{
    AttributeId, AttributeStore, Entity, EntityDestroy, EntityId, EntityInit, Message, Stage,
};
use storm_formats::{geometry::Label, texture::Texture};
use storm_render::texture_paths::TexturePaths;
use storm_script::Host;
use storm_sim::sea::camera::{Matrix, transpose};

const MSG_SEA_SUNROAD_DRAW: i32 = 50201;

pub struct ShipLightPose<'a> {
    pub root: &'a Matrix,
    /// Current global frames, keyed by the native lowercase node name.
    pub nodes: &'a BTreeMap<String, Matrix>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointLight {
    pub position: [f32; 3],
    pub rgb: [f32; 3],
    pub range: f32,
    pub attenuation: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corona {
    pub position: [f32; 3],
    pub rgb: [f32; 3],
    pub size: f32,
    pub sun_road_fade_distance: f32,
}

#[derive(Clone, Copy)]
struct LightType {
    rgb: [f32; 3],
    range: f32,
    attenuation: [f32; 3],
    corona: [f32; 3],
    corona_size: f32,
    sun_road_fade_distance: f32,
    oscillators: [[f32; 2]; 2],
}

struct Catalog {
    types: BTreeMap<String, LightType>,
    textures: [Option<Rc<Texture>>; 2],
    max_lights: usize,
}

impl Catalog {
    fn load(
        store: &AttributeStore,
        root: Option<AttributeId>,
        game: &GameDirectory,
        paths: &TexturePaths,
    ) -> Result<Option<Self>> {
        let Some(root) = root else { return Ok(None) };
        let Some(types) = store.child(root, "LightTypes").and_then(|id| store.get(id)) else {
            return Ok(None);
        };
        let mut parsed = BTreeMap::new();
        for &kind in types.children() {
            fn floats<const N: usize>(
                store: &AttributeStore,
                root: AttributeId,
                keys: [(&str, f32); N],
            ) -> Result<[f32; N]> {
                let mut values = [0.0; N];
                for (target, (key, default)) in values.iter_mut().zip(keys) {
                    *target = store
                        .lookup_path(root, key)?
                        .map_or(default, |id| store.as_float(id, None, 0.0));
                }
                ensure!(
                    values.into_iter().all(f32::is_finite),
                    "non-finite ship light setting"
                );
                Ok(values)
            }
            let read_color = |keys| floats(store, kind, keys);
            let [range, corona_size, sun_road_fade_distance] = floats(
                store,
                kind,
                [
                    ("light.range", 10.0),
                    ("corona.Size", 1.0),
                    ("SunRoadFlareFadeDistance", 100.0),
                ],
            )?;
            parsed.insert(
                store.name(kind).context("unnamed ship light type")?.into(),
                LightType {
                    rgb: read_color([("light.r", 1.0), ("light.g", 1.0), ("light.b", 1.0)])?,
                    range,
                    attenuation: read_color([
                        ("light.Attenuation0", 1.0),
                        ("light.Attenuation1", 0.0),
                        ("light.Attenuation2", 0.0),
                    ])?,
                    corona: read_color([("corona.r", 1.0), ("corona.g", 1.0), ("corona.b", 1.0)])?,
                    corona_size,
                    sun_road_fade_distance,
                    oscillators: [
                        floats(
                            store,
                            kind,
                            [("Oscillator1.Freq", 15.0), ("Oscillator1.Flicker", 0.1)],
                        )?,
                        floats(
                            store,
                            kind,
                            [("Oscillator2.Freq", 1.0), ("Oscillator2.Flicker", 0.5)],
                        )?,
                    ],
                },
            );
        }
        let [corona, reflected] = ["CoronaTexture", "FlareSunRoadTexture"]
            .map(|field| texture(game, paths, store.value(root, Some(field))));
        Ok(Some(Self {
            types: parsed,
            textures: [corona?, reflected?],
            max_lights: store.as_dword(root, Some("MaxD3DLights"), 7).min(7) as usize,
        }))
    }
}

fn texture(
    game: &GameDirectory,
    paths: &TexturePaths,
    name: Option<&str>,
) -> Result<Option<Rc<Texture>>> {
    let Some(name) = name.filter(|name| !name.is_empty()) else {
        return Ok(None);
    };
    Ok(game.load_texture(paths, name)?.map(Rc::new))
}

struct Oscillator {
    phase: f32,
    old: f32,
    new: f32,
}

impl Oscillator {
    fn new(random: &mut Random, amplitude: f32) -> Self {
        Self {
            phase: 0.0,
            old: random.range(-amplitude, amplitude),
            new: random.range(-amplitude, amplitude),
        }
    }

    fn advance(
        &mut self,
        seconds: f32,
        [frequency, amplitude]: [f32; 2],
        random: &mut Random,
    ) -> f32 {
        self.phase += frequency * seconds;
        if self.phase >= 2.0 {
            *self = Self::new(random, amplitude);
        } else if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.old = self.new;
            self.new = random.range(-amplitude, amplitude);
        }
        self.old + (self.new - self.old) * self.phase
    }
}

struct Lamp {
    owner: EntityId,
    node: Option<String>,
    local: [f32; 3],
    position: [f32; 3],
    kind: LightType,
    flare_only: bool,
    enabled: bool,
    oscillators: [Oscillator; 2],
    intensity: f32,
}

fn point(matrix: &Matrix, [x, y, z]: [f32; 3]) -> Result<[f32; 3]> {
    let result = std::array::from_fn(|i| {
        matrix[i] * x + matrix[4 + i] * y + matrix[8 + i] * z + matrix[12 + i]
    });
    ensure!(
        result.into_iter().all(f32::is_finite),
        "non-finite ship lamp position"
    );
    Ok(result)
}

fn attachment(name: &str) -> Option<String> {
    let bytes = name.as_bytes();
    let (digits, suffix) = match bytes.get(1) {
        Some(b'm') => (2..3, 3),
        Some(b'r') => (2..5, 5),
        _ => return None,
    };
    let mut index = 0u32;
    for &digit in bytes[digits.start..digits.end.min(bytes.len())]
        .iter()
        .take_while(|b| b.is_ascii_digit())
    {
        index = index * 10 + u32::from(digit - b'0');
    }
    Some(
        if let Some(&letter) = bytes.get(suffix).filter(|b| b.is_ascii_lowercase()) {
            format!("rey_{}{index}", char::from(letter))
        } else {
            format!("mast{index}")
        },
    )
}

#[derive(Default)]
struct State {
    catalog: Option<Catalog>,
    lamps: Vec<Lamp>,
}

#[derive(Clone)]
pub(crate) struct ShipLights(Rc<Shared>);
struct Shared {
    game: GameDirectory,
    random: Rc<RefCell<Random>>,
    paths: Rc<RefCell<TexturePaths>>,
    states: RefCell<BTreeMap<u64, State>>,
}

impl ShipLights {
    pub(crate) fn register(host: &mut NativeHost, game: GameDirectory) -> Result<()> {
        let lights = Self(Rc::new(Shared {
            game,
            random: host.random.clone(),
            paths: host.texture_paths.clone(),
            states: RefCell::default(),
        }));
        let factory = lights.clone();
        if host.entities.register_class("ShipLights", move || {
            Ok(Box::new(Actor {
                lights: factory.clone(),
                id: EntityId::INVALID,
            }))
        })? {
            host.ship_lights = Some(lights);
        }
        Ok(())
    }

    pub(crate) fn retain_owners(&self, mut valid: impl FnMut(EntityId) -> bool) {
        for state in self.0.states.borrow_mut().values_mut() {
            state.lamps.retain(|lamp| valid(lamp.owner));
        }
    }
}

struct Actor {
    lights: ShipLights,
    id: EntityId,
}

impl Entity for Actor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        self.id = context.id;
        self.lights
            .0
            .states
            .borrow_mut()
            .insert(self.id.raw(), State::default());
        Ok(true)
    }

    fn process_stage_checked(&mut self, stage: Stage, delta_ms: u32) -> Result<()> {
        if stage != Stage::Execute {
            return Ok(());
        }
        let mut states = self.lights.0.states.borrow_mut();
        let state = states
            .get_mut(&self.id.raw())
            .context("missing ShipLights state")?;
        if state.lamps.iter().any(|lamp| lamp.enabled) {
            let seconds = delta_ms as f32 * 0.001;
            let mut random = self.lights.0.random.borrow_mut();
            for lamp in state.lamps.iter_mut().filter(|lamp| lamp.enabled) {
                let mut intensity = 0.0;
                let mut weight = 1.0;
                for (oscillator, settings) in lamp.oscillators.iter_mut().zip(lamp.kind.oscillators)
                {
                    let wave = oscillator.advance(seconds, settings, &mut random);
                    intensity += wave * weight;
                    if settings[1] != 0.0 {
                        weight -= wave * (1.0 / settings[1]);
                    }
                    if weight < 0.0 {
                        break;
                    }
                }
                lamp.intensity = 1.0 + intensity.clamp(-1.0, 1.0);
            }
        }
        Ok(())
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        if message.long()? == MSG_SEA_SUNROAD_DRAW {
            let states = self.lights.0.states.borrow();
            let state = states
                .get(&self.id.raw())
                .context("missing ShipLights state")?;
            ensure!(
                !state.lamps.iter().any(|lamp| lamp.enabled),
                "ShipLights sun-road rendering is not ported"
            );
        }
        Ok(0)
    }

    fn destroy(&mut self, _: &mut EntityDestroy<'_>) -> Result<()> {
        self.lights.0.states.borrow_mut().remove(&self.id.raw());
        Ok(())
    }
}

impl NativeHost {
    /// ShipLights::AddLights receives the ship's labels in model-node order.
    pub fn attach_ship_lights(
        &self,
        owner: EntityId,
        labels: impl IntoIterator<Item = impl std::borrow::Borrow<Label>>,
        pose: &ShipLightPose<'_>,
    ) -> Result<usize> {
        ensure!(self.entities.is_valid(owner), "invalid ship lamp owner");
        let id = self
            .entities
            .first_id("ShipLights")
            .context("ShipLights entity is not active")?;
        let lights = self
            .ship_lights
            .as_ref()
            .context("ShipLights service is unavailable")?;
        let mut states = lights.0.states.borrow_mut();
        let state = states
            .get_mut(&id.raw())
            .context("missing ShipLights state")?;
        if state.catalog.is_none() {
            let root = self.entities.attribute_root(id);
            state.catalog = self.world().read(|store| {
                Catalog::load(store, root, &lights.0.game, &lights.0.paths.borrow())
            })??;
        }
        let Some(catalog) = &state.catalog else {
            return Ok(0);
        };
        if !catalog.types.contains_key("default") {
            return Ok(0);
        }
        let mut lamps = Vec::new();
        let mut random = lights.0.random.borrow_mut();
        for label in labels {
            let label = label.borrow();
            let name = label.name.to_ascii_lowercase();
            let (kind, flare_only) = match (label.group.as_str(), name.as_bytes().first()) {
                ("lights", Some(b'l')) => ("default", false),
                ("flares", Some(b'f')) => ("flare", true),
                _ => continue,
            };
            let Some(&kind) = catalog.types.get(kind) else {
                continue;
            };
            let position = [label.matrix[12], label.matrix[13], label.matrix[14]];
            let node = attachment(&name);
            let (local, position) = if let Some(node) = &node {
                let Some(matrix) = pose.nodes.get(node) else {
                    continue;
                };
                (point(&transpose(*matrix)?, position)?, position)
            } else {
                (position, point(pose.root, position)?)
            };
            lamps.push(Lamp {
                owner,
                node,
                local,
                position,
                kind,
                flare_only,
                enabled: false,
                oscillators: kind
                    .oscillators
                    .map(|[_, amplitude]| Oscillator::new(&mut random, amplitude)),
                intensity: 1.0,
            });
        }
        let count = lamps.len();
        state.lamps.extend(lamps);
        Ok(count)
    }

    pub fn update_ship_light_pose(&self, owner: EntityId, pose: &ShipLightPose<'_>) -> Result<()> {
        self.with_ship_lights(|state| {
            let positions = state
                .lamps
                .iter()
                .filter(|lamp| lamp.owner == owner)
                .map(|lamp| {
                    let matrix = match &lamp.node {
                        Some(node) => pose
                            .nodes
                            .get(node)
                            .context("missing attached ship lamp node")?,
                        None => pose.root,
                    };
                    point(matrix, lamp.local)
                })
                .collect::<Result<Vec<_>>>()?;
            for (lamp, position) in state
                .lamps
                .iter_mut()
                .filter(|lamp| lamp.owner == owner)
                .zip(positions)
            {
                lamp.position = position;
            }
            Ok(())
        })
        .unwrap_or(Ok(()))
    }

    /// Retains ResetLights' shared switch across all attached ships.
    pub fn reset_ship_lights(&self, enabled: bool) {
        self.with_ship_lights(|state| {
            for lamp in &mut state.lamps {
                lamp.enabled = enabled;
            }
        });
    }

    pub fn ship_corona_texture(&self, reflected: bool) -> Option<Rc<Texture>> {
        self.with_ship_lights(|state| {
            state.catalog.as_ref()?.textures[usize::from(reflected)].clone()
        })
        .flatten()
    }

    pub fn ship_coronas(&self) -> Vec<Corona> {
        self.with_ship_lights(|state| {
            state
                .lamps
                .iter()
                .filter(|lamp| lamp.enabled && self.entities.is_valid(lamp.owner))
                .map(|lamp| {
                    let intensity = 0.5 + lamp.intensity * 0.5;
                    Corona {
                        position: lamp.position,
                        rgb: lamp
                            .kind
                            .corona
                            .map(|channel| (channel * intensity).clamp(0.0, 1.0)),
                        size: lamp.kind.corona_size * intensity,
                        sun_road_fade_distance: lamp.kind.sun_road_fade_distance,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
    }

    pub fn ship_point_lights(&self, owner: EntityId, position: [f32; 3]) -> Vec<PointLight> {
        self.with_ship_lights(|state| {
            let Some(catalog) = &state.catalog else {
                return Vec::new();
            };
            let distance = |lamp: &Lamp| {
                lamp.position
                    .into_iter()
                    .zip(position)
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum::<f32>()
            };
            let (own, mut others): (Vec<_>, Vec<_>) = state
                .lamps
                .iter()
                .filter(|lamp| {
                    lamp.enabled && !lamp.flare_only && self.entities.is_valid(lamp.owner)
                })
                .partition(|lamp| lamp.owner == owner);
            others.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
            own.into_iter()
                .rev()
                .chain(others)
                .take(catalog.max_lights)
                .map(|lamp| PointLight {
                    position: lamp.position,
                    rgb: lamp.kind.rgb.map(|channel| channel * lamp.intensity),
                    range: lamp.kind.range,
                    attenuation: lamp.kind.attenuation,
                })
                .collect()
        })
        .unwrap_or_default()
    }

    fn with_ship_lights<T>(&self, f: impl FnOnce(&mut State) -> T) -> Option<T> {
        let id = self.entities.first_id("ShipLights")?;
        let mut states = self.ship_lights.as_ref()?.0.states.borrow_mut();
        states.get_mut(&id.raw()).map(f)
    }
}
