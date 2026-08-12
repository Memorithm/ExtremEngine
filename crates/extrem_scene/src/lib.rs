use extrem_ecs::{Entity, World, WorldError};
use extrem_math::{Transform, Vec3};

/// Human-readable label attached to an entity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name(pub String);

impl From<&str> for Name {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

/// Linear velocity in world units per second.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Velocity(pub Vec3);

/// Simple visibility flag for future render extraction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Visibility(pub bool);

/// Local parent relationship in the scene graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parent(pub Entity);

/// Children belonging to an entity in the scene graph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Children(pub Vec<Entity>);

/// World-space transform produced by scene propagation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlobalTransform(pub Transform);

/// A collection of root entities belonging to a named scene.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scene {
    pub name: String,
    pub roots: Vec<Entity>,
}

impl Scene {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            roots: Vec::new(),
        }
    }

    pub fn spawn_entity(
        &mut self,
        world: &mut World,
        name: impl Into<String>,
        transform: Transform,
    ) -> Result<Entity, WorldError> {
        let entity = world.spawn_empty();
        world.insert(entity, Name(name.into()))?;
        world.insert(entity, transform)?;
        world.insert(entity, GlobalTransform(transform))?;
        world.insert(entity, Visibility(true))?;
        world.insert(entity, Children::default())?;
        self.roots.push(entity);
        Ok(entity)
    }

    pub fn spawn_child(
        &mut self,
        world: &mut World,
        parent: Entity,
        name: impl Into<String>,
        transform: Transform,
    ) -> Result<Entity, WorldError> {
        if !world.contains(parent) {
            return Err(WorldError::EntityNotFound(parent));
        }

        let entity = world.spawn_empty();
        world.insert(entity, Name(name.into()))?;
        world.insert(entity, transform)?;
        world.insert(entity, GlobalTransform(transform))?;
        world.insert(entity, Visibility(true))?;
        world.insert(entity, Parent(parent))?;
        if let Some(children) = world.get_mut::<Children>(parent) {
            children.0.push(entity);
        } else {
            world.insert(parent, Children(vec![entity]))?;
        }
        Ok(entity)
    }
}

/// Recomputes world transforms from roots down through the scene graph.
pub fn propagate_transforms(world: &mut World) {
    let roots: Vec<_> = world
        .iter::<Transform>()
        .filter_map(|(entity, transform)| {
            world
                .get::<Parent>(entity)
                .is_none()
                .then_some((entity, *transform))
        })
        .collect();

    let mut pending = roots;
    while let Some((entity, parent_global)) = pending.pop() {
        if let Some(global) = world.get_mut::<GlobalTransform>(entity) {
            global.0 = parent_global;
        } else if world.contains(entity) {
            let _ = world.insert(entity, GlobalTransform(parent_global));
        }

        let children: Vec<_> = world
            .get::<Children>(entity)
            .map_or_else(Vec::new, |children| children.0.clone());
        for child in children {
            let Some(local) = world.get::<Transform>(child).copied() else {
                continue;
            };
            pending.push((child, Transform::combine(parent_global, local)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GlobalTransform, Scene, propagate_transforms};
    use extrem_ecs::World;
    use extrem_math::{Transform, Vec3};

    #[test]
    fn child_global_transform_inherits_parent_translation_and_scale() {
        let mut world = World::new();
        let mut scene = Scene::new("test");
        let parent = scene
            .spawn_entity(
                &mut world,
                "parent",
                Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
            )
            .expect("parent");
        world
            .get_mut::<Transform>(parent)
            .expect("parent transform")
            .scale = Vec3::new(2.0, 2.0, 2.0);
        let child = scene
            .spawn_child(
                &mut world,
                parent,
                "child",
                Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
            )
            .expect("child");

        propagate_transforms(&mut world);

        assert_eq!(
            world
                .get::<GlobalTransform>(child)
                .expect("global")
                .0
                .translation,
            Vec3::new(12.0, 0.0, 0.0)
        );
    }
}
