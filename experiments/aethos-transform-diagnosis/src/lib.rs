use aethos_util::math::{Angle, Pos, Vec};

pub mod bevy_backend;
pub mod edict_backend;
pub mod edict_3_backend;
pub mod edict_bevy_backend;
pub mod hecs_backend;
pub mod shipyard_backend;

#[derive(edict::component::Component, bevy_ecs::component::Component, shipyard::Component)]
pub struct Level<const N: usize>;

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Transform {
    pub pos: Pos,
    pub angle: Angle,
}

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct LocalTransform {
    pub offset: Vec<i32>,
    pub angle: Angle,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub parent: Option<usize>,
    pub global: Transform,
    pub local: LocalTransform,
}

#[derive(Clone, Debug)]
pub struct Fixture {
    pub nodes: std::vec::Vec<Node>,
    pub spawn_order: std::vec::Vec<usize>,
}

impl Fixture {
    pub fn depths(&self) -> std::vec::Vec<usize> {
        (0..self.nodes.len())
            .map(|index| {
                let mut depth = 0;
                let mut parent = self.nodes[index].parent;
                while let Some(index) = parent {
                    depth += 1;
                    parent = self.nodes[index].parent;
                }
                depth
            })
            .collect()
    }
}

pub fn compose(parent: Transform, local: LocalTransform) -> Transform {
    Transform {
        pos: parent.pos + local.offset.as_f32().rotated(parent.angle).as_i32(),
        angle: parent.angle + local.angle,
    }
}

pub trait Backend: Sized {
    fn new(fixture: &Fixture) -> Self;
    fn set_root(&mut self, index: usize, value: Transform);
    fn set_local(&mut self, index: usize, value: LocalTransform);
    fn reparent(&mut self, index: usize, parent: usize);
    fn sync(&mut self);
    fn global(&mut self, index: usize) -> Transform;
}

/// Forests of four-level trees, branching four times at each level.
pub fn forest(count: usize) -> Fixture {
    let nodes = (0..count)
        .map(|index| {
            let within_tree = index % 85;
            let parent = (within_tree != 0).then(|| index - within_tree + (within_tree - 1) / 4);
            Node {
                parent,
                global: Transform {
                    pos: Pos::default() + Vec::new((index % 17) as i32, (index % 23) as i32),
                    angle: Angle::new(0.1),
                },
                local: LocalTransform {
                    offset: Vec::new(3, 7),
                    angle: Angle::new(0.025),
                },
            }
        })
        .collect();
    Fixture {
        nodes,
        spawn_order: (0..count).collect(),
    }
}

/// Independent full recomputation; supports parents appearing after children.
pub fn expected(fixture: &Fixture) -> std::vec::Vec<Transform> {
    fn resolve(index: usize, fixture: &Fixture, out: &mut [Option<Transform>]) -> Transform {
        if let Some(value) = out[index] {
            return value;
        }
        let node = &fixture.nodes[index];
        let value = match node.parent {
            Some(parent) => compose(resolve(parent, fixture, out), node.local),
            None => node.global,
        };
        out[index] = Some(value);
        value
    }
    let mut out = vec![None; fixture.nodes.len()];
    for index in 0..out.len() {
        resolve(index, fixture, &mut out);
    }
    out.into_iter().map(Option::unwrap).collect()
}

pub fn assert_matches<B: Backend>(backend: &mut B, fixture: &Fixture) {
    for (index, value) in expected(fixture).into_iter().enumerate() {
        assert_eq!(backend.global(index), value, "node {index}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depths_follow_parents_with_reversed_spawn_order() {
        let fixture = Fixture {
            nodes: (0..4)
                .map(|index| Node {
                    parent: (index < 3).then_some(index + 1),
                    global: Transform::default(),
                    local: LocalTransform::default(),
                })
                .collect(),
            spawn_order: (0..4).rev().collect(),
        };
        assert_eq!(fixture.depths(), [3, 2, 1, 0]);
        let depths = forest(85).depths();
        for (level, count) in [1, 4, 16, 64].into_iter().enumerate() {
            assert_eq!(depths.iter().filter(|&&depth| depth == level).count(), count);
        }
    }
}

pub mod edict_alt_backend;

pub mod edict_narrow_backend;

pub mod edict_located_backend;

pub mod edict_3_alt_backend;
