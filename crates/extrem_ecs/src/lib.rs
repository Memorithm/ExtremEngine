use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;

/// Stable generational identifier for an entity in a [`World`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Entity {
    index: u32,
    generation: u32,
}

impl Entity {
    /// Creates an entity from raw index and generation.
    pub const fn from_raw_parts(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Creates an entity with generation 1 from a raw index (primarily for test mock entity creation).
    pub const fn from_raw(index: u32) -> Self {
        Self {
            index,
            generation: 1,
        }
    }

    /// Returns the raw index of the entity.
    pub const fn index(self) -> u32 {
        self.index
    }

    /// Returns the generation of the entity.
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

impl fmt::Display for Entity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Entity({}:{})", self.index, self.generation)
    }
}

#[derive(Clone, Copy, Debug)]
struct EntitySlot {
    generation: u32,
    alive: bool,
}

#[derive(Debug)]
struct TypedStorage<T> {
    values: HashMap<Entity, T>,
}

trait Storage: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn remove_entity(&mut self, entity: Entity);
    fn len(&self) -> usize;
}

impl<T: 'static> Storage for TypedStorage<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn remove_entity(&mut self, entity: Entity) {
        self.values.remove(&entity);
    }

    fn len(&self) -> usize {
        self.values.len()
    }
}

/// Errors returned by world operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldError {
    /// The entity is not alive in the target world.
    EntityNotFound(Entity),
}

impl fmt::Display for WorldError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EntityNotFound(entity) => write!(formatter, "{entity} does not exist"),
        }
    }
}

impl std::error::Error for WorldError {}

/// The ECS container holding entities, components and global resources.
#[derive(Default)]
pub struct World {
    slots: Vec<EntitySlot>,
    free_list: Vec<u32>,
    components: HashMap<TypeId, Box<dyn Storage>>,
    resources: HashMap<TypeId, Box<dyn Any>>,
}

impl World {
    /// Creates an empty world.
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawns an entity without components.
    pub fn spawn_empty(&mut self) -> Entity {
        if let Some(index) = self.free_list.pop() {
            let slot = &mut self.slots[index as usize];
            slot.alive = true;
            Entity {
                index,
                generation: slot.generation,
            }
        } else {
            let index = self.slots.len() as u32;
            let slot = EntitySlot {
                generation: 1,
                alive: true,
            };
            self.slots.push(slot);
            Entity {
                index,
                generation: 1,
            }
        }
    }

