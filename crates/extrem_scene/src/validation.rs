use crate::{Children, HierarchyError, Parent};
use extrem_ecs::{Entity, World, WorldError};
use std::collections::{HashMap, HashSet};

/// Work counters for one successful hierarchy validation, not timing estimates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HierarchyValidationStats {
    pub parent_links: usize,
    pub child_links: usize,
    /// Number of parent links first entered by cycle detection.
    /// Each link is entered exactly once on a valid hierarchy.
    pub ancestor_steps: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VisitState {
    Unseen,
    Active,
    Finished,
}

#[derive(Clone, Copy, Debug)]
struct Link {
    parent: Entity,
    state: VisitState,
}

/// Reusable scratch storage for full, read-only hierarchy validation.
///
/// Each call rebuilds all links; no previously validated world state is trusted.
/// Storage is keyed by complete generational entities, not untrusted raw indices.
/// Default randomized hashing is retained. Capacity follows the high-water mark;
/// call [`Self::release_memory`] when that retention is no longer appropriate.
#[derive(Debug, Default)]
pub struct HierarchyValidator {
    links: HashMap<Entity, Link>,
    referenced: HashSet<Entity>,
}

impl HierarchyValidator {
    /// Releases retained scratch collections. Does not modify any world.
    pub fn release_memory(&mut self) {
        *self = Self::default();
    }

    /// Checks liveness, both edge directions, uniqueness and acyclicity.
    ///
    /// The graph work is expected linear in the number of links; ECS HashMap
    /// scans and clearing retained scratch storage also depend on their capacities.
    /// No recursion or repeated scan of a parent's full child list is performed.
    /// Errors never mutate the world. Which defect is reported first is unspecified.
    pub fn validate(&mut self, world: &World) -> Result<HierarchyValidationStats, HierarchyError> {
        self.links.clear();
        self.referenced.clear();
        let parent_count = world.component_count::<Parent>();
        self.links.reserve(parent_count);
        self.referenced.reserve(parent_count);
        let mut stats = HierarchyValidationStats::default();

        for (child, parent) in world.iter::<Parent>() {
            if !world.contains(parent.0) {
                return Err(WorldError::EntityNotFound(parent.0).into());
            }
            if child == parent.0 {
                return Err(HierarchyError::SelfParent(child));
            }
            self.links.insert(
                child,
                Link {
                    parent: parent.0,
                    state: VisitState::Unseen,
                },
            );
            stats.parent_links += 1;
        }

        // A child belongs to at most one parent. One global set replaces both
        // per-parent sets and a linear Children::contains search for each child.
        for (parent, children) in world.iter::<Children>() {
            for child in &children.0 {
                if !world.contains(*child) {
                    return Err(WorldError::EntityNotFound(*child).into());
                }
                if self
                    .links
                    .get(child)
                    .is_none_or(|link| link.parent != parent)
                {
                    return Err(HierarchyError::InconsistentState(format!(
                        "{parent} references {child}, but the child's Parent component disagrees"
                    )));
                }
                if !self.referenced.insert(*child) {
                    return Err(HierarchyError::InconsistentState(format!(
                        "{parent} contains duplicate child {child}"
                    )));
                }
                stats.child_links += 1;
            }
        }

        for (child, link) in &self.links {
            if !self.referenced.contains(child) {
                return Err(HierarchyError::InconsistentState(format!(
                    "{child} has Parent({}), but the parent does not reference it",
                    link.parent
                )));
            }
        }

        // Parent links form a functional graph. Each link transitions exactly
        // Unseen -> Active -> Finished. Previously finished tails are not revisited.
        // A second walk finishes the active path without a recursive call stack.
        for (start, parent) in world.iter::<Parent>() {
            let mut current = start;
            while let Some(link) = self.links.get_mut(&current) {
                match link.state {
                    VisitState::Finished => break,
                    VisitState::Active => {
                        return Err(HierarchyError::CycleDetected {
                            child: start,
                            parent: parent.0,
                        });
                    }
                    VisitState::Unseen => {
                        link.state = VisitState::Active;
                        stats.ancestor_steps += 1;
                        current = link.parent;
                    }
                }
            }
            current = start;
            while let Some(link) = self.links.get_mut(&current) {
                if link.state != VisitState::Active {
                    break;
                }
                link.state = VisitState::Finished;
                current = link.parent;
            }
        }
        Ok(stats)
    }
}

/// Validates both hierarchy directions and rejects stale links, duplicates and cycles.
///
/// This compatibility entry point uses fresh scratch storage. Repeated callers can
/// retain a [`HierarchyValidator`] to reuse its allocation capacity, not its results.
pub fn validate_hierarchy(world: &World) -> Result<(), HierarchyError> {
    HierarchyValidator::default().validate(world).map(|_| ())
}
