use extrem_assets::AssetId;

/// Audio playback command emitted by gameplay systems.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioCommand {
    Play {
        source: AssetId,
        volume: f32,
        looping: bool,
    },
    Stop {
        source: AssetId,
    },
    SetMasterVolume(f32),
}

/// Backend boundary for platform audio implementations.
pub trait AudioBackend {
    fn submit(&mut self, command: AudioCommand);
    fn end_frame(&mut self);
}

/// Deterministic backend for tests and headless tools.
#[derive(Clone, Debug, Default)]
pub struct NullAudioBackend {
    commands: Vec<AudioCommand>,
    frames: u64,
}

impl NullAudioBackend {
    pub fn commands(&self) -> &[AudioCommand] {
        &self.commands
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }
}

impl AudioBackend for NullAudioBackend {
    fn submit(&mut self, command: AudioCommand) {
        self.commands.push(command);
    }

    fn end_frame(&mut self) {
        self.frames = self.frames.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{AudioBackend, AudioCommand, NullAudioBackend};
    use extrem_assets::AssetId;

    #[test]
    fn null_backend_records_commands() {
        let mut backend = NullAudioBackend::default();
        backend.submit(AudioCommand::Play {
            source: AssetId::from_path("audio/click.ogg"),
            volume: 0.8,
            looping: false,
        });
        backend.end_frame();
        assert_eq!(backend.commands().len(), 1);
        assert_eq!(backend.frames(), 1);
    }
}
