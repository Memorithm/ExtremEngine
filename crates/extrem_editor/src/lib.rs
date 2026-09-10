use extrem_ecs::{Entity, World, WorldError};
use extrem_math::{Transform, Vec3};
use extrem_scene::{HierarchyError, Name, Visibility, despawn_recursive};
use std::fmt;

const MAX_EDITOR_NAME_BYTES: usize = 4096;

/// Commands that can be sent by a GUI, CLI or remote editor client.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorCommand {
    Select(Entity),
    Rename { entity: Entity, name: String },
    Translate { entity: Entity, delta: Vec3 },
    SetVisible { entity: Entity, visible: bool },
    Delete(Entity),
}

/// A transactional command record coupling forward and inverse operations.
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

#[derive(Debug)]
pub enum EditorError {
    World(WorldError),
    Hierarchy(HierarchyError),
    MissingTransform(Entity),
    InvalidTranslation,
    InvalidName,
}

impl fmt::Display for EditorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::World(error) => error.fmt(formatter),
            Self::Hierarchy(error) => error.fmt(formatter),
            Self::MissingTransform(entity) => write!(formatter, "{entity} has no transform"),
            Self::InvalidTranslation => write!(formatter, "editor translation must be finite"),
            Self::InvalidName => write!(formatter, "editor name is empty, too long, or contains NUL"),
        }
    }
}

impl std::error::Error for EditorError {}

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

/// Maintains selection and transaction history for editor commands.
#[derive(Debug, Default)]
pub struct EditorState {
    pub selection: Option<Entity>,
    undo_stack: Vec<CommandRecord>,
    redo_stack: Vec<CommandRecord>,
}

impl EditorState {
    pub fn apply(&mut self, world: &mut World, command: EditorCommand) -> Result<(), EditorError> {
        let mutates_world = !matches!(command, EditorCommand::Select(_));
        let inverse = self.execute_forward(world, &command)?;

        if mutates_world {
            // Any successful new world mutation establishes a new history branch, including
            // non-undoable barriers such as Delete.
            self.redo_stack.clear();
        }
        if let Some(inverse_command) = inverse {
            self.undo_stack.push(CommandRecord {
                forward: command,
                inverse: inverse_command,
            });
        }

        self.clean_selection(world);
        Ok(())
    }

    pub fn undo(&mut self, world: &mut World) -> Result<bool, EditorError> {
        let Some(record) = self.undo_stack.pop() else {
            return Ok(false);
        };

        if let Err(error) = self.execute_raw(world, &record.inverse) {
            // Failed undo must not destroy recoverability/history state.
            self.undo_stack.push(record);
            return Err(error);
        }
        self.redo_stack.push(record);
        self.clean_selection(world);
        Ok(true)
    }

    pub fn redo(&mut self, world: &mut World) -> Result<bool, EditorError> {
        let Some(record) = self.redo_stack.pop() else {
            return Ok(false);
        };

        if let Err(error) = self.execute_raw(world, &record.forward) {
            // Failed redo remains retryable after the external cause is repaired.
            self.redo_stack.push(record);
            return Err(error);
        }
        self.undo_stack.push(record);
        self.clean_selection(world);
        Ok(true)
    }

    pub fn inspect(&self, world: &World, entity: Entity) -> Result<InspectorSnapshot, EditorError> {
        if !world.contains(entity) {
            return Err(WorldError::EntityNotFound(entity).into());
        }
        Ok(InspectorSnapshot {
            entity,
            name: world.get::<Name>(entity).map(|name| name.0.clone()),
            transform: world.get::<Transform>(entity).copied(),
            visible: world.get::<Visibility>(entity).map(|value| value.0),
        })
    }

    fn execute_forward(
        &mut self,
        world: &mut World,
        command: &EditorCommand,
    ) -> Result<Option<EditorCommand>, EditorError> {
        match command {
            EditorCommand::Select(entity) => {
                if !world.contains(*entity) {
                    return Err(WorldError::EntityNotFound(*entity).into());
                }
                self.selection = Some(*entity);
                Ok(None)
            }
            EditorCommand::Rename { entity, name } => {
                validate_name(name)?;
                let previous = world
                    .get::<Name>(*entity)
                    .ok_or(WorldError::EntityNotFound(*entity))?
                    .0
                    .clone();
                world.insert(*entity, Name(name.clone()))?;
                Ok(Some(EditorCommand::Rename {
                    entity: *entity,
                    name: previous,
                }))
            }
            EditorCommand::Translate { entity, delta } => {
                if !delta.is_finite() {
                    return Err(EditorError::InvalidTranslation);
                }
                let transform = world
                    .get_mut::<Transform>(*entity)
                    .ok_or(EditorError::MissingTransform(*entity))?;
                let next = transform.translation + *delta;
                if !next.is_finite() {
                    return Err(EditorError::InvalidTranslation);
                }
                transform.translation = next;
                Ok(Some(EditorCommand::Translate {
                    entity: *entity,
                    delta: *delta * -1.0,
                }))
            }
            EditorCommand::SetVisible { entity, visible } => {
                if !world.contains(*entity) {
                    return Err(WorldError::EntityNotFound(*entity).into());
                }
                let previous = world.get::<Visibility>(*entity).is_none_or(|value| value.0);
                world.insert(*entity, Visibility(*visible))?;
                Ok(Some(EditorCommand::SetVisible {
                    entity: *entity,
                    visible: previous,
                }))
            }
            EditorCommand::Delete(entity) => {
                if self.selection == Some(*entity) {
                    self.selection = None;
                }
                // Delete is intentionally a history barrier until a complete subtree snapshot
                // format exists. It must still preserve scene hierarchy invariants.
                despawn_recursive(world, *entity)?;
                Ok(None)
            }
        }
    }

