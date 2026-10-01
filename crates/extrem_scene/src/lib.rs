use extrem_ecs::{Entity, World, WorldError};
use extrem_math::{Mat4, Transform, Vec3};
use ron::ser::PrettyConfig;
use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

mod propagation;
pub use propagation::{TransformPropagationStats, TransformPropagator, propagate_transforms};
mod validation;
pub use validation::{HierarchyValidationStats, HierarchyValidator, validate_hierarchy};

const MAX_SCENE_NODES: usize = 1_000_000;
const MAX_SCENE_DEPTH: usize = 256;
const MAX_SCENE_BYTES: usize = 8 * 1024 * 1024;
const MAX_SCENE_NAME_BYTES: usize = 4 * 1024;

/// Hard and caller-reducible limits for untrusted scene loading.
///
/// The limits cannot be raised above the engine's hard safety envelope. This
/// keeps a configured loader from accidentally disabling the boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneLoadLimits {
    max_bytes: usize,
    max_name_bytes: usize,
    max_nodes: usize,
    max_depth: usize,
}

impl Default for SceneLoadLimits {
    fn default() -> Self {
        Self {
            max_bytes: MAX_SCENE_BYTES,
            max_name_bytes: MAX_SCENE_NAME_BYTES,
            max_nodes: MAX_SCENE_NODES,
            max_depth: MAX_SCENE_DEPTH,
        }
    }
}

impl SceneLoadLimits {
    pub fn new(
        max_bytes: usize,
        max_name_bytes: usize,
        max_nodes: usize,
        max_depth: usize,
    ) -> Result<Self, SceneFormatError> {
        if max_bytes == 0 || max_bytes > MAX_SCENE_BYTES {
            return Err(SceneFormatError::Invalid(format!(
                "scene byte limit must be in 1..={MAX_SCENE_BYTES}"
            )));
        }
        if max_name_bytes == 0 || max_name_bytes > MAX_SCENE_NAME_BYTES {
            return Err(SceneFormatError::Invalid(format!(
                "scene name limit must be in 1..={MAX_SCENE_NAME_BYTES}"
            )));
        }
        if max_nodes == 0 || max_nodes > MAX_SCENE_NODES {
            return Err(SceneFormatError::Invalid(format!(
                "scene node limit must be in 1..={MAX_SCENE_NODES}"
            )));
        }
        if max_depth > MAX_SCENE_DEPTH {
            return Err(SceneFormatError::Invalid(format!(
                "scene depth limit must be in 0..={MAX_SCENE_DEPTH}"
            )));
        }
        Ok(Self {
            max_bytes,
            max_name_bytes,
            max_nodes,
            max_depth,
        })
    }

    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }

    pub const fn max_name_bytes(self) -> usize {
        self.max_name_bytes
    }

    pub const fn max_nodes(self) -> usize {
        self.max_nodes
    }

    pub const fn max_depth(self) -> usize {
        self.max_depth
    }
}

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

#[derive(Debug, PartialEq, Eq)]
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

#[derive(Debug, PartialEq, Eq)]
pub enum SceneInstantiationError {
    Format(SceneFormatError),
    World(WorldError),
    Capacity,
}

impl fmt::Display for SceneInstantiationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(error) => error.fmt(formatter),
            Self::World(error) => error.fmt(formatter),
            Self::Capacity => write!(formatter, "scene instantiation capacity exhausted"),
        }
    }
}

impl std::error::Error for SceneInstantiationError {}

impl From<SceneFormatError> for SceneInstantiationError {
    fn from(error: SceneFormatError) -> Self {
        Self::Format(error)
    }
}

impl From<WorldError> for SceneInstantiationError {
    fn from(error: WorldError) -> Self {
        Self::World(error)
    }
}

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
        Self::from_ron_with_limits(text, world, SceneLoadLimits::default())
    }

    pub fn from_ron_with_limits(
        text: &str,
        world: &mut World,
        limits: SceneLoadLimits,
    ) -> Result<Self, SceneFormatError> {
        if text.len() > limits.max_bytes {
            return Err(SceneFormatError::Invalid(
                "scene exceeds encoded byte limit".to_owned(),
            ));
        }

        let ron_recursion_limit = limits.max_depth.saturating_mul(2).saturating_add(16);
        let options = ron::Options::default().with_recursion_limit(ron_recursion_limit);
        let mut state = SceneReadState::default();
        let document = options
            .from_str_seed(
                text,
                SceneDocumentSeed {
                    limits: &limits,
                    state: &mut state,
                },
            )
            .map_err(|error| SceneFormatError::Decode(error.to_string()))?;
        document.validate_with_limits(limits)?;
        document
            .instantiate_with_limits(world, limits)
            .map_err(|error| {
                SceneFormatError::Decode(format!("could not instantiate scene: {error}"))
            })
    }
}

