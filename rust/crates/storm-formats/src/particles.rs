//! Bounded PSYS v3.5 (XPS) data reader and retained particle curve arithmetic.
use crate::FormatError;
use serde_json::{Value as Json, json};

/// Portable relative cache key used by PARTICLES::CreateSystem and DataCache.
/// Windows separators are accepted on every host; unsafe/non-ASCII paths are excluded.
pub fn profile_key(name: &str) -> Result<String, FormatError> {
    resource_key(name, "xps")
}
pub(super) fn resource_key(name: &str, suffix: &str) -> Result<String, FormatError> {
    let path = name.replace('\\', "/");
    if path.is_empty() || path.len() > 512 || !path.is_ascii() || path.contains(['\0', ':']) {
        return Err(FormatError("invalid particle profile name"));
    }
    if path
        .split('/')
        .any(|p| p.is_empty() || matches!(p, "." | ".."))
    {
        return Err(FormatError(
            "particle profile path must be relative without traversal",
        ));
    }
    let extension = std::path::Path::new(&path)
        .extension()
        .and_then(|s| s.to_str());
    let suffix = if extension.is_some_and(|s| s.eq_ignore_ascii_case(suffix)) {
        ""
    } else {
        suffix
    };
    Ok(if suffix.is_empty() {
        path.to_ascii_lowercase()
    } else {
        format!("{path}.{suffix}").to_ascii_lowercase()
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text(pub Vec<u8>);
impl Text {
    pub fn display(&self) -> String {
        match std::str::from_utf8(&self.0) {
            Ok(s) => s.to_owned(),
            Err(_) => encoding_rs::WINDOWS_1251.decode(&self.0).0.into_owned(),
        }
    }
    pub fn matches(&self, name: &str) -> bool {
        self.0.eq_ignore_ascii_case(name.as_bytes())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct GraphPoint {
    pub time: f32,
    pub value: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Graph {
    pub negative: bool,
    pub relative: bool,
    pub max: Vec<GraphPoint>,
    pub min: Vec<GraphPoint>,
}
fn lerp(a: f32, b: f32, k: f32) -> f32 {
    a + (b - a) * k
}
impl Graph {
    /// Retains the first matching segment, including extrapolation before its start.
    /// An empty/single-point graph safely returns zero (native empty graphs are unsafe).
    pub fn bounds(&self, time: f32, lifetime: f32) -> (f32, f32) {
        let time = if self.relative {
            time / lifetime * 100.0
        } else {
            time
        };
        let evaluate = |points: &[GraphPoint]| {
            for pair in points.windows(2) {
                if time <= pair[1].time {
                    let delta = pair[1].time - pair[0].time;
                    let k = if delta > 0.001 {
                        (time - pair[0].time) / delta
                    } else {
                        0.0
                    };
                    return lerp(pair[0].value, pair[1].value, k);
                }
            }
            0.0
        };
        (evaluate(&self.min), evaluate(&self.max))
    }
    pub fn sample(&self, time: f32, lifetime: f32, random: f32) -> f32 {
        let (min, max) = self.bounds(time, lifetime);
        lerp(min, max, random)
    }
    /// Native GetRandomValue swaps inverted limits and uses rand()%1000.
    pub fn sample_range(&self, time: f32, lifetime: f32, random: u32) -> f32 {
        let (mut min, mut max) = self.bounds(time, lifetime);
        if min > max {
            std::mem::swap(&mut min, &mut max);
        }
        let mut value = (random % 1000) as f32 / 1000.0;
        value *= max - min;
        value + min
    }
    pub fn maximum(&self) -> f32 {
        self.min
            .iter()
            .chain(&self.max)
            .fold(0.0, |a, p| a.max(p.value))
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ColorPoint {
    pub time: f32,
    pub max: [f32; 4],
    pub min: [f32; 4],
}
#[derive(Clone, Debug, PartialEq)]
pub struct ColorGraph(pub Vec<ColorPoint>);
impl ColorGraph {
    pub fn sample(&self, time: f32, lifetime: f32, random: f32) -> [f32; 4] {
        let time = time / lifetime;
        for pair in self.0.windows(2) {
            if time >= pair[0].time && time <= pair[1].time {
                let delta = pair[1].time - pair[0].time;
                let k = if delta > 0.001 {
                    (time - pair[0].time) / delta
                } else {
                    0.0
                };
                return std::array::from_fn(|i| {
                    if i == 3 {
                        1.0
                    } else {
                        lerp(
                            lerp(pair[0].min[i], pair[1].min[i], k),
                            lerp(pair[0].max[i], pair[1].max[i], k),
                            random,
                        )
                    }
                });
            }
        }
        [1.0; 4]
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Float(f32),
    Graph(Graph),
    Position([f32; 3]),
    String(Text),
    /// Converted from saved (u,v,width,height) to native (u1,v1,u2,v2).
    Uv(Vec<[f32; 4]>),
    Color(ColorGraph),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: Text,
    pub value: Value,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Fields(pub Vec<Field>);
impl Fields {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.0
            .iter()
            .find(|f| f.name.matches(name))
            .map(|f| &f.value)
    }
    pub fn graph(&self, name: &str) -> Option<&Graph> {
        self.0.iter().find_map(|f| match &f.value {
            Value::Graph(g) if f.name.matches(name) => Some(g),
            _ => None,
        })
    }
    pub fn float(&self, name: &str) -> Option<f32> {
        self.0.iter().find_map(|f| match f.value {
            Value::Float(v) if f.name.matches(name) => Some(v),
            _ => None,
        })
    }
    pub fn text(&self, name: &str) -> Option<&Text> {
        self.0.iter().find_map(|f| match &f.value {
            Value::String(v) if f.name.matches(name) => Some(v),
            _ => None,
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Particle {
    pub kind: u32,
    pub fields: Fields,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Emitter {
    pub fields: Fields,
    pub particles: Vec<Particle>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    pub emitters: Vec<Emitter>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
    budget: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], FormatError> {
        let end = self
            .offset
            .checked_add(n)
            .ok_or(FormatError("XPS offset overflow"))?;
        let part = self
            .bytes
            .get(self.offset..end)
            .ok_or(FormatError("truncated XPS data"))?;
        self.offset = end;
        Ok(part)
    }
    fn u32(&mut self) -> Result<u32, FormatError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn float(&mut self) -> Result<f32, FormatError> {
        let f = f32::from_bits(self.u32()?);
        if !f.is_finite() {
            return Err(FormatError("non-finite XPS value"));
        }
        Ok(f)
    }
    fn floats<const N: usize>(&mut self) -> Result<[f32; N], FormatError> {
        let mut a = [0.0; N];
        for v in &mut a {
            *v = self.float()?;
        }
        Ok(a)
    }
    fn count(&mut self, limit: usize, stride: usize) -> Result<usize, FormatError> {
        let n = self.u32()? as usize;
        if n > limit || n > self.budget || n > self.bytes.len().saturating_sub(self.offset) / stride
        {
            return Err(FormatError("XPS count exceeds input or budget"));
        }
        self.budget -= n;
        Ok(n)
    }
    fn string(&mut self, n: usize) -> Result<Text, FormatError> {
        let bytes = self.take(n)?;
        let end = bytes
            .iter()
            .position(|v| *v == 0)
            .ok_or(FormatError("unterminated XPS string"))?;
        Ok(Text(bytes[..end].to_vec()))
    }
    fn name(&mut self) -> Result<Text, FormatError> {
        let n = self.u32()? as usize;
        if n == 0 || n >= 128 {
            return Err(FormatError("invalid XPS field-name length"));
        }
        self.string(n)
    }
    fn points(&mut self, negative: bool) -> Result<Vec<GraphPoint>, FormatError> {
        let count = self.count(16_384, 8)?;
        (0..count)
            .map(|_| {
                let time = self.float()?;
                let mut value = self.float()?;
                if !negative && value < 0.0 {
                    value = 0.0;
                }
                Ok(GraphPoint { time, value })
            })
            .collect()
    }
    fn fields(&mut self) -> Result<Fields, FormatError> {
        let count = self.count(256, 9)?;
        let mut fields = Vec::with_capacity(count);
        for _ in 0..count {
            let value = match self.u32()? {
                1 => Value::Bool(self.u32()? != 0),
                2 => Value::Float(self.float()?),
                3 => {
                    let negative = self.u32()? != 0;
                    let relative = self.u32()? != 0;
                    Value::Graph(Graph {
                        negative,
                        relative,
                        max: self.points(negative)?,
                        min: self.points(negative)?,
                    })
                }
                4 => Value::Position(self.floats()?),
                5 => Value::String(self.string(128)?),
                6 => {
                    let count = self.count(16_384, 16)?;
                    let mut frames = Vec::with_capacity(count);
                    for _ in 0..count {
                        let [x, y, w, h] = self.floats()?;
                        let frame = [x, y, x + w, y + h];
                        if !frame.iter().all(|f| f.is_finite()) {
                            return Err(FormatError("XPS UV extent overflow"));
                        }
                        frames.push(frame);
                    }
                    Value::Uv(frames)
                }
                7 => {
                    let count = self.count(16_384, 36)?;
                    let points = (0..count)
                        .map(|_| {
                            Ok(ColorPoint {
                                time: self.float()?,
                                max: self.floats()?,
                                min: self.floats()?,
                            })
                        })
                        .collect::<Result<_, FormatError>>()?;
                    Value::Color(ColorGraph(points))
                }
                _ => return Err(FormatError("unsupported XPS field type")),
            };
            fields.push(Field {
                name: self.name()?,
                value,
            });
        }
        Ok(Fields(fields))
    }
}
impl Profile {
    pub fn read(bytes: &[u8]) -> Result<Self, FormatError> {
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(FormatError("XPS input exceeds 16 MiB"));
        }
        let mut reader = Reader {
            bytes,
            offset: 0,
            budget: 262_144,
        };
        if reader.take(8)? != b"PSYSv3.5" {
            return Err(FormatError("unsupported XPS signature or version"));
        }
        let count = reader.count(256, 12)?;
        let mut emitters = Vec::with_capacity(count);
        for _ in 0..count {
            if reader.u32()? != 0 {
                return Err(FormatError("unsupported XPS emitter type"));
            }
            let fields = reader.fields()?;
            let count = reader.count(256, 8)?;
            let mut particles = Vec::with_capacity(count);
            for _ in 0..count {
                let kind = reader.u32()?;
                if kind != 1 && kind != 2 {
                    return Err(FormatError("unsupported XPS particle type"));
                }
                particles.push(Particle {
                    kind,
                    fields: reader.fields()?,
                });
            }
            emitters.push(Emitter { fields, particles });
        }
        if reader.offset != bytes.len() {
            return Err(FormatError("trailing XPS data"));
        }
        Ok(Self { emitters })
    }
    /// Raw loaded fields, before native schema Convert adds defaults/removes unknown fields.
    pub fn report(&self) -> Json {
        fn fields(fields: &Fields) -> Json {
            json!(fields.0.iter().map(|f| {
                let (kind,value)=match &f.value {
                    Value::Bool(v)=>("bool",json!(v)), Value::Float(v)=>("float",json!(v.to_bits())),
                    Value::Position(v)=>("position",json!(v.map(f32::to_bits))), Value::String(v)=>("string",json!(v.0)),
                    Value::Graph(g)=>("graph",json!({"negative":g.negative,"relative":g.relative,"max":g.max.iter().map(|p|[p.time.to_bits(),p.value.to_bits()]).collect::<Vec<_>>(),"min":g.min.iter().map(|p|[p.time.to_bits(),p.value.to_bits()]).collect::<Vec<_>>()})),
                    Value::Uv(v)=>("uv",json!(v.iter().map(|p|p.map(f32::to_bits)).collect::<Vec<_>>())),
                    Value::Color(c)=>("color",json!(c.0.iter().map(|p|json!({"time":p.time.to_bits(),"max":p.max.map(f32::to_bits),"min":p.min.map(f32::to_bits)})).collect::<Vec<_>>())),
                };
                json!({"name":f.name.0,"kind":kind,"value":value})
            }).collect::<Vec<_>>())
        }
        json!({"signature":"PSYSv3.5","stage":"raw fields before schema conversion","emitters":self.emitters.iter().map(|e|json!({"fields":fields(&e.fields),"particles":e.particles.iter().map(|p|json!({"kind":p.kind,"fields":fields(&p.fields)})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
    }
}
#[cfg(test)]
mod tests;
