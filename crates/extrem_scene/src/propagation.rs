//! Iterative transform propagation with reusable scratch, not cached scene state.
use crate::{Children, GlobalTransform, Parent};
use extrem_ecs::{Entity, World, WorldError};
use extrem_math::Transform;
use std::collections::HashSet;

/// Observations from one propagation call, not timing or allocation measurements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransformPropagationStats {
    pub roots: usize,
    pub visited: usize,
    pub child_links: usize,
    pub skipped_revisits: usize,
    pub missing_child_transforms: usize,
    pub inserted_globals: usize,
    /// Defensive diagnostic; entities with a Transform remain live during this exclusive call.
    pub first_write_error: Option<WorldError>,
    pub pending_capacity: usize,
    pub visited_capacity: usize,
}

/// Reuses traversal allocations across frames without caching topology or transforms.
///
/// Every call discovers roots anew and clears its visited set. Child lists are
/// borrowed after writing the current global, never cloned. Entity generations,
/// stack order and the existing TRS composition arithmetic are preserved.
///
/// Like the compatibility function, this is NOT hierarchy validation: unreachable
/// components keep their previous globals, and malformed reachable cycles terminate
/// at the first visit. Call `validate_hierarchy` separately at trust boundaries.
/// For inconsistent multi-parent input, first-visit order remains unspecified.
#[derive(Debug, Default)]
pub struct TransformPropagator {
    pending: Vec<(Entity, Transform)>,
    visited: HashSet<Entity>,
}

impl TransformPropagator {
    /// Recomputes reachable globals. No allocations are indexed by a raw entity ID.
    ///
    /// Retained scratch is a high-water mark, not a world-size limit. Use
    /// `release_memory` after a large scene when retaining capacity is undesirable.
    /// This method retains the existing numerical behavior; it does not validate
    /// finite transforms, repair hierarchy edges or promise panic/OOM recovery.
    pub fn propagate(&mut self, world: &mut World) -> TransformPropagationStats {
        self.pending.clear();
        self.visited.clear();
        self.pending.extend(world.iter::<Transform>().filter_map(|(entity, transform)| {
            world
                .get::<Parent>(entity)
                .is_none()
                .then_some((entity, *transform))
        }));
        let mut stats = TransformPropagationStats {
            roots: self.pending.len(),
            ..TransformPropagationStats::default()
        };

        while let Some((entity, parent_global)) = self.pending.pop() {
            if !self.visited.insert(entity) {
                stats.skipped_revisits += 1;
                continue;
            }
            if let Some(global) = world.get_mut::<GlobalTransform>(entity) {
                global.0 = parent_global;
            } else {
                match world.insert(entity, GlobalTransform(parent_global)) {
                    Ok(_) => stats.inserted_globals += 1,
                    Err(error) => {
                        stats.first_write_error.get_or_insert(error);
                    }
                }
            }
            // All following World accesses are immutable. End this borrow before
            // the next iteration's mutable GlobalTransform access.
            if let Some(children) = world.get::<Children>(entity) {
                stats.child_links += children.0.len();
                for &child in &children.0 {
                    if let Some(local) = world.get::<Transform>(child).copied() {
                        self.pending.push((child, Transform::combine(parent_global, local)));
                    } else {
                        stats.missing_child_transforms += 1;
                    }
                }
            }
        }
        stats.visited = self.visited.len();
        stats.pending_capacity = self.pending.capacity();
        stats.visited_capacity = self.visited.capacity();
        stats
    }

    /// Drops scratch allocations. This does not promise an operating-system RSS reduction.
    pub fn release_memory(&mut self) {
        *self = Self::default();
    }

    /// Current `(pending entries, visited entities)` capacities, not byte counts.
    pub fn scratch_capacity(&self) -> (usize, usize) {
        (self.pending.capacity(), self.visited.capacity())
    }
}

/// Compatibility entry point using fresh scratch. Reuse `TransformPropagator` in frame loops.
pub fn propagate_transforms(world: &mut World) {
    TransformPropagator::default().propagate(world);
}