impl SceneDocument {
    pub fn validate(&self) -> Result<(), SceneFormatError> {
        self.validate_with_limits(SceneLoadLimits::default())
    }

    pub fn validate_with_limits(&self, limits: SceneLoadLimits) -> Result<(), SceneFormatError> {
        self.validated_node_count(limits).map(|_| ())
    }

    fn validated_node_count(&self, limits: SceneLoadLimits) -> Result<usize, SceneFormatError> {
        if self.format_version != 1 {
            return Err(SceneFormatError::Invalid(format!(
                "unsupported scene format version {}",
                self.format_version
            )));
        }
        if self.name.len() > limits.max_name_bytes {
            return Err(SceneFormatError::Invalid(
                "scene name exceeds byte limit".to_owned(),
            ));
        }
        if self.roots.len() > limits.max_nodes {
            return Err(SceneFormatError::Invalid(
                "scene exceeds node limit".to_owned(),
            ));
        }

        let mut count = 0usize;
        let mut stack = Vec::new();
        stack
            .try_reserve_exact(self.roots.len())
            .map_err(|_| SceneFormatError::Invalid("scene capacity exhausted".to_owned()))?;
        stack.extend(self.roots.iter().map(|node| (node, 0usize)));
        while let Some((node, depth)) = stack.pop() {
            count = count
                .checked_add(1)
                .ok_or_else(|| SceneFormatError::Invalid("scene node count overflow".to_owned()))?;
            if count > limits.max_nodes {
                return Err(SceneFormatError::Invalid(
                    "scene exceeds node limit".to_owned(),
                ));
            }
            if depth > limits.max_depth {
                return Err(SceneFormatError::Invalid(
                    "scene exceeds hierarchy depth limit".to_owned(),
                ));
            }
            if node.name.len() > limits.max_name_bytes {
                return Err(SceneFormatError::Invalid(
                    "node name exceeds byte limit".to_owned(),
                ));
            }
            if !node.transform.is_valid() {
                return Err(SceneFormatError::Invalid(format!(
                    "node '{}' contains an invalid transform",
                    node.name
                )));
            }
            stack.try_reserve(node.children.len()).map_err(|_| {
                SceneFormatError::Invalid("scene traversal capacity exhausted".to_owned())
            })?;
            stack.extend(node.children.iter().map(|child| (child, depth + 1)));
        }
        Ok(count)
    }

    pub fn instantiate(&self, world: &mut World) -> Result<Scene, SceneInstantiationError> {
        self.instantiate_with_limits(world, SceneLoadLimits::default())
    }

    pub fn instantiate_with_limits(
        &self,
        world: &mut World,
        limits: SceneLoadLimits,
    ) -> Result<Scene, SceneInstantiationError> {
        let node_count = self.validated_node_count(limits)?;
        let name = clone_string_fallibly(&self.name)?;
        let plan = build_instantiation_plan(self, node_count)?;
        instantiate_plan(world, name, plan, self.roots.len(), node_count)
    }
}

#[derive(Default)]
struct SceneReadState {
    nodes: usize,
    decoded_name_bytes: usize,
}

impl SceneReadState {
    fn begin_node<E: de::Error>(&mut self, limits: &SceneLoadLimits) -> Result<(), E> {
        self.nodes = self
            .nodes
            .checked_add(1)
            .ok_or_else(|| E::custom("scene node count overflow"))?;
        if self.nodes > limits.max_nodes {
            return Err(E::custom("scene exceeds node limit while decoding"));
        }
        Ok(())
    }

