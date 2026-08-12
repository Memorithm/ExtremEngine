use extrem_ecs::{Entity, World, WorldError};
use extrem_math::{Mat4, Transform, Vec3};
use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};
use std::fmt;

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

/// Projection parameters for a camera component.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum Projection {
    Perspective {
        fov_y_radians: f32,
        near: f32,
        far: f32,
    },
    Orthographic {
        width: f32,
        height: f32,
        near: f32,
        far: f32,
    },
}

impl Default for Projection {
    fn default() -> Self {
        Self::Perspective {
            fov_y_radians: std::f32::consts::FRAC_PI_3,
            near: 0.1,
            far: 10_000.0,
        }
    }
}

impl Projection {
    pub fn matrix(self, aspect: f32) -> Mat4 {
        match self {
            Self::Perspective {
                fov_y_radians,
                near,
                far,
            } => Mat4::perspective(fov_y_radians, aspect.max(0.000_1), near, far),
            Self::Orthographic {
                width,
                height,
                near,
                far,
            } => Mat4::orthographic(width.max(0.000_1), height.max(0.000_1), near, far),
        }
    }
}

/// Camera marker and projection settings.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Camera {
    pub active: bool,
    pub projection: Projection,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            active: true,
            projection: Projection::default(),
        }
    }
}

impl Camera {
    pub fn view_projection(self, transform: Transform, aspect: f32) -> Mat4 {
        self.projection
            .matrix(aspect)
            .multiply(Mat4::translation(transform.translation * -1.0))
    }
}

/// Serializable scene representation independent from runtime entity IDs.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneDocument {
    pub format_version: u32,
    pub name: String,
    pub roots: Vec<SceneNode>,
}

/// Serializable scene node used by editor and asset pipelines.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneNode {
    pub name: String,
    pub transform: Transform,
    pub visible: bool,
    pub children: Vec<Self>,
}

/// Errors produced by scene encoding and decoding.
#[derive(Debug)]
pub enum SceneFormatError {
    Encode(String),
    Decode(String),
}

impl fmt::Display for SceneFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(message) => write!(formatter, "scene encoding failed: {message}"),
            Self::Decode(message) => write!(formatter, "scene decoding failed: {message}"),
        }
    }
}

impl std::error::Error for SceneFormatError {}

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

    pub fn document(&self, world: &World) -> SceneDocument {
        SceneDocument {
            format_version: 1,
            name: self.name.clone(),
            roots: self
                .roots
                .iter()
                .filter_map(|entity| snapshot_node(world, *entity))
                .collect(),
        }
    }

    pub fn to_ron(&self, world: &World) -> Result<String, SceneFormatError> {
        ron::ser::to_string_pretty(&self.document(world), PrettyConfig::default())
            .map_err(|error| SceneFormatError::Encode(error.to_string()))
    }

    pub fn from_ron(text: &str, world: &mut World) -> Result<Self, SceneFormatError> {
        let document: SceneDocument =
            ron::from_str(text).map_err(|error| SceneFormatError::Decode(error.to_string()))?;
        document.instantiate(world).map_err(|error| {
            SceneFormatError::Decode(format!("could not instantiate scene: {error}"))
        })
    }
}

impl SceneDocument {
    pub fn instantiate(&self, world: &mut World) -> Result<Scene, WorldError> {
        let mut scene = Scene::new(&self.name);
        for node in &self.roots {
            instantiate_node(&mut scene, world, None, node)?;
        }
        Ok(scene)
    }
}

fn snapshot_node(world: &World, entity: Entity) -> Option<SceneNode> {
    let name = world.get::<Name>(entity)?.0.clone();
    let transform = *world.get::<Transform>(entity)?;
    let visible = world.get::<Visibility>(entity).is_none_or(|value| value.0);
    let children = world
        .get::<Children>(entity)
        .map(|children| {
            children
                .0
                .iter()
                .filter_map(|child| snapshot_node(world, *child))
                .collect()
        })
        .unwrap_or_default();
    Some(SceneNode {
        name,
        transform,
        visible,
        children,
    })
}

fn instantiate_node(
    scene: &mut Scene,
    world: &mut World,
    parent: Option<Entity>,
    node: &SceneNode,
) -> Result<Entity, WorldError> {
    let entity = match parent {
        Some(parent) => scene.spawn_child(world, parent, &node.name, node.transform)?,
        None => scene.spawn_entity(world, &node.name, node.transform)?,
    };
    world.insert(entity, Visibility(node.visible))?;
    for child in &node.children {
        instantiate_node(scene, world, Some(entity), child)?;
    }
    Ok(entity)
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
    use super::{Children, GlobalTransform, Scene, propagate_transforms};
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

    #[test]
    fn scene_round_trip_preserves_hierarchy() {
        let mut world = World::new();
        let mut scene = Scene::new("round-trip");
        let parent = scene
            .spawn_entity(&mut world, "parent", Transform::IDENTITY)
            .expect("parent");
        scene
            .spawn_child(
                &mut world,
                parent,
                "child",
                Transform::from_translation(Vec3::new(2.0, 0.0, 0.0)),
            )
            .expect("child");

        let encoded = scene.to_ron(&world).expect("encode");
        let mut restored_world = World::new();
        let restored = Scene::from_ron(&encoded, &mut restored_world).expect("decode");

        assert_eq!(restored.name, "round-trip");
        assert_eq!(restored.roots.len(), 1);
        let restored_parent = restored.roots[0];
        assert_eq!(
            restored_world
                .get::<Children>(restored_parent)
                .map(|c| c.0.len()),
            Some(1)
        );
    }
}
