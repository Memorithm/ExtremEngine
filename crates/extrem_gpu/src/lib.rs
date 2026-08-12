use std::fmt;

/// Errors returned while initializing the graphics device.
#[derive(Debug)]
pub enum GpuError {
    Adapter(String),
    Device(String),
}

impl fmt::Display for GpuError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Adapter(message) => write!(formatter, "GPU adapter unavailable: {message}"),
            Self::Device(message) => write!(formatter, "GPU device unavailable: {message}"),
        }
    }
}

impl std::error::Error for GpuError {}

/// Owns the wgpu instance, adapter, device and queue for future render backends.
pub struct GpuContext {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl GpuContext {
    /// Requests a headless adapter and logical device without creating a window surface.
    pub fn headless() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            ..Default::default()
        }))
        .map_err(|error| GpuError::Adapter(error.to_string()))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ExtremEngine device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .map_err(|error| GpuError::Device(error.to_string()))?;
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
        })
    }

    pub fn instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn adapter_name(&self) -> String {
        self.adapter.get_info().name
    }
}