    fn register_name<E: de::Error>(
        &mut self,
        len: usize,
        limits: &SceneLoadLimits,
    ) -> Result<(), E> {
        if len > limits.max_name_bytes {
            return Err(E::custom("scene name exceeds byte limit while decoding"));
        }
        self.decoded_name_bytes = self
            .decoded_name_bytes
            .checked_add(len)
            .ok_or_else(|| E::custom("decoded scene name budget overflow"))?;
        if self.decoded_name_bytes > limits.max_bytes {
            return Err(E::custom("decoded scene names exceed load budget"));
        }
        Ok(())
    }
}

struct BoundedStringSeed<'a> {
    limits: &'a SceneLoadLimits,
    state: &'a mut SceneReadState,
}

impl<'de> DeserializeSeed<'de> for BoundedStringSeed<'_> {
    type Value = String;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_string(BoundedStringVisitor {
            limits: self.limits,
            state: self.state,
        })
    }
}

struct BoundedStringVisitor<'a> {
    limits: &'a SceneLoadLimits,
    state: &'a mut SceneReadState,
}

impl<'de> Visitor<'de> for BoundedStringVisitor<'_> {
    type Value = String;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "a scene name no longer than {} bytes",
            self.limits.max_name_bytes
        )
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_str(value)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.state.register_name::<E>(value.len(), self.limits)?;
        let mut owned = String::new();
        owned
            .try_reserve_exact(value.len())
            .map_err(|_| E::custom("scene name allocation failed"))?;
        owned.push_str(value);
        Ok(owned)
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.state.register_name::<E>(value.len(), self.limits)?;
        Ok(value)
    }
}

struct SceneDocumentSeed<'a> {
    limits: &'a SceneLoadLimits,
    state: &'a mut SceneReadState,
}

impl<'de> DeserializeSeed<'de> for SceneDocumentSeed<'_> {
    type Value = SceneDocument;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_struct(
            "SceneDocument",
            &["format_version", "name", "roots"],
            SceneDocumentVisitor {
                limits: self.limits,
                state: self.state,
            },
        )
    }
}

#[derive(Deserialize)]
#[serde(field_identifier)]
enum SceneDocumentField {
    #[serde(rename = "format_version")]
    FormatVersion,
    #[serde(rename = "name")]
    Name,
    #[serde(rename = "roots")]
    Roots,
    #[serde(other)]
    Ignore,
}

struct SceneDocumentVisitor<'a> {
    limits: &'a SceneLoadLimits,
    state: &'a mut SceneReadState,
}

impl<'de> Visitor<'de> for SceneDocumentVisitor<'_> {
    type Value = SceneDocument;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bounded SceneDocument")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let format_version = sequence
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;
        let name = sequence
            .next_element_seed(BoundedStringSeed {
                limits: self.limits,
                state: self.state,
            })?
            .ok_or_else(|| de::Error::invalid_length(1, &self))?;
        let roots = sequence
            .next_element_seed(SceneNodeVecSeed {
                limits: self.limits,
                state: self.state,
                depth: 0,
            })?
            .ok_or_else(|| de::Error::invalid_length(2, &self))?;
        Ok(SceneDocument {
            format_version,
            name,
            roots,
        })
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut format_version = None;
        let mut name = None;
        let mut roots = None;
        while let Some(field) = map.next_key()? {
            match field {
                SceneDocumentField::FormatVersion => {
                    if format_version.is_some() {
                        return Err(de::Error::duplicate_field("format_version"));
                    }
                    format_version = Some(map.next_value()?);
                }
                SceneDocumentField::Name => {
                    if name.is_some() {
                        return Err(de::Error::duplicate_field("name"));
                    }
                    name = Some(map.next_value_seed(BoundedStringSeed {
                        limits: self.limits,
                        state: self.state,
                    })?);
                }
                SceneDocumentField::Roots => {
                    if roots.is_some() {
                        return Err(de::Error::duplicate_field("roots"));
                    }
                    roots = Some(map.next_value_seed(SceneNodeVecSeed {
                        limits: self.limits,
                        state: self.state,
                        depth: 0,
                    })?);
                }
                SceneDocumentField::Ignore => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(SceneDocument {
            format_version: format_version
                .ok_or_else(|| de::Error::missing_field("format_version"))?,
            name: name.ok_or_else(|| de::Error::missing_field("name"))?,
            roots: roots.ok_or_else(|| de::Error::missing_field("roots"))?,
        })
    }
}

