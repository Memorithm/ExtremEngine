//! Platform window and event-loop integration.

use std::error::Error;
use std::fmt::{Display, Formatter};

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::error::{EventLoopError, OsError};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

/// Configuration used when creating the host window.
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

/// Errors returned by the window host.
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

/// Runs an engine frame callback from a native cross-platform event loop.
pub struct WindowHost;

impl WindowHost {
    /// Opens a window and invokes `on_frame` whenever a redraw is requested.
    pub fn run(config: WindowConfig, on_frame: impl FnMut() + 'static) -> Result<(), WindowError> {
        let event_loop = EventLoop::new().map_err(WindowError::EventLoop)?;
        let mut application = WindowApplication {
            config,
            window: None,
            on_frame: Box::new(on_frame),
            error: None,
        };
        event_loop
            .run_app(&mut application)
            .map_err(WindowError::EventLoop)?;
        application.error.map_or(Ok(()), Err)
    }
}

struct WindowApplication {
    config: WindowConfig,
    window: Option<Window>,
    on_frame: Box<dyn FnMut()>,
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
            Ok(window) => self.window = Some(window),
            Err(error) => {
                self.error = Some(WindowError::Create(error));
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                (self.on_frame)();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
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
    use super::WindowConfig;

    #[test]
    fn default_window_is_hd_ready() {
        let config = WindowConfig::default();
        assert_eq!(config.width, 1280);
        assert_eq!(config.height, 720);
        assert_eq!(config.title, "ExtremEngine");
    }
}
