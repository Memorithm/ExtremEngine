use extrem_ecs::{Entity, World, WorldError};
use extrem_math::{Transform, Vec3};
use extrem_scene::{HierarchyError, Name, Visibility, despawn_recursive, validate_hierarchy};
use std::collections::VecDeque;
use std::fmt;

/// Commands that can be sent by a GUI, CLI or remote editor client.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorCommand {
    Select(Entity),
    Rename {
        entity: Entity,
        name: String,
    },
    Translate {
        entity: Entity,
        delta: Vec3,
    },
    /// Assigns an absolute position; also used for exact history replay.
    SetTranslation {
        entity: Entity,
        translation: Vec3,
    },
    SetVisible {
        entity: Entity,
        visible: bool,
    },
    /// Removes the component instead of replacing absence with `Visibility(true)`.
    RemoveVisibility(Entity),
    /// Deletes a validated subtree. Successful deletion clears both history stacks.
    Delete(Entity),
}

impl EditorCommand {
    fn entity(&self) -> Entity {
        match self {
            Self::Select(entity) | Self::RemoveVisibility(entity) | Self::Delete(entity) => *entity,
            Self::Rename { entity, .. }
            | Self::Translate { entity, .. }
            | Self::SetTranslation { entity, .. }
            | Self::SetVisible { entity, .. } => *entity,
        }
    }
}

/// A command record coupling exact forward and inverse assignments.
///
/// Translation records store absolute positions, not an arithmetic inverse delta.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandRecord {
    pub forward: EditorCommand,
    pub inverse: EditorCommand,
}

/// Read-only representation suitable for an inspector panel.
#[derive(Clone, Debug, PartialEq)]
pub struct InspectorSnapshot {
    pub entity: Entity,
    pub name: Option<String>,
    pub transform: Option<Transform>,
    pub visible: Option<bool>,
}

/// Limits for retained history and names copied into history records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorLimits {
    /// Maximum combined undo/redo records. Zero disables recording, not editing.
    pub max_history_entries: usize,
    /// Maximum UTF-8 bytes in either the old or the new name of a rename.
    pub max_name_bytes: usize,
}

impl Default for EditorLimits {
    fn default() -> Self {
        Self {
            max_history_entries: 256,
            max_name_bytes: 4096,
        }
    }
}

/// Editor-side errors with stable user-facing messages.
#[derive(Debug)]
pub enum EditorError {
    World(WorldError),
    Hierarchy(HierarchyError),
    MissingTransform(Entity),
    MissingName(Entity),
    NonFiniteTranslation(Entity),
    NameTooLong { max_bytes: usize },
}

impl fmt::Display for EditorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::World(error) => error.fmt(formatter),
            Self::Hierarchy(error) => error.fmt(formatter),
            Self::MissingTransform(entity) => write!(formatter, "{entity} has no transform"),
            Self::MissingName(entity) => write!(formatter, "{entity} has no name"),
            Self::NonFiniteTranslation(entity) => {
                write!(formatter, "translation for {entity} must remain finite")
            }
            Self::NameTooLong { max_bytes } => {
                write!(formatter, "editor name exceeds {max_bytes} UTF-8 bytes")
            }
        }
    }
}

impl std::error::Error for EditorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::World(error) => Some(error),
            Self::Hierarchy(error) => Some(error),
            _ => None,
        }
    }
}

impl From<WorldError> for EditorError {
    fn from(error: WorldError) -> Self {
        Self::World(error)
    }
}

impl From<HierarchyError> for EditorError {
    fn from(error: HierarchyError) -> Self {
        Self::Hierarchy(error)
    }
}

/// Maintains selection and bounded transaction history for one caller-owned world.
///
/// Returned errors leave the world, selection and history unchanged. This is not
/// a panic/OOM recovery boundary. Replay restores only the edited component field;
/// callers must clear history when replacing the world or discarding external edits.
#[derive(Debug, Default)]
pub struct EditorState {
    pub selection: Option<Entity>,
    undo_stack: VecDeque<CommandRecord>,
    redo_stack: VecDeque<CommandRecord>,
    limits: EditorLimits,
}

impl EditorState {
    pub fn with_limits(limits: EditorLimits) -> Self {
        Self {
            limits,
            ..Self::default()
        }
    }