struct SceneNodeVecSeed<'a> {
    limits: &'a SceneLoadLimits,
    state: &'a mut SceneReadState,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for SceneNodeVecSeed<'_> {
    type Value = Vec<SceneNode>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(SceneNodeVecVisitor {
            limits: self.limits,
            state: self.state,
            depth: self.depth,
        })
    }
}

struct SceneNodeVecVisitor<'a> {
    limits: &'a SceneLoadLimits,
    state: &'a mut SceneReadState,
    depth: usize,
}

impl<'de> Visitor<'de> for SceneNodeVecVisitor<'_> {
    type Value = Vec<SceneNode>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bounded sequence of scene nodes")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut nodes = Vec::new();
        while let Some(node) = sequence.next_element_seed(SceneNodeSeed {
            limits: self.limits,
            state: self.state,
            depth: self.depth,
        })? {
            nodes
                .try_reserve_exact(1)
                .map_err(|_| de::Error::custom("scene node allocation failed"))?;
            nodes.push(node);
        }
        Ok(nodes)
    }
}

struct SceneNodeSeed<'a> {
    limits: &'a SceneLoadLimits,
    state: &'a mut SceneReadState,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for SceneNodeSeed<'_> {
    type Value = SceneNode;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if self.depth > self.limits.max_depth {
            return Err(de::Error::custom(
                "scene exceeds hierarchy depth limit while decoding",
            ));
        }
        self.state.begin_node::<D::Error>(self.limits)?;
        deserializer.deserialize_struct(
            "SceneNode",
            &["name", "transform", "visible", "children"],
            SceneNodeVisitor {
                limits: self.limits,
                state: self.state,
                depth: self.depth,
            },
        )
    }
}

#[derive(Deserialize)]
#[serde(field_identifier)]
enum SceneNodeField {
    #[serde(rename = "name")]
    Name,
    #[serde(rename = "transform")]
    Transform,
    #[serde(rename = "visible")]
    Visible,
    #[serde(rename = "children")]
    Children,
    #[serde(other)]
    Ignore,
}

struct SceneNodeVisitor<'a> {
    limits: &'a SceneLoadLimits,
    state: &'a mut SceneReadState,
    depth: usize,
}

impl<'de> Visitor<'de> for SceneNodeVisitor<'_> {
    type Value = SceneNode;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bounded SceneNode")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let name = sequence
            .next_element_seed(BoundedStringSeed {
                limits: self.limits,
                state: self.state,
            })?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;
        let transform = sequence
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(1, &self))?;
        let visible = sequence
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(2, &self))?;
        let children = sequence
            .next_element_seed(SceneNodeVecSeed {
                limits: self.limits,
                state: self.state,
                depth: self.depth.saturating_add(1),
            })?
            .ok_or_else(|| de::Error::invalid_length(3, &self))?;
        Ok(SceneNode {
            name,
            transform,
            visible,
            children,
        })
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut name = None;
        let mut transform = None;
        let mut visible = None;
        let mut children = None;
        while let Some(field) = map.next_key()? {
            match field {
                SceneNodeField::Name => {
                    if name.is_some() {
                        return Err(de::Error::duplicate_field("name"));
                    }
                    name = Some(map.next_value_seed(BoundedStringSeed {
                        limits: self.limits,
                        state: self.state,
                    })?);
                }
                SceneNodeField::Transform => {
                    if transform.is_some() {
                        return Err(de::Error::duplicate_field("transform"));
                    }
                    transform = Some(map.next_value()?);
                }
                SceneNodeField::Visible => {
                    if visible.is_some() {
                        return Err(de::Error::duplicate_field("visible"));
                    }
                    visible = Some(map.next_value()?);
                }
                SceneNodeField::Children => {
                    if children.is_some() {
                        return Err(de::Error::duplicate_field("children"));
                    }
                    children = Some(map.next_value_seed(SceneNodeVecSeed {
                        limits: self.limits,
                        state: self.state,
                        depth: self.depth.saturating_add(1),
                    })?);
                }
                SceneNodeField::Ignore => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(SceneNode {
            name: name.ok_or_else(|| de::Error::missing_field("name"))?,
            transform: transform.ok_or_else(|| de::Error::missing_field("transform"))?,
            visible: visible.ok_or_else(|| de::Error::missing_field("visible"))?,
            children: children.ok_or_else(|| de::Error::missing_field("children"))?,
        })
    }
}