    fn execute_raw(
        &mut self,
        world: &mut World,
        command: &EditorCommand,
    ) -> Result<(), EditorError> {
        match command {
            EditorCommand::Select(entity) => {
                if world.contains(*entity) {
                    self.selection = Some(*entity);
                } else {
                    return Err(WorldError::EntityNotFound(*entity).into());
                }
            }
            EditorCommand::Rename { entity, name } => {
                validate_name(name)?;
                world.insert(*entity, Name(name.clone()))?;
            }
            EditorCommand::Translate { entity, delta } => {
                if !delta.is_finite() {
                    return Err(EditorError::InvalidTranslation);
                }
                let transform = world
                    .get_mut::<Transform>(*entity)
                    .ok_or(EditorError::MissingTransform(*entity))?;
                let next = transform.translation + *delta;
                if !next.is_finite() {
                    return Err(EditorError::InvalidTranslation);
                }
                transform.translation = next;
            }
            EditorCommand::SetVisible { entity, visible } => {
                world.insert(*entity, Visibility(*visible))?;
            }
            EditorCommand::Delete(entity) => {
                despawn_recursive(world, *entity)?;
            }
        }
        Ok(())
    }

    fn clean_selection(&mut self, world: &World) {
        if self.selection.is_some_and(|selected| !world.contains(selected)) {
            self.selection = None;
        }
    }
}

fn validate_name(name: &str) -> Result<(), EditorError> {
    if name.is_empty() || name.len() > MAX_EDITOR_NAME_BYTES || name.as_bytes().contains(&0) {
        return Err(EditorError::InvalidName);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{EditorCommand, EditorError, EditorState};
    use extrem_ecs::World;
    use extrem_math::{Transform, Vec3};
    use extrem_scene::{Children, Name, Parent, set_parent};

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
        assert_eq!(world.get::<Transform>(entity).expect("transform").translation.x, 2.0);
        editor.undo(&mut world).expect("undo");
        assert_eq!(world.get::<Transform>(entity).expect("transform").translation.x, 0.0);
        editor.redo(&mut world).expect("redo");
        assert_eq!(world.get::<Transform>(entity).expect("transform").translation.x, 2.0);
    }

    #[test]
    fn delete_clears_selection_and_recursively_preserves_hierarchy() {
        let mut world = World::new();
        let parent = world.spawn(Transform::default());
        let child = world.spawn(Transform::default());
        set_parent(&mut world, child, parent).expect("hierarchy");
        let mut editor = EditorState::default();
        editor
            .apply(&mut world, EditorCommand::Select(parent))
            .expect("select");
        editor
            .apply(&mut world, EditorCommand::Delete(parent))
            .expect("delete subtree");

        assert_eq!(editor.selection, None);
        assert!(!world.contains(parent));
        assert!(!world.contains(child));
        assert_eq!(world.get::<Parent>(child), None);
        assert_eq!(world.get::<Children>(parent), None);
    }

    #[test]
    fn deleting_unselected_entity_preserves_selection() {
        let mut world = World::new();
        let selected = world.spawn(Transform::default());
        let deleted = world.spawn(Transform::default());
        let mut editor = EditorState::default();
        editor
            .apply(&mut world, EditorCommand::Select(selected))
            .expect("select");
        editor
            .apply(&mut world, EditorCommand::Delete(deleted))
            .expect("delete");
        assert_eq!(editor.selection, Some(selected));
    }

    #[test]
    fn non_finite_translation_is_rejected_without_mutation() {
        let mut world = World::new();
        let entity = world.spawn(Transform::default());
        let mut editor = EditorState::default();
        let result = editor.apply(
            &mut world,
            EditorCommand::Translate {
                entity,
                delta: Vec3::new(f32::NAN, 0.0, 0.0),
            },
        );
        assert!(matches!(result, Err(EditorError::InvalidTranslation)));
        assert_eq!(world.get::<Transform>(entity), Some(&Transform::IDENTITY));
    }

    #[test]
    fn non_undoable_world_mutation_clears_redo_branch() {
        let mut world = World::new();
        let translated = world.spawn(Transform::default());
        let deleted = world.spawn(Transform::default());
        let mut editor = EditorState::default();
        editor
            .apply(
                &mut world,
                EditorCommand::Translate {
                    entity: translated,
                    delta: Vec3::X,
                },
            )
            .expect("translate");
        editor.undo(&mut world).expect("undo");
        editor
            .apply(&mut world, EditorCommand::Delete(deleted))
            .expect("delete");
        assert!(!editor.redo(&mut world).expect("redo query"));
    }
}
