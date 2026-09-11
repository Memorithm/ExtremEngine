//! Platform window and event-loop integration.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use extrem_input::{Input, KeyCode, MouseButton};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode as WinitKeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "ExtremEngine".to_owned(),
            width: 1280,
            height: 720,
        }
    }
}

#[derive(Debug)]
pub enum WindowError {
    EventLoop(EventLoopError),
    Create(OsError),
}

impl Display for WindowError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EventLoop(error) => write!(formatter, "window event loop failed: {error}"),
            Self::Create(error) => write!(formatter, "window creation failed: {error}"),
        }
    }
}

impl Error for WindowError {}

pub fn process_window_event(input: &mut Input, event: &WindowEvent) {
    match event {
        WindowEvent::KeyboardInput { event, .. } => {
            if let PhysicalKey::Code(key_code) = event.physical_key {
                let mapped = map_winit_key(key_code);
                match event.state {
                    ElementState::Pressed => input.keys.press(mapped),
                    ElementState::Released => input.keys.release(mapped),
                }
            }
        }
        WindowEvent::MouseInput { state, button, .. } => {
            let mapped = map_winit_mouse_button(*button);
            match state {
                ElementState::Pressed => input.mouse_buttons.press(mapped),
                ElementState::Released => input.mouse_buttons.release(mapped),
            }
        }
        WindowEvent::CursorMoved { position, .. } => {
            if position.x.is_finite() && position.y.is_finite() {
                input.mouse.move_to(position.x as f32, position.y as f32);
            }
        }
        WindowEvent::MouseWheel { delta, .. } => {
            let amount = match delta {
                MouseScrollDelta::LineDelta(_, y) => *y,
                MouseScrollDelta::PixelDelta(position) => position.y as f32,
            };
            if amount.is_finite() {
                input.mouse.scroll(amount);
            }
        }
        WindowEvent::Focused(false) => {
            input.keys.clear();
            input.mouse_buttons.clear();
        }
        _ => {}
    }
}

fn map_winit_key(key: WinitKeyCode) -> KeyCode {
    match key {
        WinitKeyCode::KeyA => KeyCode::A,
        WinitKeyCode::ArrowDown => KeyCode::ArrowDown,
        WinitKeyCode::ArrowLeft => KeyCode::ArrowLeft,
        WinitKeyCode::ArrowRight => KeyCode::ArrowRight,
        WinitKeyCode::ArrowUp => KeyCode::ArrowUp,
        WinitKeyCode::ControlLeft | WinitKeyCode::ControlRight => KeyCode::Control,
        WinitKeyCode::KeyD => KeyCode::D,
        WinitKeyCode::KeyE => KeyCode::E,
        WinitKeyCode::Escape => KeyCode::Escape,
        WinitKeyCode::KeyQ => KeyCode::Q,
        WinitKeyCode::KeyS => KeyCode::S,
        WinitKeyCode::ShiftLeft | WinitKeyCode::ShiftRight => KeyCode::Shift,
        WinitKeyCode::Space => KeyCode::Space,
        WinitKeyCode::KeyW => KeyCode::W,
        _ => KeyCode::Unknown(key as u32),
    }
}

fn map_winit_mouse_button(button: winit::event::MouseButton) -> MouseButton {
    match button {
        winit::event::MouseButton::Left => MouseButton::Left,
        winit::event::MouseButton::Middle => MouseButton::Middle,
        winit::event::MouseButton::Right => MouseButton::Right,
        winit::event::MouseButton::Other(value) => MouseButton::Other(value as u8),
        _ => MouseButton::Other(255),
    }
}

pub struct WindowHost;

impl WindowHost {
    pub fn run(
        config: WindowConfig,
        mut on_frame: impl FnMut() + 'static,
    ) -> Result<(), WindowError> {
        Self::run_with_input(config, move |_window, _input| on_frame())
    }

    /// Provides an owned `Arc<Window>` so a renderer may safely retain a WGPU surface target.
    pub fn run_with_input(
        config: WindowConfig,
        on_frame: impl FnMut(&Arc<Window>, &mut Input) + 'static,
    ) -> Result<(), WindowError> {
        let event_loop = EventLoop::new().map_err(WindowError::EventLoop)?;
        let mut application = WindowApplication {
            config,
            window: None,
            input: Input::default(),
            on_frame: Box::new(on_frame),
            error: None,
        };
        event_loop
            .run_app(&mut application)
            .map_err(WindowError::EventLoop)?;
        application.error.map_or(Ok(()), Err)
    }
}

type WindowFrameCallback = Box<dyn FnMut(&Arc<Window>, &mut Input)>;

struct WindowApplication {
    config: WindowConfig,
    window: Option<Arc<Window>>,
    input: Input,
    on_frame: WindowFrameCallback,
    error: Option<WindowError>,
}

impl ApplicationHandler for WindowApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(self.config.title.clone())
            .with_inner_size(LogicalSize::new(self.config.width, self.config.height));
        match event_loop.create_window(attributes) {
            Ok(window) => self.window = Some(Arc::new(window)),
            Err(error) => {
                self.error = Some(WindowError::Create(error));
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if window.id() != window_id {
            return;
        }

        process_window_event(&mut self.input, &event);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                (self.on_frame)(window, &mut self.input);
                self.input.end_frame();
                window.request_redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{WindowConfig, process_window_event};
    use extrem_input::{Input, MouseButton};
    use winit::event::{ElementState, MouseScrollDelta, WindowEvent};

    #[test]
    fn default_window_is_hd_ready() {
        let config = WindowConfig::default();
        assert_eq!(config.width, 1280);
        assert_eq!(config.height, 720);
        assert_eq!(config.title, "ExtremEngine");
    }

    #[test]
    fn process_window_event_updates_mouse_state() {
        let mut input = Input::default();
        process_window_event(
            &mut input,
            &WindowEvent::CursorMoved {
                device_id: winit::event::DeviceId::dummy(),
                position: winit::dpi::PhysicalPosition::new(100.0, 200.0),
            },
        );
        assert_eq!(input.mouse.position, (100.0, 200.0));
        assert_eq!(input.mouse.delta, (100.0, 200.0));
        process_window_event(
            &mut input,
            &WindowEvent::MouseWheel {
                device_id: winit::event::DeviceId::dummy(),
                delta: MouseScrollDelta::LineDelta(0.0, 1.5),
                phase: winit::event::TouchPhase::Moved,
            },
        );
        assert_eq!(input.mouse.wheel, 1.5);
    }

    #[test]
    fn focus_loss_clears_stuck_buttons() {
        let mut input = Input::default();
        input.mouse_buttons.press(MouseButton::Left);
        process_window_event(&mut input, &WindowEvent::Focused(false));
        assert!(!input.mouse_buttons.pressed(MouseButton::Left));
    }

    #[test]
    fn mouse_button_translation_preserves_edges() {
        let mut input = Input::default();
        process_window_event(
            &mut input,
            &WindowEvent::MouseInput {
                device_id: winit::event::DeviceId::dummy(),
                state: ElementState::Pressed,
                button: winit::event::MouseButton::Left,
            },
        );
        assert!(input.mouse_buttons.just_pressed(MouseButton::Left));
    }
}