struct PlannedSceneNode {
    parent_index: Option<usize>,
    name: String,
    transform: Transform,
    visible: bool,
    children: Vec<Entity>,
}

fn build_instantiation_plan(
    document: &SceneDocument,
    node_count: usize,
) -> Result<Vec<PlannedSceneNode>, SceneInstantiationError> {
    let mut pending = Vec::new();
    pending
        .try_reserve_exact(node_count)
        .map_err(|_| SceneInstantiationError::Capacity)?;
    for node in document.roots.iter().rev() {
        pending.push((None, node));
    }

    let mut plan = Vec::new();
    plan.try_reserve_exact(node_count)
        .map_err(|_| SceneInstantiationError::Capacity)?;
    while let Some((parent_index, node)) = pending.pop() {
        let plan_index = plan.len();
        let mut children = Vec::new();
        children
            .try_reserve_exact(node.children.len())
            .map_err(|_| SceneInstantiationError::Capacity)?;
        plan.push(PlannedSceneNode {
            parent_index,
            name: clone_string_fallibly(&node.name)?,
            transform: node.transform,
            visible: node.visible,
            children,
        });
        for child in node.children.iter().rev() {
            pending.push((Some(plan_index), child));
        }
    }
    Ok(plan)
}

fn clone_string_fallibly(value: &str) -> Result<String, SceneInstantiationError> {
    let mut cloned = String::new();
    cloned
        .try_reserve_exact(value.len())
        .map_err(|_| SceneInstantiationError::Capacity)?;
    cloned.push_str(value);
    Ok(cloned)
}

fn instantiate_plan(
    world: &mut World,
    name: String,
    plan: Vec<PlannedSceneNode>,
    root_count: usize,
    spawn_limit: usize,
) -> Result<Scene, SceneInstantiationError> {
    let mut entities = Vec::new();
    entities
        .try_reserve_exact(plan.len())
        .map_err(|_| SceneInstantiationError::Capacity)?;
    let mut roots = Vec::new();
    roots
        .try_reserve_exact(root_count)
        .map_err(|_| SceneInstantiationError::Capacity)?;

    for planned in plan {
        if entities.len() >= spawn_limit {
            rollback_created_entities(world, &entities);
            return Err(SceneInstantiationError::Capacity);
        }
        let entity = match world.try_spawn_empty() {
            Ok(entity) => entity,
            Err(error) => {
                rollback_created_entities(world, &entities);
                return Err(error.into());
            }
        };
        entities.push(entity);

        let result = (|| -> Result<(), WorldError> {
            world.insert(entity, Name(planned.name))?;
            world.insert(entity, planned.transform)?;
            world.insert(entity, GlobalTransform(planned.transform))?;
            world.insert(entity, Visibility(planned.visible))?;
            world.insert(entity, Children(planned.children))?;
            if let Some(parent_index) = planned.parent_index {
                let parent = entities[parent_index];
                world.insert(entity, Parent(parent))?;
                world
                    .get_mut::<Children>(parent)
                    .ok_or(WorldError::EntityNotFound(parent))?
                    .0
                    .push(entity);
            } else {
                roots.push(entity);
            }
            Ok(())
        })();

        if let Err(error) = result {
            rollback_created_entities(world, &entities);
            return Err(error.into());
        }
    }

    Ok(Scene { name, roots })
}

