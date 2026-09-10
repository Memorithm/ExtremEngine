use std::fmt;

#[derive(Debug)]
pub enum GpuError {
    Adapter(String),
    Device(String),
    Surface(String),
    SurfaceCapabilities(String),
}

impl fmt::Display for GpuError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Adapter(message) => write!(formatter, "GPU adapter unavailable: {message}"),
            Self::Device(message) => write!(formatter, "GPU device unavailable: {message}"),
            Self::Surface(message) => write!(formatter, "GPU surface creation failed: {message}"),
            Self::SurfaceCapabilities(message) => {
                write!(formatter, "GPU surface capabilities are unusable: {message}")
            }
        }
    }
}

impl std::error::Error for GpuError {}

#[derive(Clone, Copy, Debug)]
pub struct GpuContextOptions {
    pub power_preference: wgpu::PowerPreference,
}

impl Default for GpuContextOptions {
    fn default() -> Self {
        Self {
            power_preference: wgpu::PowerPreference::HighPerformance,
        }
    }
}

/// Owns the WGPU instance, adapter, device and queue.
pub struct GpuContext {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl GpuContext {
    pub fn headless() -> Result<Self, GpuError> {
        Self::headless_with_options(GpuContextOptions::default())
    }

    pub fn headless_with_options(options: GpuContextOptions) -> Result<Self, GpuError> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: options.power_preference,
            ..Default::default()
        }))
        .map_err(|error| GpuError::Adapter(error.to_string()))?;
        let (device, queue) = request_device(&adapter)?;
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
        })
    }

    /// Creates a surface first, then requests an adapter explicitly compatible with it.
    pub fn for_surface(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<(Self, SurfaceTarget), GpuError> {
        Self::for_surface_with_options(target, width, height, GpuContextOptions::default())
    }

    pub fn for_surface_with_options(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
        options: GpuContextOptions,
    ) -> Result<(Self, SurfaceTarget), GpuError> {
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(target)
            .map_err(|error| GpuError::Surface(error.to_string()))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: options.power_preference,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(|error| GpuError::Adapter(error.to_string()))?;
        let (device, queue) = request_device(&adapter)?;
        let surface_target = SurfaceTarget::from_surface(surface, &adapter, &device, width, height)?;
        Ok((
            Self {
                instance,
                adapter,
                device,
                queue,
            },
            surface_target,
        ))
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

fn request_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), GpuError> {
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("ExtremEngine device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        ..Default::default()
    }))
    .map_err(|error| GpuError::Device(error.to_string()))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfaceFrameStatus {
    Success,
    Suboptimal,
    Timeout,
    Occluded,
    Outdated,
    Lost,
    Validation,
}

pub enum SurfaceFrame {
    Renderable {
        texture: wgpu::SurfaceTexture,
        status: SurfaceFrameStatus,
    },
    Unavailable(SurfaceFrameStatus),
}

/// Configured window surface with explicit handling for every WGPU acquisition state.
pub struct SurfaceTarget {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

impl SurfaceTarget {
    pub fn new(
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, GpuError> {
        let surface = instance
            .create_surface(target)
            .map_err(|error| GpuError::Surface(error.to_string()))?;
        Self::from_surface(surface, adapter, device, width, height)
    }

    fn from_surface(
        surface: wgpu::Surface<'static>,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Result<Self, GpuError> {
        let capabilities = surface.get_capabilities(adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or_else(|| GpuError::SurfaceCapabilities("no texture format".to_owned()))?;
        let alpha_mode = capabilities
            .alpha_modes
            .first()
            .copied()
            .ok_or_else(|| GpuError::SurfaceCapabilities("no alpha mode".to_owned()))?;
        let present_mode = if capabilities.present_modes.contains(&wgpu::PresentMode::Fifo) {
            wgpu::PresentMode::Fifo
        } else {
            capabilities
                .present_modes
                .first()
                .copied()
                .ok_or_else(|| GpuError::SurfaceCapabilities("no present mode".to_owned()))?
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Srgb,
        };
        surface.configure(device, &config);
        Ok(Self { surface, config })
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) -> bool {
        if width == 0 || height == 0 {
            return false;
        }
        if self.config.width == width && self.config.height == height {
            return false;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(device, &self.config);
        true
    }

    pub fn reconfigure(&self, device: &wgpu::Device) {
        self.surface.configure(device, &self.config);
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn width(&self) -> u32 {
        self.config.width
    }

    pub fn height(&self) -> u32 {
        self.config.height
    }

    pub fn acquire_frame(&self) -> SurfaceFrame {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => SurfaceFrame::Renderable {
                texture,
                status: SurfaceFrameStatus::Success,
            },
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => SurfaceFrame::Renderable {
                texture,
                status: SurfaceFrameStatus::Suboptimal,
            },
            wgpu::CurrentSurfaceTexture::Timeout => {
                SurfaceFrame::Unavailable(SurfaceFrameStatus::Timeout)
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                SurfaceFrame::Unavailable(SurfaceFrameStatus::Occluded)
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                SurfaceFrame::Unavailable(SurfaceFrameStatus::Outdated)
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                SurfaceFrame::Unavailable(SurfaceFrameStatus::Lost)
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                SurfaceFrame::Unavailable(SurfaceFrameStatus::Validation)
            }
        }
    }
}
