use extrem_ecs::{Entity, World, WorldError};
use extrem_math::{Quat, Transform, Vec3};
use extrem_scene::{Children, GlobalTransform, Parent};

#[derive(Clone, Copy, Debug)]
pub enum Shape {
    Chain,
    Wide,
    Balanced,
    Independent,
}

impl Shape {
    pub const ALL: [Self; 4] = [Self::Chain, Self::Wide, Self::Balanced, Self::Independent];

    pub fn name(self) -> &'static str {
        match self {
            Self::Chain => "chain",
            Self::Wide => "wide",
            Self::Balanced => "balanced",
            Self::Independent => "independent",
        }
    }

    pub fn parent(self, index: usize) -> Option<usize> {
        if index == 0 {
            return None;
        }
        match self {
            Self::Chain => Some(index - 1),
            Self::Wide => Some(0),
            Self::Balanced => Some((index - 1) / 2),
            Self::Independent => None,
        }
    }
}

pub fn local(index: usize) -> Transform {
    Transform {
        translation: Vec3::new((index % 7) as f32 / 16.0, -0.0, 0.125),
        rotation: Quat::from_axis_angle(Vec3::Y, 0.002),
        scale: Vec3::ONE,
    }
}

/// O(N) construction, deliberately not repeated calls to ancestor-checking set_parent.
pub fn fixture(shape: Shape, nodes: usize) -> Result<(World, Vec<Entity>), WorldError> {
    let mut world = World::new();
    let mut entities = Vec::with_capacity(nodes);
    for index in 0..nodes {
        let entity = world.try_spawn(local(index))?;
        world.insert(entity, GlobalTransform(Transform::IDENTITY))?;
        entities.push(entity);
    }
    let mut children = vec![Vec::new(); nodes];
    for index in 0..nodes {
        if let Some(parent) = shape.parent(index) {
            world.insert(entities[index], Parent(entities[parent]))?;
            children[parent].push(entities[index]);
        }
    }
    for (&entity, children) in entities.iter().zip(children) {
        world.insert(entity, Children(children))?;
    }
    Ok((world, entities))
}

/// Bit-level fingerprint of TRS fields, including signed zero and non-finite payloads.
pub fn bits(transform: Transform) -> [u32; 10] {
    let t = transform.translation;
    let r = transform.rotation;
    let s = transform.scale;
    [t.x, t.y, t.z, r.x, r.y, r.z, r.w, s.x, s.y, s.z].map(f32::to_bits)
}

pub fn snapshot(world: &World) -> Vec<(Entity, [u32; 10])> {
    let mut globals: Vec<_> = world
        .iter::<GlobalTransform>()
        .map(|(entity, global)| (entity, bits(global.0)))
        .collect();
    globals.sort_unstable_by_key(|(entity, _)| *entity);
    globals
}