fn rollback_created_entities(world: &mut World, entities: &[Entity]) {
    for entity in entities.iter().rev().copied() {
        if !world.contains(entity) {
            continue;
        }
        if let Some(Parent(parent)) = world.get::<Parent>(entity).copied() {
            if let Some(children) = world.get_mut::<Children>(parent) {
                children.0.retain(|child| *child != entity);
            }
        }
        let _ = world.despawn(entity);
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

#[cfg(test)]
mod tests {
    use super::{
        Children, GlobalTransform, HierarchyError, Name, Parent, Scene, SceneDocument,
        SceneFormatError, SceneInstantiationError, SceneLoadLimits, SceneNode, Visibility,
        build_instantiation_plan, despawn_recursive, detach, instantiate_plan,
        propagate_transforms, set_parent, validate_hierarchy,
    };
    use extrem_ecs::World;
    use extrem_math::{Quat, Transform, Vec3};

    fn node(name: &str, children: Vec<SceneNode>) -> SceneNode {
        SceneNode {
            name: name.to_owned(),
            transform: Transform::IDENTITY,
            visible: true,
            children,
        }
    }

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

    #[test]
    fn direct_instantiation_validates_before_world_mutation() {
        let document = SceneDocument {
            format_version: 2,
            name: "invalid".to_owned(),
            roots: vec![node("root", Vec::new())],
        };
        let mut world = World::new();
        let sentinel = world.spawn(Name("sentinel".to_owned()));
        let count_before = world.entity_count();

        assert!(matches!(
            document.instantiate(&mut world),
            Err(SceneInstantiationError::Format(SceneFormatError::Invalid(
                _
            )))
        ));
        assert_eq!(world.entity_count(), count_before);
        assert_eq!(
            world.get::<Name>(sentinel).map(|name| name.0.as_str()),
            Some("sentinel")
        );
    }

    #[test]
    fn flat_node_limit_is_enforced_during_decode() {
        let document = SceneDocument {
            format_version: 1,
            name: "flat".to_owned(),
            roots: vec![
                node("one", Vec::new()),
                node("two", Vec::new()),
                node("three", Vec::new()),
            ],
        };
        let encoded = ron::ser::to_string(&document).expect("encode test document");
        let limits = SceneLoadLimits::new(encoded.len(), 32, 2, 4).expect("limits");
        let mut world = World::new();

        assert!(matches!(
            Scene::from_ron_with_limits(&encoded, &mut world, limits),
            Err(SceneFormatError::Decode(message))
                if message.contains("node limit while decoding")
        ));
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn encoded_byte_limit_rejects_before_decode() {
        let limits = SceneLoadLimits::new(16, 16, 4, 4).expect("limits");
        let mut world = World::new();

        assert_eq!(
            Scene::from_ron_with_limits(
                "this input is longer than sixteen bytes",
                &mut world,
                limits
            ),
            Err(SceneFormatError::Invalid(
                "scene exceeds encoded byte limit".to_owned()
            ))
        );
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn name_limit_is_enforced_during_decode() {
        let document = SceneDocument {
            format_version: 1,
            name: "oversized".to_owned(),
            roots: Vec::new(),
        };
        let encoded = ron::ser::to_string(&document).expect("encode test document");
        let limits = SceneLoadLimits::new(encoded.len(), 4, 4, 4).expect("limits");
        let mut world = World::new();

        assert!(matches!(
            Scene::from_ron_with_limits(&encoded, &mut world, limits),
            Err(SceneFormatError::Decode(message))
                if message.contains("name exceeds byte limit while decoding")
        ));
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn direct_instantiation_enforces_depth_without_recursion() {
        let document = SceneDocument {
            format_version: 1,
            name: "deep".to_owned(),
            roots: vec![node(
                "root",
                vec![node("child", vec![node("grandchild", Vec::new())])],
            )],
        };
        let limits = SceneLoadLimits::new(1024, 32, 8, 1).expect("limits");
        let mut world = World::new();

        assert!(matches!(
            document.instantiate_with_limits(&mut world, limits),
            Err(SceneInstantiationError::Format(SceneFormatError::Invalid(message)))
                if message.contains("depth limit")
        ));
        assert_eq!(world.entity_count(), 0);
    }

    #[test]
    fn mid_instantiation_failure_rolls_back_created_entities() {
        let document = SceneDocument {
            format_version: 1,
            name: "rollback".to_owned(),
            roots: vec![node("one", Vec::new()), node("two", Vec::new())],
        };
        let plan = build_instantiation_plan(&document, 2).expect("plan");
        let mut world = World::new();
        let sentinel = world.spawn(Name("sentinel".to_owned()));

        assert_eq!(
            instantiate_plan(&mut world, document.name.clone(), plan, 2, 1),
            Err(SceneInstantiationError::Capacity)
        );
        assert_eq!(world.entity_count(), 1);
        assert_eq!(
            world.get::<Name>(sentinel).map(|name| name.0.as_str()),
            Some("sentinel")
        );
        assert!(world.get::<Visibility>(sentinel).is_none());
    }
}
