use extrem_ecs::{Entity, World, WorldError};
use extrem_math::{Mat4, Transform, Vec3};
use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

const MAX_SCENE_NODES: usize = 1_000_000;
const MAX_SCENE_DEPTH: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name(pub String);

impl From<&str> for Name {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Velocity(pub Vec3);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Visibility(pub bool);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parent(pub Entity);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Children(pub Vec<Entity>);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlobalTransform(pub Transform);

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
        let safe_aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            1.0
        };
        match self {
            Self::Perspective {
                fov_y_radians,
                near,
                far,
            } => {
                let fov = if fov_y_radians.is_finite()
                    && fov_y_radians > 0.0
                    && fov_y_radians < std::f32::consts::PI
                {
                    fov_y_radians
                } else {
                    std::f32::consts::FRAC_PI_3
                };
                let safe_near = if near.is_finite() && near > 0.0 {
                    near
                } else {
                    0.1
                };
                let safe_far = if far.is_finite() && far > safe_near {
                    far
                } else {
                    safe_near + 10_000.0
                };
                Mat4::perspective(fov, safe_aspect, safe_near, safe_far)
            }
            Self::Orthographic {
                width,
                height,
                near,
                far,
            } => {
                let safe_width = if width.is_finite() && width > 0.0 {
                    width
                } else {
                    1.0
                };
                let safe_height = if height.is_finite() && height > 0.0 {
                    height
                } else {
                    1.0
                };
                let safe_near = if near.is_finite() { near } else { -1.0 };
                let safe_far = if far.is_finite() && far > safe_near {
                    far
                } else {
                    safe_near + 2.0
                };
                Mat4::orthographic(safe_width, safe_height, safe_near, safe_far)
            }
        }
    }
}

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
    pub fn view_matrix(transform: Transform) -> Mat4 {
        let inv_rot = transform.rotation.inverse().to_mat4();
        let inv_trans = Mat4::translation(transform.translation * -1.0);
        inv_rot.multiply(inv_trans)
    }

    pub fn view_projection(self, transform: Transform, aspect: f32) -> Mat4 {
        self.projection
            .matrix(aspect)
            .multiply(Self::view_matrix(transform))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum HierarchyError {
    World(WorldError),
    SelfParent(Entity),
    CycleDetected { child: Entity, parent: Entity },
    InconsistentState(String),
}

impl fmt::Display for HierarchyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::World(err) => err.fmt(formatter),
            Self::SelfParent(entity) => write!(formatter, "{entity} cannot be its own parent"),
            Self::CycleDetected { child, parent } => {
                write!(
                    formatter,
                    "reparenting {child} under {parent} creates or encounters a cycle"
                )
            }
            Self::InconsistentState(msg) => {
                write!(formatter, "hierarchy state inconsistent: {msg}")
            }
        }
    }
}

impl std::error::Error for HierarchyError {}

impl From<WorldError> for HierarchyError {
    fn from(err: WorldError) -> Self {
        Self::World(err)
    }
}

/// Sets `parent` for `child`, updating both relationship directions atomically at API level.
pub fn set_parent(world: &mut World, child: Entity, parent: Entity) -> Result<(), HierarchyError> {
    if !world.contains(child) {
        return Err(WorldError::EntityNotFound(child).into());
    }
    if !world.contains(parent) {
        return Err(WorldError::EntityNotFound(parent).into());
    }
    if child == parent {
        return Err(HierarchyError::SelfParent(child));
    }

    // Existing data may already be corrupt. Never follow Parent links without a visited set.
    let mut visited = HashSet::new();
    let mut current = Some(parent);
    while let Some(curr) = current {
        if !visited.insert(curr) || curr == child {
            return Err(HierarchyError::CycleDetected { child, parent });
        }
        current = world.get::<Parent>(curr).map(|p| p.0);
    }

    detach(world, child)?;
    world.insert(child, Parent(parent))?;
    if let Some(children) = world.get_mut::<Children>(parent) {
        if !children.0.contains(&child) {
            children.0.push(child);
        }
    } else {
        world.insert(parent, Children(vec![child]))?;
    }
    Ok(())
}

