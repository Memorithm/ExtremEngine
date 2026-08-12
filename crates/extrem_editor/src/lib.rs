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
}

impl fmt::Display for EditorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::World(error) => error.fmt(formatter),
            Self::MissingTransform(entity) => write!(formatter, "{entity} has no transform"),
        }
    }
}

impl std::error::Error for EditorError {}

impl From<WorldError> for EditorError {
    fn from(error: WorldError) -> Self {
        Self::World(error)
    }
}

/// Maintains selection and undo/redo history for editor commands.
#[derive(Debug, Default)]
pub struct EditorState {
    pub selection: Option<Entity>,
    undo_stack: Vec<EditorCommand>,
    redo_stack: Vec<EditorCommand>,
}

impl EditorState {
    pub fn apply(&mut self, world: &mut World, command: EditorCommand) -> Result<(), EditorError> {
        let inverse = match &command {
            EditorCommand::Select(entity) => {
                self.selection = Some(*entity);
                None
            }
            EditorCommand::Rename { entity, name } => {
                let previous = world
                    .get::<Name>(*entity)
                    .ok_or(WorldError::EntityNotFound(*entity))?
                    .0
                    .clone();
                world.insert(*entity, Name(name.clone()))?;
                Some(EditorCommand::Rename {
                    entity: *entity,
                    name: previous,
                })
            }
            EditorCommand::Translate { entity, delta } => {
                let transform = world
                    .get_mut::<Transform>(*entity)
                    .ok_or(EditorError::MissingTransform(*entity))?;
                transform.translation += *delta;
                Some(EditorCommand::Translate {
                    entity: *entity,
                    delta: *delta * -1.0,
                })
            }
            EditorCommand::SetVisible { entity, visible } => {
                let previous = world.get::<Visibility>(*entity).is_none_or(|value| value.0);
                world.insert(*entity, Visibility(*visible))?;
                Some(EditorCommand::SetVisible {
                    entity: *entity,
                    visible: previous,
                })
            }
            EditorCommand::Delete(entity) => {
                world.despawn(*entity)?;
                self.selection = (self.selection == Some(*entity)).then_some(*entity);
                None
            }
        };
        if let Some(inverse) = inverse {
            self.undo_stack.push(inverse);
            self.redo_stack.clear();
        }
        Ok(())
    }

    pub fn undo(&mut self, world: &mut World) -> Result<bool, EditorError> {
        let Some(command) = self.undo_stack.pop() else {
            return Ok(false);
        };
        let redo = command.clone();
        self.apply_without_history(world, command)?;
        self.redo_stack.push(redo);
        Ok(true)
    }

    pub fn redo(&mut self, world: &mut World) -> Result<bool, EditorError> {
        let Some(command) = self.redo_stack.pop() else {
            return Ok(false);
        };
        self.apply(world, command)?;
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

    fn apply_without_history(
        &mut self,
        world: &mut World,
        command: EditorCommand,
    ) -> Result<(), EditorError> {
        match command {
            EditorCommand::Select(entity) => self.selection = Some(entity),
            EditorCommand::Rename { entity, name } => {
                world.insert(entity, Name(name))?;
            }
            EditorCommand::Translate { entity, delta } => {
                world
                    .get_mut::<Transform>(entity)
                    .ok_or(EditorError::MissingTransform(entity))?
                    .translation += delta;
            }
            EditorCommand::SetVisible { entity, visible } => {
                world.insert(entity, Visibility(visible))?;
            }
            EditorCommand::Delete(entity) => {
                world.despawn(entity)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{EditorCommand, EditorState};
    use extrem_ecs::World;
    use extrem_math::{Transform, Vec3};
    use extrem_scene::Name;

    #[test]
    fn editor_commands_can_be_undone_and_inspected() {
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
        assert_eq!(
            editor
                .inspect(&world, entity)
                .expect("inspect")
                .transform
                .unwrap()
                .translation
                .x,
            2.0
        );
        editor.undo(&mut world).expect("undo");
        assert_eq!(
            world
                .get::<Transform>(entity)
                .expect("transform")
                .translation,
            Vec3::ZERO
        );
    }
}
