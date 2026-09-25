use extrem_ecs::{Entity, World, WorldError};
use extrem_math::{Transform, Vec3};
use extrem_render::{FrameInfo, FrameStats, RenderBackend, RenderCommand};
use extrem_scene::{Camera, GlobalTransform, Visibility};

#[derive(Clone, Copy, Debug)]
pub enum Profile {
    Global,
    Local,
    Mixed,
    ManyCameras,
}

impl Profile {
    pub const ALL: [Self; 4] = [Self::Global, Self::Local, Self::Mixed, Self::ManyCameras];

    pub fn name(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Local => "local",
            Self::Mixed => "mixed",
            Self::ManyCameras => "many_cameras",
        }
    }
}

pub fn fixture(profile: Profile, nodes: usize) -> Result<(World, Vec<Entity>), WorldError> {
    let mut world = World::new();
    let mut ids = Vec::with_capacity(nodes);
    for index in 0..nodes {
        let id = world.try_spawn_empty()?;
        let mode = match profile {
            Profile::Global | Profile::ManyCameras => 3,
            Profile::Local => 1,
            Profile::Mixed => index % 4,
        };
        let position = index as f32 / 32.0;
        if mode & 1 != 0 {
            world.insert(
                id,
                Transform::from_translation(Vec3::new(position, -0.0, 1.0)),
            )?;
        }
        if mode & 2 != 0 {
            let transform = Transform::from_translation(Vec3::new(-position, 2.0, 3.0));
            world.insert(id, GlobalTransform(transform))?;
        }
        if matches!(profile, Profile::ManyCameras) || index < 2 {
            world.insert(id, Camera::default())?;
        }
        // Visibility is deliberately ignored by both old and current command contracts.
        world.insert(id, Visibility(index % 3 != 0))?;
        ids.push(id);
    }
    Ok((world, ids))
}

#[derive(Default)]
pub struct Capture {
    pub commands: Vec<RenderCommand>,
}

impl RenderBackend for Capture {
    fn begin_frame(&mut self, _info: FrameInfo) {
        self.commands.clear();
    }

    fn submit(&mut self, command: RenderCommand) {
        self.commands.push(command);
    }

    fn end_frame(&mut self) -> FrameStats {
        FrameStats {
            submitted_commands: self.commands.len(),
            drawn_pixels: 0,
        }
    }
}

/// All matrix/translation bits and order, not a lossy numerical tolerance or hash.
pub fn fingerprint(commands: &[RenderCommand]) -> Vec<(u8, Entity, [u32; 16])> {
    commands
        .iter()
        .map(|command| match *command {
            RenderCommand::SetCamera {
                entity,
                view_projection,
                world_position,
            } => {
                let mut bits = view_projection.data.map(f32::to_bits);
                // Fold translation into the fingerprint so position regressions fail closed.
                bits[0] ^= world_position.x.to_bits();
                bits[1] ^= world_position.y.to_bits();
                bits[2] ^= world_position.z.to_bits();
                (0, entity, bits)
            }
            RenderCommand::Transform {
                entity,
                translation,
            } => {
                let mut bits = [0; 16];
                bits[..3].copy_from_slice(
                    &[translation.x, translation.y, translation.z].map(f32::to_bits),
                );
                (1, entity, bits)
            }
        })
        .collect()
}
