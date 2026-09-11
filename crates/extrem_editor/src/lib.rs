use extrem_ecs::{Entity, World, WorldError};
use extrem_math::{Transform, Vec3};
use extrem_scene::{Name, Visibility};
use std::fmt;

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

/// Editor-side errors with stable user-facing messages.
#[derive(Debug)]
pub enum EditorError {
    World(WorldError),
    MissingTransform(Entity),
    NonFiniteDelta(Entity),
}

impl fmt::Display for EditorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::World(error) => error.fmt(formatter),
            Self::MissingTransform(entity) => write!(formatter, "{entity} has no transform"),
            Self::NonFiniteDelta(entity) => {
                write!(formatter, "translation delta for {entity} is not finite")
            }
        }
    }
}

impl std::error::Error for EditorError {}

impl From<WorldError> for EditorError {
    fn from(error: WorldError) -> Self {
        Self::World(error)
    }
}

/// Maintains selection and transaction history for editor commands.
///
/// `Delete` is intentionally not reversible: generational entity IDs cannot be
/// resurrected after despawn without inventing a new identity.
#[derive(Debug, Default)]
pub struct EditorState {
    pub selection: Option<Entity>,
    undo_stack: Vec<CommandRecord>,
    redo_stack: Vec<CommandRecord>,
}

impl EditorState {
    pub fn apply(&mut self, world: &mut World, command: EditorCommand) -> Result<(), EditorError> {
        let inverse = self.execute_forward(world, &command)?;

        if let Some(inverse_cmd) = inverse {
            self.undo_stack.push(CommandRecord {
                forward: command,
                inverse: inverse_cmd,
            });
            self.redo_stack.clear();
        }

        self.clean_selection(world);
        Ok(())
    }

    pub fn undo(&mut self, world: &mut World) -> Result<bool, EditorError> {
        let Some(record) = self.undo_stack.pop() else {
            return Ok(false);
        };

        self.execute_raw(world, &record.inverse)?;
        self.redo_stack.push(record);
        self.clean_selection(world);
        Ok(true)
    }

    pub fn redo(&mut self, world: &mut World) -> Result<bool, EditorError> {
        let Some(record) = self.redo_stack.pop() else {
            return Ok(false);
        };

        self.execute_raw(world, &record.forward)?;
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
                if world.contains(*entity) {
                    self.selection = Some(*entity);
                } else {
                    return Err(WorldError::EntityNotFound(*entity).into());
                }
                Ok(None)
            }
            EditorCommand::Rename { entity, name } => {
                if !world.contains(*entity) {
                    return Err(WorldError::EntityNotFound(*entity).into());
                }
                let previous = world
                    .get::<Name>(*entity)
                    .map(|name| name.0.clone())
                    .unwrap_or_default();
                world.insert(*entity, Name(name.clone()))?;
                Ok(Some(EditorCommand::Rename {
                    entity: *entity,
                    name: previous,
                }))
            }
            EditorCommand::Translate { entity, delta } => {
                if !delta.is_finite() {
                    return Err(EditorError::NonFiniteDelta(*entity));
                }
                let transform = world
                    .get_mut::<Transform>(*entity)
                    .ok_or(EditorError::MissingTransform(*entity))?;
                transform.translation += *delta;
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
                world.despawn(*entity)?;
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
                }
            }
            EditorCommand::Rename { entity, name } => {
                if !world.contains(*entity) {
                    return Err(WorldError::EntityNotFound(*entity).into());
                }
                world.insert(*entity, Name(name.clone()))?;
            }
            EditorCommand::Translate { entity, delta } => {
                if !delta.is_finite() {
                    return Err(EditorError::NonFiniteDelta(*entity));
                }
                let transform = world
                    .get_mut::<Transform>(*entity)
                    .ok_or(EditorError::MissingTransform(*entity))?;
                transform.translation += *delta;
            }
            EditorCommand::SetVisible { entity, visible } => {
                if !world.contains(*entity) {
                    return Err(WorldError::EntityNotFound(*entity).into());
                }
                world.insert(*entity, Visibility(*visible))?;
            }
            EditorCommand::Delete(entity) => {
                if self.selection == Some(*entity) {
                    self.selection = None;
                }
                let _ = world.despawn(*entity);
            }
        }
        Ok(())
    }

    fn clean_selection(&mut self, world: &World) {
        if let Some(selected) = self.selection {
            if !world.contains(selected) {
                self.selection = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EditorCommand, EditorError, EditorState};
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

        editor.undo(&mut world).expect("undo");
        assert_eq!(world.get::<Transform>(entity).unwrap().translation.x, 0.0);

        editor.redo(&mut world).expect("redo");
        assert_eq!(world.get::<Transform>(entity).unwrap().translation.x, 2.0);
    }

    #[test]
    fn rename_creates_name_when_missing() {
        let mut world = World::new();
        let entity = world.spawn(Transform::default());
        let mut editor = EditorState::default();
        editor
            .apply(
                &mut world,
                EditorCommand::Rename {
                    entity,
                    name: "hero".to_owned(),
                },
            )
            .expect("rename");
        assert_eq!(
            world.get::<Name>(entity).map(|name| name.0.as_str()),
            Some("hero"),
        );
    }

    #[test]
    fn non_finite_translation_is_rejected() {
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
        assert!(matches!(result, Err(EditorError::NonFiniteDelta(_))));
        assert_eq!(
            world.get::<Transform>(entity).unwrap().translation,
            Vec3::ZERO,
        );
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

        editor
            .apply(&mut world, EditorCommand::Delete(e2))
            .expect("delete e2");
        assert_eq!(editor.selection, Some(e1));

        editor
            .apply(&mut world, EditorCommand::Delete(e1))
            .expect("delete e1");
        assert_eq!(editor.selection, None);
    }
}
