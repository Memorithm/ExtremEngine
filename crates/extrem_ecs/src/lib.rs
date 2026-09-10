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
    pub const fn from_raw_parts(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    pub const fn from_raw(index: u32) -> Self {
        Self {
            index,
            generation: 1,
        }
    }

    pub const fn index(self) -> u32 {
        self.index
    }

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
    retired: bool,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldError {
    EntityNotFound(Entity),
    EntityCapacityExhausted,
}

impl fmt::Display for WorldError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EntityNotFound(entity) => write!(formatter, "{entity} does not exist"),
            Self::EntityCapacityExhausted => write!(formatter, "entity index capacity is exhausted"),
        }
    }
}

impl std::error::Error for WorldError {}

#[derive(Default)]
pub struct World {
    slots: Vec<EntitySlot>,
    free_list: Vec<u32>,
    components: HashMap<TypeId, Box<dyn Storage>>,
    resources: HashMap<TypeId, Box<dyn Any>>,
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fallible spawn path for code that must handle resource exhaustion without panicking.
    pub fn try_spawn_empty(&mut self) -> Result<Entity, WorldError> {
        while let Some(index) = self.free_list.pop() {
            let slot = &mut self.slots[index as usize];
            if slot.retired {
                continue;
            }
            slot.alive = true;
            return Ok(Entity {
                index,
                generation: slot.generation,
            });
        }

        let index = u32::try_from(self.slots.len()).map_err(|_| WorldError::EntityCapacityExhausted)?;
        self.slots.push(EntitySlot {
            generation: 1,
            alive: true,
            retired: false,
        });
        Ok(Entity {
            index,
            generation: 1,
        })
    }

    /// Convenience spawn for existing callers. Prefer [`Self::try_spawn_empty`] at untrusted boundaries.
    pub fn spawn_empty(&mut self) -> Entity {
        self.try_spawn_empty()
            .expect("ExtremEngine entity index capacity exhausted")
    }

    pub fn try_spawn<T: 'static>(&mut self, component: T) -> Result<Entity, WorldError> {
        let entity = self.try_spawn_empty()?;
        self.insert(entity, component)?;
        Ok(entity)
    }

    pub fn spawn<T: 'static>(&mut self, component: T) -> Entity {
        self.try_spawn(component)
            .expect("ExtremEngine entity index capacity exhausted")
    }

    pub fn contains(&self, entity: Entity) -> bool {
        let index = entity.index as usize;
        let Some(slot) = self.slots.get(index) else {
            return false;
        };
        slot.alive && !slot.retired && slot.generation == entity.generation
    }

    /// Removes an entity and retires the slot permanently before generation wraparound.
    pub fn despawn(&mut self, entity: Entity) -> Result<(), WorldError> {
        if !self.contains(entity) {
            return Err(WorldError::EntityNotFound(entity));
        }

        let slot = &mut self.slots[entity.index as usize];
        slot.alive = false;
        if slot.generation == u32::MAX {
            // Reusing generation 1 after wrap could make a stale handle valid again.
            slot.retired = true;
        } else {
            slot.generation += 1;
            self.free_list.push(entity.index);
        }

        for storage in self.components.values_mut() {
            storage.remove_entity(entity);
        }
        Ok(())
    }

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

    pub fn get<T: 'static>(&self, entity: Entity) -> Option<&T> {
        if !self.contains(entity) {
            return None;
        }
        self.components
            .get(&TypeId::of::<T>())
            .and_then(|storage| storage.as_any().downcast_ref::<TypedStorage<T>>())
            .and_then(|storage| storage.values.get(&entity))
    }

    pub fn get_mut<T: 'static>(&mut self, entity: Entity) -> Option<&mut T> {
        if !self.contains(entity) {
            return None;
        }
        self.components
            .get_mut(&TypeId::of::<T>())
            .and_then(|storage| storage.as_any_mut().downcast_mut::<TypedStorage<T>>())
            .and_then(|storage| storage.values.get_mut(&entity))
    }

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

    /// Iteration order is intentionally unspecified. Sort entity IDs at semantic boundaries where order matters.
    pub fn iter<T: 'static>(&self) -> impl Iterator<Item = (Entity, &T)> {
        self.components
            .get(&TypeId::of::<T>())
            .into_iter()
            .filter_map(|storage| storage.as_any().downcast_ref::<TypedStorage<T>>())
            .flat_map(|storage| storage.values.iter().map(|(entity, value)| (*entity, value)))
    }

    /// Mutable iteration order is intentionally unspecified.
    pub fn iter_mut<T: 'static>(&mut self) -> impl Iterator<Item = (Entity, &mut T)> {
        self.components
            .get_mut(&TypeId::of::<T>())
            .into_iter()
            .filter_map(|storage| storage.as_any_mut().downcast_mut::<TypedStorage<T>>())
            .flat_map(|storage| storage.values.iter_mut().map(|(entity, value)| (*entity, value)))
    }

    pub fn entity_count(&self) -> usize {
        self.slots.iter().filter(|slot| slot.alive).count()
    }

    pub fn component_count<T: 'static>(&self) -> usize {
        self.components
            .get(&TypeId::of::<T>())
            .map_or(0, |storage| storage.len())
    }

    pub fn insert_resource<T: Any>(&mut self, resource: T) -> Option<T> {
        self.resources
            .insert(TypeId::of::<T>(), Box::new(resource))
            .and_then(|old| old.downcast::<T>().ok())
            .map(|old| *old)
    }

    pub fn get_resource<T: Any>(&self) -> Option<&T> {
        self.resources
            .get(&TypeId::of::<T>())
            .and_then(|resource| resource.downcast_ref::<T>())
    }

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
        assert_eq!(world.despawn(entity), Ok(()));
        assert!(!world.contains(entity));
        assert_eq!(world.get::<Health>(entity), None);
        assert_eq!(world.despawn(entity), Err(WorldError::EntityNotFound(entity)));
    }

    #[test]
    fn generational_index_prevents_stale_handle_reuse() {
        let mut world = World::new();
        let entity_v1 = world.spawn(Health(100));
        world.despawn(entity_v1).expect("despawn");
        let entity_v2 = world.spawn(Health(200));
        assert_eq!(entity_v1.index(), entity_v2.index());
        assert_ne!(entity_v1.generation(), entity_v2.generation());
        assert!(!world.contains(entity_v1));
        assert_eq!(world.get::<Health>(entity_v2), Some(&Health(200)));
    }

    #[test]
    fn retired_generation_cannot_wrap_to_stale_handle() {
        let mut world = World::new();
        world.slots.push(super::EntitySlot {
            generation: u32::MAX,
            alive: true,
            retired: false,
        });
        let final_generation = Entity::from_raw_parts(0, u32::MAX);
        world.despawn(final_generation).expect("despawn final generation");
        let next = world.try_spawn_empty().expect("new slot");
        assert_ne!(next.index(), final_generation.index());
        assert!(!world.contains(final_generation));
    }

    #[test]
    fn missing_entities_are_rejected() {
        let mut world = World::new();
        let entity = Entity::from_raw(42);
        assert_eq!(world.insert(entity, Health(10)), Err(WorldError::EntityNotFound(entity)));
        assert_eq!(world.remove::<Health>(entity), Err(WorldError::EntityNotFound(entity)));
    }

    #[test]
    fn resources_are_type_indexed() {
        let mut world = World::new();
        assert_eq!(world.insert_resource(12_u32), None);
        assert_eq!(world.insert_resource(20_u32), Some(12));
        assert_eq!(world.get_resource::<u32>(), Some(&20));
    }
}