    pub fn limits(&self) -> EditorLimits {
        self.limits
    }

    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }

    /// Drops retained records without changing world state or selection.
    pub fn clear_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Applies a validated command. Selection alone does not invalidate redo.
    ///
    /// Deletion is an explicit irreversible history barrier, not a fake undoable edit.
    pub fn apply(&mut self, world: &mut World, command: EditorCommand) -> Result<(), EditorError> {
        ensure_alive(world, command.entity())?;
        let record = self.prepare_record(world, &command)?;
        let forward = record.as_ref().map_or(&command, |record| &record.forward);
        self.execute_raw(world, forward)?;

        if matches!(command, EditorCommand::Delete(_)) {
            self.clear_history();
        } else if let Some(record) = record {
            self.redo_stack.clear();
            if self.limits.max_history_entries > 0 {
                if self.undo_stack.len() == self.limits.max_history_entries {
                    drop(self.undo_stack.pop_front());
                }
                self.undo_stack.push_back(record);
            }
        }
        self.clean_selection(world);
        Ok(())
    }

    /// Restores the previous field value; a failed replay retains its record for retry.
    pub fn undo(&mut self, world: &mut World) -> Result<bool, EditorError> {
        let Some(record) = self.undo_stack.pop_back() else {
            return Ok(false);
        };
        if let Err(error) = self.execute_raw(world, &record.inverse) {
            self.undo_stack.push_back(record);
            return Err(error);
        }
        self.redo_stack.push_back(record);
        self.clean_selection(world);
        Ok(true)
    }

    /// Restores the recorded forward value; a failed replay retains its record for retry.
    pub fn redo(&mut self, world: &mut World) -> Result<bool, EditorError> {
        let Some(record) = self.redo_stack.pop_back() else {
            return Ok(false);
        };
        if let Err(error) = self.execute_raw(world, &record.forward) {
            self.redo_stack.push_back(record);
            return Err(error);
        }
        self.undo_stack.push_back(record);
        self.clean_selection(world);
        Ok(true)
    }

    pub fn inspect(&self, world: &World, entity: Entity) -> Result<InspectorSnapshot, EditorError> {
        ensure_alive(world, entity)?;
        Ok(InspectorSnapshot {
            entity,
            name: world.get::<Name>(entity).map(|name| name.0.clone()),
            transform: world.get::<Transform>(entity).copied(),
            visible: world.get::<Visibility>(entity).map(|value| value.0),
        })
    }

    fn prepare_record(
        &self,
        world: &World,
        command: &EditorCommand,
    ) -> Result<Option<CommandRecord>, EditorError> {
        let record = match command {
            EditorCommand::Select(_) | EditorCommand::Delete(_) => return Ok(None),
            EditorCommand::Rename { entity, name } => {
                self.validate_name(name)?;
                let previous = &world
                    .get::<Name>(*entity)
                    .ok_or(EditorError::MissingName(*entity))?
                    .0;
                self.validate_name(previous)?;
                CommandRecord {
                    forward: command.clone(),
                    inverse: EditorCommand::Rename {
                        entity: *entity,
                        name: previous.clone(),
                    },
                }
            }
            EditorCommand::Translate { entity, delta } => {
                let previous = translation(world, *entity)?;
                validate_translation(*entity, *delta)?;
                let next = previous + *delta;
                validate_translation(*entity, next)?;
                translation_record(*entity, previous, next)
            }
            EditorCommand::SetTranslation {
                entity,
                translation: next,
            } => {
                let previous = translation(world, *entity)?;
                validate_translation(*entity, *next)?;
                translation_record(*entity, previous, *next)
            }
            EditorCommand::SetVisible { entity, .. } | EditorCommand::RemoveVisibility(entity) => {
                let inverse = match world.get::<Visibility>(*entity) {
                    Some(previous) => EditorCommand::SetVisible {
                        entity: *entity,
                        visible: previous.0,
                    },
                    None => EditorCommand::RemoveVisibility(*entity),
                };
                CommandRecord {
                    forward: command.clone(),
                    inverse,
                }
            }
        };
        Ok(Some(record))
    }

    fn execute_raw(
        &mut self,
        world: &mut World,
        command: &EditorCommand,
    ) -> Result<(), EditorError> {
        ensure_alive(world, command.entity())?;
        match command {
            EditorCommand::Select(entity) => self.selection = Some(*entity),
            EditorCommand::Rename { entity, name } => {
                self.validate_name(name)?;
                world
                    .get_mut::<Name>(*entity)
                    .ok_or(EditorError::MissingName(*entity))?
                    .0
                    .clone_from(name);
            }
            EditorCommand::Translate { entity, delta } => {
                let previous = translation(world, *entity)?;
                validate_translation(*entity, *delta)?;
                set_translation(world, *entity, previous + *delta)?;
            }
            EditorCommand::SetTranslation {
                entity,
                translation,
            } => set_translation(world, *entity, *translation)?,
            EditorCommand::SetVisible { entity, visible } => {
                world.insert(*entity, Visibility(*visible))?;
            }
            EditorCommand::RemoveVisibility(entity) => {
                world.remove::<Visibility>(*entity)?;
            }
            EditorCommand::Delete(entity) => {
                // Validate before mutating: a corrupt edge must not authorize deletion.
                validate_hierarchy(world)?;
                despawn_recursive(world, *entity)?;
            }
        }
        Ok(())
    }

    fn validate_name(&self, name: &str) -> Result<(), EditorError> {
        if name.len() > self.limits.max_name_bytes {
            return Err(EditorError::NameTooLong {
                max_bytes: self.limits.max_name_bytes,
            });
        }
        Ok(())
    }

    fn clean_selection(&mut self, world: &World) {
        if self.selection.is_some_and(|entity| !world.contains(entity)) {
            self.selection = None;
        }
    }
}