pub fn detach(world: &mut World, child: Entity) -> Result<bool, HierarchyError> {
    if !world.contains(child) {
        return Err(WorldError::EntityNotFound(child).into());
    }

    if let Some(Parent(old_parent)) = world.remove::<Parent>(child)? {
        if let Some(children) = world.get_mut::<Children>(old_parent) {
            children.0.retain(|candidate| *candidate != child);
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Despawns an entity and every reachable descendant. Corrupt cycles terminate safely.
pub fn despawn_recursive(world: &mut World, entity: Entity) -> Result<(), HierarchyError> {
    if !world.contains(entity) {
        return Err(WorldError::EntityNotFound(entity).into());
    }

    detach(world, entity)?;
    let mut visited = HashSet::new();
    let mut to_despawn = Vec::new();
    let mut stack = vec![entity];

    while let Some(current) = stack.pop() {
        if !world.contains(current) || !visited.insert(current) {
            continue;
        }
        to_despawn.push(current);
        if let Some(children) = world.get::<Children>(current) {
            stack.extend(children.0.iter().copied());
        }
    }

    for current in to_despawn.into_iter().rev() {
        // Every entity was checked alive above. If duplicated/corrupt edges existed, visited removed them.
        if world.contains(current) {
            world.despawn(current)?;
        }
    }
    Ok(())
}

/// Validates both Parent→Children and Children→Parent edges and rejects cycles/duplicates.
pub fn validate_hierarchy(world: &World) -> Result<(), HierarchyError> {
    for (entity, parent) in world.iter::<Parent>() {
        let parent_entity = parent.0;
        if !world.contains(parent_entity) {
            return Err(WorldError::EntityNotFound(parent_entity).into());
        }
        if entity == parent_entity {
            return Err(HierarchyError::SelfParent(entity));
        }
        let parent_has_child = world
            .get::<Children>(parent_entity)
            .is_some_and(|children| children.0.contains(&entity));
        if !parent_has_child {
            return Err(HierarchyError::InconsistentState(format!(
                "{entity} has Parent({parent_entity}), but the parent does not reference it"
            )));
        }

        let mut visited = HashSet::new();
        visited.insert(entity);
        let mut current = Some(parent_entity);
        while let Some(candidate) = current {
            if !visited.insert(candidate) {
                return Err(HierarchyError::CycleDetected {
                    child: entity,
                    parent: parent_entity,
                });
            }
            current = world.get::<Parent>(candidate).map(|p| p.0);
        }
    }

    for (parent, children) in world.iter::<Children>() {
        let mut unique = HashSet::new();
        for child in &children.0 {
            if !world.contains(*child) {
                return Err(WorldError::EntityNotFound(*child).into());
            }
            if !unique.insert(*child) {
                return Err(HierarchyError::InconsistentState(format!(
                    "{parent} contains duplicate child {child}"
                )));
            }
            if world.get::<Parent>(*child) != Some(&Parent(parent)) {
                return Err(HierarchyError::InconsistentState(format!(
                    "{parent} references {child}, but the child's Parent component disagrees"
                )));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneDocument {
    pub format_version: u32,
    pub name: String,
    pub roots: Vec<SceneNode>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneNode {
    pub name: String,
    pub transform: Transform,
    pub visible: bool,
    pub children: Vec<Self>,
}

#[derive(Debug)]
pub enum SceneFormatError {
    Encode(String),
    Decode(String),
    Invalid(String),
}

impl fmt::Display for SceneFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(message) => write!(formatter, "scene encoding failed: {message}"),
            Self::Decode(message) => write!(formatter, "scene decoding failed: {message}"),
            Self::Invalid(message) => write!(formatter, "scene validation failed: {message}"),
        }
    }
}

impl std::error::Error for SceneFormatError {}

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
        if let Err(error) = set_parent(world, entity, parent) {
            let _ = world.despawn(entity);
            return Err(match error {
                HierarchyError::World(world_error) => world_error,
                _ => WorldError::EntityNotFound(entity),
            });
        }
        Ok(entity)
    }

    pub fn document(&self, world: &World) -> SceneDocument {
        let mut visited = HashSet::new();
        SceneDocument {
            format_version: 1,
            name: self.name.clone(),
            roots: self
                .roots
                .iter()
                .filter_map(|entity| snapshot_node(world, *entity, 0, &mut visited))
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
        document.validate()?;
        document.instantiate(world).map_err(|error| {
            SceneFormatError::Decode(format!("could not instantiate scene: {error}"))
        })
    }
}

impl SceneDocument {
    pub fn validate(&self) -> Result<(), SceneFormatError> {
        if self.format_version != 1 {
            return Err(SceneFormatError::Invalid(format!(
                "unsupported scene format version {}",
                self.format_version
            )));
        }

        let mut count = 0usize;
        let mut stack: Vec<(&SceneNode, usize)> = self.roots.iter().map(|node| (node, 0)).collect();
        while let Some((node, depth)) = stack.pop() {
            count = count.saturating_add(1);
            if count > MAX_SCENE_NODES {
                return Err(SceneFormatError::Invalid(
                    "scene exceeds node limit".to_owned(),
                ));
            }
            if depth > MAX_SCENE_DEPTH {
                return Err(SceneFormatError::Invalid(
                    "scene exceeds hierarchy depth limit".to_owned(),
                ));
            }
            if !node.transform.is_valid() {
                return Err(SceneFormatError::Invalid(format!(
                    "node '{}' contains an invalid transform",
                    node.name
                )));
            }
            stack.extend(node.children.iter().map(|child| (child, depth + 1)));
        }
        Ok(())
    }

    pub fn instantiate(&self, world: &mut World) -> Result<Scene, WorldError> {
        let mut scene = Scene::new(&self.name);
        for node in &self.roots {
            instantiate_node(&mut scene, world, None, node)?;
        }
        Ok(scene)
    }
}

fn snapshot_node(
    world: &World,
    entity: Entity,
    depth: usize,
    visited: &mut HashSet<Entity>,
) -> Option<SceneNode> {
    if depth > MAX_SCENE_DEPTH || !visited.insert(entity) {
        return None;
    }
    let name = world.get::<Name>(entity)?.0.clone();
    let transform = *world.get::<Transform>(entity)?;
    let visible = world.get::<Visibility>(entity).is_none_or(|value| value.0);
    let children = world
        .get::<Children>(entity)
        .map(|children| {
            children
                .0
                .iter()
                .filter_map(|child| snapshot_node(world, *child, depth + 1, visited))
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

/// Recomputes world transforms from roots. Corrupt cycles terminate through the visited set.
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
    let mut visited = HashSet::new();
    while let Some((entity, parent_global)) = pending.pop() {
        if !visited.insert(entity) {
            continue;
        }
        if let Some(global) = world.get_mut::<GlobalTransform>(entity) {
            global.0 = parent_global;
        } else if world.contains(entity) {
            let _ = world.insert(entity, GlobalTransform(parent_global));
        }
        let children: Vec<_> = world
            .get::<Children>(entity)
            .map_or_else(Vec::new, |children| children.0.clone());
        for child in children {
            if let Some(local) = world.get::<Transform>(child).copied() {
                pending.push((child, Transform::combine(parent_global, local)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Children, GlobalTransform, HierarchyError, Parent, Scene, despawn_recursive, detach,
        propagate_transforms, set_parent, validate_hierarchy,
    };
    use extrem_ecs::World;
    use extrem_math::{Quat, Transform, Vec3};

    #[test]
    fn child_global_transform_inherits_parent_rotation_translation_and_scale() {
        let mut world = World::new();
        let mut scene = Scene::new("test");
        let parent = scene
            .spawn_entity(
                &mut world,
                "parent",
                Transform {
                    translation: Vec3::new(10.0, 0.0, 0.0),
                    rotation: Quat::from_axis_angle(Vec3::Y, std::f32::consts::FRAC_PI_2),
                    scale: Vec3::new(2.0, 2.0, 2.0),
                },
            )
            .expect("parent");
        let child = scene
            .spawn_child(
                &mut world,
                parent,
                "child",
                Transform::from_translation(Vec3::new(0.0, 0.0, 1.0)),
            )
            .expect("child");
        propagate_transforms(&mut world);
        let global = world.get::<GlobalTransform>(child).expect("global").0;
        assert!((global.translation.x - 12.0).abs() < 1e-5);
        assert!(global.translation.z.abs() < 1e-5);
    }

    #[test]
    fn camera_view_matrix_includes_inverse_rotation() {
        let cam_transform = Transform {
            translation: Vec3::new(10.0, 0.0, 0.0),
            rotation: Quat::from_axis_angle(Vec3::Y, std::f32::consts::FRAC_PI_2),
            scale: Vec3::ONE,
        };
        let camera_pt =
            super::Camera::view_matrix(cam_transform).transform_point3(Vec3::new(10.0, 0.0, -5.0));
        assert!((camera_pt.x - 5.0).abs() < 1e-4);
        assert!(camera_pt.y.abs() < 1e-4);
        assert!(camera_pt.z.abs() < 1e-4);
    }

    #[test]
    fn hierarchy_cycle_prevention_and_invariants() {
        let mut world = World::new();
        let e1 = world.spawn(Transform::IDENTITY);
        let e2 = world.spawn(Transform::IDENTITY);
        set_parent(&mut world, e2, e1).expect("e2 child of e1");
        assert_eq!(world.get::<Parent>(e2), Some(&Parent(e1)));
        assert_eq!(world.get::<Children>(e1), Some(&Children(vec![e2])));
        assert_eq!(
            set_parent(&mut world, e1, e1),
            Err(HierarchyError::SelfParent(e1))
        );
        assert_eq!(
            set_parent(&mut world, e1, e2),
            Err(HierarchyError::CycleDetected {
                child: e1,
                parent: e2
            })
        );
        validate_hierarchy(&world).expect("hierarchy valid");
        detach(&mut world, e2).expect("detach");
        assert_eq!(world.get::<Parent>(e2), None);
    }

    #[test]
    fn corrupt_cycle_does_not_loop_in_despawn_recursive() {
        let mut world = World::new();
        let a = world.spawn(Transform::IDENTITY);
        let b = world.spawn(Transform::IDENTITY);
        // Deliberately bypass the safe API to model malformed/deserialized state.
        world.insert(a, Children(vec![b])).expect("a children");
        world.insert(b, Children(vec![a])).expect("b children");
        world.insert(a, Parent(b)).expect("a parent");
        world.insert(b, Parent(a)).expect("b parent");

        despawn_recursive(&mut world, a).expect("bounded despawn");
        assert!(!world.contains(a));
        assert!(!world.contains(b));
    }

    #[test]
    fn validator_rejects_reverse_edge_mismatch_and_duplicates() {
        let mut world = World::new();
        let parent = world.spawn(Transform::IDENTITY);
        let child = world.spawn(Transform::IDENTITY);
        world
            .insert(parent, Children(vec![child, child]))
            .expect("children");
        world.insert(child, Parent(parent)).expect("parent");
        assert!(validate_hierarchy(&world).is_err());
    }

    #[test]
    fn despawn_recursive_cleans_up_descendants() {
        let mut world = World::new();
        let e1 = world.spawn(Transform::IDENTITY);
        let e2 = world.spawn(Transform::IDENTITY);
        let e3 = world.spawn(Transform::IDENTITY);
        set_parent(&mut world, e2, e1).expect("set parent e2");
        set_parent(&mut world, e3, e2).expect("set parent e3");
        despawn_recursive(&mut world, e1).expect("despawn recursive");
        assert!(!world.contains(e1));
        assert!(!world.contains(e2));
        assert!(!world.contains(e3));
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
        assert_eq!(
            restored_world
                .get::<Children>(restored.roots[0])
                .map(|children| children.0.len()),
            Some(1)
        );
    }
}