    /// Spawns an entity and inserts its first component.
    pub fn spawn<T: 'static>(&mut self, component: T) -> Entity {
        let entity = self.spawn_empty();
        self.insert(entity, component)
            .expect("a freshly spawned entity must be alive");
        entity
    }

    /// Returns whether the entity is alive in this world with matching generation.
    pub fn contains(&self, entity: Entity) -> bool {
        let index = entity.index as usize;
        if index >= self.slots.len() {
            return false;
        }
        let slot = self.slots[index];
        slot.alive && slot.generation == entity.generation
    }

    /// Removes an entity and all of its components.
    pub fn despawn(&mut self, entity: Entity) -> Result<(), WorldError> {
        if !self.contains(entity) {
            return Err(WorldError::EntityNotFound(entity));
        }

        let slot = &mut self.slots[entity.index as usize];
        slot.alive = false;
        slot.generation = slot.generation.wrapping_add(1);
        self.free_list.push(entity.index);

        for storage in self.components.values_mut() {
            storage.remove_entity(entity);
        }
        Ok(())
    }

    /// Inserts or replaces a component on an entity.
    pub fn insert<T: 'static>(
        &mut self,
        entity: Entity,
        component: T,
    ) -> Result<Option<T>, WorldError> {
        if !self.contains(entity) {
            return Err(WorldError::EntityNotFound(entity));
        }

        let storage = self.components.entry(TypeId::of::<T>()).or_insert_with(|| {
            Box::new(TypedStorage::<T> {
                values: HashMap::new(),
            })
        });
        let typed = storage
            .as_any_mut()
            .downcast_mut::<TypedStorage<T>>()
            .expect("component storage type must match its TypeId");
        Ok(typed.values.insert(entity, component))
    }

    /// Gets an immutable component reference.
    pub fn get<T: 'static>(&self, entity: Entity) -> Option<&T> {
        if !self.contains(entity) {
            return None;
        }
        self.components
            .get(&TypeId::of::<T>())
            .and_then(|storage| storage.as_any().downcast_ref::<TypedStorage<T>>())
            .and_then(|storage| storage.values.get(&entity))
    }

    /// Gets a mutable component reference.
    pub fn get_mut<T: 'static>(&mut self, entity: Entity) -> Option<&mut T> {
        if !self.contains(entity) {
            return None;
        }
        self.components
            .get_mut(&TypeId::of::<T>())
            .and_then(|storage| storage.as_any_mut().downcast_mut::<TypedStorage<T>>())
            .and_then(|storage| storage.values.get_mut(&entity))
    }

    /// Removes a component from an entity.
    pub fn remove<T: 'static>(&mut self, entity: Entity) -> Result<Option<T>, WorldError> {
        if !self.contains(entity) {
            return Err(WorldError::EntityNotFound(entity));
        }

        Ok(self
            .components
            .get_mut(&TypeId::of::<T>())
            .and_then(|storage| storage.as_any_mut().downcast_mut::<TypedStorage<T>>())
            .and_then(|storage| storage.values.remove(&entity)))
    }

    /// Iterates over entities that own a component of type `T`.
    pub fn iter<T: 'static>(&self) -> impl Iterator<Item = (Entity, &T)> {
        self.components
            .get(&TypeId::of::<T>())
            .into_iter()
            .filter_map(|storage| storage.as_any().downcast_ref::<TypedStorage<T>>())
            .flat_map(|storage| {
                storage
                    .values
                    .iter()
                    .map(|(entity, value)| (*entity, value))
            })
    }

    /// Iterates mutably over entities that own a component of type `T`.
    pub fn iter_mut<T: 'static>(&mut self) -> impl Iterator<Item = (Entity, &mut T)> {
        self.components
            .get_mut(&TypeId::of::<T>())
            .into_iter()
            .filter_map(|storage| storage.as_any_mut().downcast_mut::<TypedStorage<T>>())
            .flat_map(|storage| {
                storage
                    .values
                    .iter_mut()
                    .map(|(entity, value)| (*entity, value))
            })
    }

    /// Returns the number of alive entities.
    pub fn entity_count(&self) -> usize {
        self.slots.iter().filter(|s| s.alive).count()
    }

    /// Returns the number of stored values for a component type.
    pub fn component_count<T: 'static>(&self) -> usize {
        self.components
            .get(&TypeId::of::<T>())
            .map_or(0, |storage| storage.len())
    }

    /// Inserts or replaces a global resource.
    pub fn insert_resource<T: Any>(&mut self, resource: T) -> Option<T> {
        self.resources
            .insert(TypeId::of::<T>(), Box::new(resource))
            .and_then(|old| old.downcast::<T>().ok())
            .map(|old| *old)
    }

    /// Gets an immutable global resource.
    pub fn get_resource<T: Any>(&self) -> Option<&T> {
        self.resources
            .get(&TypeId::of::<T>())
            .and_then(|resource| resource.downcast_ref::<T>())
    }

    /// Gets a mutable global resource.
    pub fn get_resource_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.resources
            .get_mut(&TypeId::of::<T>())
            .and_then(|resource| resource.downcast_mut::<T>())
    }
}

#[cfg(test)]
mod tests {
    use super::{Entity, World, WorldError};

    #[derive(Debug, PartialEq)]
    struct Health(u32);

    #[test]
    fn components_follow_entity_lifetime() {
        let mut world = World::new();
        let entity = world.spawn(Health(100));

        assert_eq!(world.get::<Health>(entity), Some(&Health(100)));
        assert_eq!(world.insert(entity, Health(75)), Ok(Some(Health(100))));
        assert_eq!(world.get::<Health>(entity), Some(&Health(75)));
        assert_eq!(world.despawn(entity), Ok(()));
        assert!(!world.contains(entity));
        assert_eq!(world.get::<Health>(entity), None);
        assert_eq!(
            world.despawn(entity),
            Err(WorldError::EntityNotFound(entity))
        );
    }

    #[test]
    fn generational_index_prevents_stale_handle_reuse() {
        let mut world = World::new();
        let entity_v1 = world.spawn(Health(100));
        world.despawn(entity_v1).expect("despawn");

        let entity_v2 = world.spawn(Health(200));

        // Index is reused but generation changed
        assert_eq!(entity_v1.index(), entity_v2.index());
        assert_ne!(entity_v1.generation(), entity_v2.generation());

        // Stale handle should fail lookup and contain check
        assert!(!world.contains(entity_v1));
        assert!(world.contains(entity_v2));
        assert_eq!(world.get::<Health>(entity_v1), None);
        assert_eq!(world.get::<Health>(entity_v2), Some(&Health(200)));
    }

    #[test]
    fn missing_entities_are_rejected() {
        let mut world = World::new();
        let entity = Entity::from_raw(42);

        assert_eq!(
            world.insert(entity, Health(10)),
            Err(WorldError::EntityNotFound(entity))
        );
        assert_eq!(
            world.remove::<Health>(entity),
            Err(WorldError::EntityNotFound(entity))
        );
    }

    #[test]
    fn resources_are_type_indexed() {
        let mut world = World::new();
        assert_eq!(world.insert_resource(12_u32), None);
        assert_eq!(world.insert_resource(20_u32), Some(12));
        assert_eq!(world.get_resource::<u32>(), Some(&20));
    }
}
