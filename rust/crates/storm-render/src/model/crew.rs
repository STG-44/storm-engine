//! Original two-bone GM skinning and AN walk clips over original deck links.
use super::ModelVertex;
use anyhow::{Result, ensure};
use glam::{Mat4, Quat, Vec3};
use std::sync::Arc;
use storm_formats::{
    animation::{Animation, Clip},
    geometry::Vertex,
    waypoints::DeckGraph,
};

#[derive(Debug)]
pub struct Skeleton {
    pub animation: Animation,
    pub walk: Clip,
    inverse_bind: Vec<Mat4>,
}
impl Skeleton {
    pub fn new(animation: Animation, walk: Clip) -> Self {
        let mut bind = vec![Mat4::IDENTITY; animation.parents.len()];
        for &i in &animation.order {
            let local = Mat4::from_rotation_translation(
                Quat::from_array(animation.rotations[i * animation.frames]).normalize(),
                Vec3::from_array(animation.bind_positions[i]),
            );
            bind[i] = animation.parents[i].map_or(local, |p| bind[p] * local);
        }
        let inverse_bind = bind.into_iter().map(|m| m.inverse()).collect();
        Self {
            animation,
            walk,
            inverse_bind,
        }
    }
    fn palette(&self, seconds: f32) -> Vec<Mat4> {
        let a = &self.animation;
        let span = (self.walk.last - self.walk.first) as f32;
        let frame = self.walk.first as f32
            + if span > 0.0 {
                seconds.rem_euclid(span / a.fps) * a.fps
            } else {
                0.0
            };
        let first = frame.floor() as usize;
        let next = (first + 1).min(self.walk.last);
        let k = frame.fract();
        let mut result = vec![Mat4::IDENTITY; a.parents.len()];
        for &i in &a.order {
            let q = |f| Quat::from_array(a.rotations[i * a.frames + f]).normalize();
            let position = if i == 0 {
                Vec3::from_array(a.root_positions[first])
                    .lerp(Vec3::from_array(a.root_positions[next]), k)
            } else {
                Vec3::from_array(a.bind_positions[i])
            };
            let local = Mat4::from_rotation_translation(q(first).slerp(q(next), k), position);
            result[i] = a.parents[i].map_or(local, |p| result[p] * local);
        }
        for (i, matrix) in result.iter_mut().enumerate() {
            *matrix *= self.inverse_bind[i];
        }
        result
    }
}

#[derive(Debug)]
pub struct Patrol {
    pub points: Vec<Vec3>,
    lengths: Vec<f32>,
    pub length: f32,
}
impl Patrol {
    pub fn from_graph(graph: &DeckGraph, id: usize) -> Result<Self> {
        let starts = graph
            .points
            .iter()
            .enumerate()
            .filter(|(_, p)| p.kind == 0)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        ensure!(
            !starts.is_empty(),
            "crew graph has no normal starting point"
        );
        let start = starts[(id * 7) % starts.len()];
        let mut adjacency = vec![Vec::new(); graph.points.len()];
        for &[a, b] in &graph.links {
            if [a, b]
                .into_iter()
                .any(|i| (5..=9).contains(&graph.points[i].kind))
            {
                continue;
            }
            adjacency[a].push(b);
            adjacency[b].push(a);
        }
        for a in &mut adjacency {
            if !a.is_empty() {
                let n = a.len();
                a.rotate_left(id % n);
            }
        }
        fn visit(i: usize, adj: &[Vec<usize>], seen: &mut [bool], out: &mut Vec<usize>) {
            seen[i] = true;
            out.push(i);
            for &next in &adj[i] {
                if !seen[next] {
                    visit(next, adj, seen, out);
                    out.push(i);
                }
            }
        }
        let mut path = Vec::new();
        visit(
            start,
            &adjacency,
            &mut vec![false; graph.points.len()],
            &mut path,
        );
        let points = path
            .into_iter()
            .map(|i| Vec3::from_array(graph.points[i].position))
            .collect::<Vec<_>>();
        let lengths = points
            .windows(2)
            .map(|p| p[0].distance(p[1]))
            .collect::<Vec<_>>();
        let length = lengths.iter().sum();
        Ok(Self {
            points,
            lengths,
            length,
        })
    }
    pub fn sample(&self, seconds: f32) -> (Vec3, Vec3) {
        if self.length < 0.001 {
            return (self.points[0], Vec3::Z);
        }
        let mut distance = seconds.rem_euclid(self.length / 1.3) * 1.3;
        for (i, &length) in self.lengths.iter().enumerate() {
            if length > 0.0001 && distance <= length {
                return (
                    self.points[i].lerp(self.points[i + 1], distance / length),
                    (self.points[i + 1] - self.points[i]).normalize(),
                );
            }
            distance -= length;
        }
        (self.points[0], Vec3::Z)
    }
}

#[derive(Clone, Debug)]
pub struct CrewPatch {
    pub skeleton: Arc<Skeleton>,
    pub source: Arc<Vec<Vertex>>,
    pub route: Arc<Patrol>,
    pub phase: f32,
}
impl CrewPatch {
    pub fn deform(&self, out: &mut [ModelVertex], seconds: f32) -> Result<()> {
        ensure!(
            seconds.is_finite() && out.len() == self.source.len(),
            "invalid crew frame"
        );
        let palette = self.skeleton.palette(seconds + self.phase);
        let (position, direction) = self.route.sample(seconds + self.phase);
        let placement = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0))
            * Mat4::from_translation(position)
            * Mat4::from_rotation_y(direction.x.atan2(direction.z))
            * Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
        for (source, dest) in self.source.iter().zip(out) {
            let skin = source
                .skin
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("crew vertex has no skin weights"))?;
            let [a, b] = skin.bones.map(|b| {
                palette
                    .get(b as usize)
                    .copied()
                    .ok_or_else(|| anyhow::anyhow!("crew bone is outside skeleton"))
            });
            let (a, b) = (a?, b?);
            let w = skin.weight;
            let position = Vec3::from_array(source.position);
            let normal = Vec3::from_array(source.normal);
            *dest = ModelVertex {
                position: placement
                    .transform_point3(
                        a.transform_point3(position) * w + b.transform_point3(position) * (1.0 - w),
                    )
                    .to_array(),
                normal: placement
                    .transform_vector3(
                        a.transform_vector3(normal) * w + b.transform_vector3(normal) * (1.0 - w),
                    )
                    .normalize_or_zero()
                    .to_array(),
                uv: source.uv,
                color: [1.0; 4],
            };
        }
        Ok(())
    }
}