fn ensure_alive(world: &World, entity: Entity) -> Result<(), EditorError> {
    if !world.contains(entity) {
        return Err(WorldError::EntityNotFound(entity).into());
    }
    Ok(())
}

fn validate_translation(entity: Entity, value: Vec3) -> Result<(), EditorError> {
    if !value.is_finite() {
        return Err(EditorError::NonFiniteTranslation(entity));
    }
    Ok(())
}

fn translation(world: &World, entity: Entity) -> Result<Vec3, EditorError> {
    let value = world
        .get::<Transform>(entity)
        .ok_or(EditorError::MissingTransform(entity))?
        .translation;
    validate_translation(entity, value)?;
    Ok(value)
}

fn set_translation(world: &mut World, entity: Entity, value: Vec3) -> Result<(), EditorError> {
    validate_translation(entity, value)?;
    world
        .get_mut::<Transform>(entity)
        .ok_or(EditorError::MissingTransform(entity))?
        .translation = value;
    Ok(())
}

fn translation_record(entity: Entity, previous: Vec3, next: Vec3) -> CommandRecord {
    CommandRecord {
        forward: EditorCommand::SetTranslation {
            entity,
            translation: next,
        },
        inverse: EditorCommand::SetTranslation {
            entity,
            translation: previous,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{EditorCommand, EditorState};
    use extrem_ecs::World;
    use extrem_math::{Transform, Vec3};
    use extrem_scene::Name;

    #[test]
    fn editor_commands_can_be_undone_and_redone() {
        let mut world = World::new();
        let entity = world.spawn(Transform::default());
        world.insert(entity, Name::from("before")).expect("entity");
        let mut editor = EditorState::default();

        editor
            .apply(
                &mut world,
                EditorCommand::Translate {
                    entity,
                    delta: Vec3::new(2.0, 0.0, 0.0),
                },
            )
            .expect("translate");
        assert_eq!(world.get::<Transform>(entity).unwrap().translation.x, 2.0);

        // Undo translates back to 0
        editor.undo(&mut world).expect("undo");
        assert_eq!(world.get::<Transform>(entity).unwrap().translation.x, 0.0);

        // Redo replays forward translation to 2.0 (NOT -2.0!)
        editor.redo(&mut world).expect("redo");
        assert_eq!(world.get::<Transform>(entity).unwrap().translation.x, 2.0);
    }

    #[test]
    fn delete_clears_selection_if_selected_and_preserves_valid_selection() {
        let mut world = World::new();
        let e1 = world.spawn(Transform::default());
        let e2 = world.spawn(Transform::default());
        let mut editor = EditorState::default();

        editor
            .apply(&mut world, EditorCommand::Select(e1))
            .expect("select e1");
        assert_eq!(editor.selection, Some(e1));

        // Delete unselected e2 does not clear selection e1
        editor
            .apply(&mut world, EditorCommand::Delete(e2))
            .expect("delete e2");
        assert_eq!(editor.selection, Some(e1));

        // Delete selected e1 clears selection
        editor
            .apply(&mut world, EditorCommand::Delete(e1))
            .expect("delete e1");
        assert_eq!(editor.selection, None);
    }
}
