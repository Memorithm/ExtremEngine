use std::collections::HashSet;
use std::hash::Hash;

/// Platform-neutral keyboard identifiers.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KeyCode {
    A,
    D,
    E,
    Escape,
    Q,
    S,
    Space,
    W,
    Unknown(u32),
}

/// Platform-neutral mouse buttons.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Other(u8),
}

/// Generic button state with edge transitions for one frame.
#[derive(Clone, Debug)]
pub struct ButtonInput<T> {
    pressed: HashSet<T>,
    just_pressed: HashSet<T>,
    just_released: HashSet<T>,
}

impl<T> Default for ButtonInput<T> {
    fn default() -> Self {
        Self {
            pressed: HashSet::new(),
            just_pressed: HashSet::new(),
            just_released: HashSet::new(),
        }
    }
}

impl<T: Eq + Hash + Copy> ButtonInput<T> {
    pub fn press(&mut self, button: T) {
        if self.pressed.insert(button) {
            self.just_pressed.insert(button);
        }
    }

    pub fn release(&mut self, button: T) {
        if self.pressed.remove(&button) {
            self.just_released.insert(button);
        }
    }

    pub fn pressed(&self, button: T) -> bool {
        self.pressed.contains(&button)
    }

    pub fn just_pressed(&self, button: T) -> bool {
        self.just_pressed.contains(&button)
    }

    pub fn just_released(&self, button: T) -> bool {
        self.just_released.contains(&button)
    }

    pub fn end_frame(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
    }

    pub fn clear(&mut self) {
        self.pressed.clear();
        self.end_frame();
    }
}

/// Mouse state accumulated between application frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MouseState {
    pub position: (f32, f32),
    pub delta: (f32, f32),
    pub wheel: f32,
}

impl MouseState {
    pub fn move_to(&mut self, x: f32, y: f32) {
        self.delta.0 += x - self.position.0;
        self.delta.1 += y - self.position.1;
        self.position = (x, y);
    }

    pub fn scroll(&mut self, amount: f32) {
        self.wheel += amount;
    }

    pub fn end_frame(&mut self) {
        self.delta = (0.0, 0.0);
        self.wheel = 0.0;
    }
}

/// All platform-neutral input state exposed as an ECS resource.
#[derive(Clone, Debug, Default)]
pub struct Input {
    pub keys: ButtonInput<KeyCode>,
    pub mouse_buttons: ButtonInput<MouseButton>,
    pub mouse: MouseState,
}

impl Input {
    pub fn end_frame(&mut self) {
        self.keys.end_frame();
        self.mouse_buttons.end_frame();
        self.mouse.end_frame();
    }
}

#[cfg(test)]
mod tests {
    use super::{Input, KeyCode};

    #[test]
    fn edges_last_for_one_frame() {
        let mut input = Input::default();
        input.keys.press(KeyCode::Space);
        assert!(input.keys.pressed(KeyCode::Space));
        assert!(input.keys.just_pressed(KeyCode::Space));
        input.end_frame();
        assert!(input.keys.pressed(KeyCode::Space));
        assert!(!input.keys.just_pressed(KeyCode::Space));
        input.keys.release(KeyCode::Space);
        assert!(input.keys.just_released(KeyCode::Space));
    }
}
