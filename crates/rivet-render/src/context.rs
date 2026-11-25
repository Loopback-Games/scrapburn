//! Device, surface and frame acquisition.

use std::str::FromStr;
use std::sync::Arc;

use rivet_platform::{AdapterDesc, AdapterError, AdapterPreference, select_adapter};
use winit::window::Window;

use crate::DEPTH_FORMAT;

/// Presentation mode request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PresentChoice {
    #[default]
    Vsync,
    /// Uncapped; falls back to Mailbox, then Fifo.
    Immediate,
}

impl FromStr for PresentChoice {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "vsync" | "fifo" => Ok(Self::Vsync),
            "immediate" | "uncapped" => Ok(Self::Immediate),
            other => Err(format!(
                "unknown present mode '{other}' (use vsync or immediate)"
            )),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error(transparent)]
    Adapter(#[from] AdapterError),
    #[error("creating the window surface failed: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error("requesting the GPU device failed: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("the surface reports no supported formats on this adapter")]
    NoSurfaceFormat,
}

/// One acquired swapchain image.
pub struct Frame {
    pub texture: wgpu::SurfaceTexture,
    pub view: wgpu::TextureView,
}

/// Window-bound GPU state.
pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub adapter_info: wgpu::AdapterInfo,
    pub timestamps: bool,
    surface: wgpu::Surface<'static>,
    depth_view: wgpu::TextureView,
    needs_reconfigure: bool,
}

fn timestamp_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    adapter.features() & wgpu::Features::TIMESTAMP_QUERY
}

fn request_device(
    adapter: &wgpu::Adapter,
) -> Result<(wgpu::Device, wgpu::Queue), wgpu::RequestDeviceError> {
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("rivet device"),
        required_features: timestamp_features(adapter),
        required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
        ..Default::default()
    }))
}

fn pick(
    adapters: Vec<wgpu::Adapter>,
    pref: &AdapterPreference,
) -> Result<wgpu::Adapter, AdapterError> {
    let descs: Vec<AdapterDesc> = adapters
        .iter()
        .map(|a| AdapterDesc::from_info(&a.get_info()))
        .collect();
    let i = select_adapter(&descs, pref)?;
    Ok(adapters
        .into_iter()
        .nth(i)
        .expect("select_adapter returns an index into the list"))
}

/// A device without a surface (tests, tools).
pub fn headless_device(
    pref: &AdapterPreference,
) -> Result<(wgpu::Device, wgpu::Queue, wgpu::AdapterInfo), RenderError> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env());
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::PRIMARY));
    let adapter = pick(adapters, pref)?;
    let (device, queue) = request_device(&adapter)?;
    Ok((device, queue, adapter.get_info()))
}

fn create_depth(device: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

impl GpuContext {
    pub fn new(
        window: Arc<Window>,
        pref: &AdapterPreference,
        present: PresentChoice,
    ) -> Result<Self, RenderError> {
        let instance = wgpu::Instance::new(
            wgpu::InstanceDescriptor::new_with_display_handle(Box::new(window.clone())).with_env(),
        );
        let surface = instance.create_surface(window.clone())?;
        let adapters: Vec<wgpu::Adapter> =
            pollster::block_on(instance.enumerate_adapters(wgpu::Backends::PRIMARY))
                .into_iter()
                .filter(|a| a.is_surface_supported(&surface))
                .collect();
        let adapter = pick(adapters, pref)?;
        let adapter_info = adapter.get_info();
        log::info!(
            "GPU: {} ({:?}, {:?}, driver {} {})",
            adapter_info.name,
            adapter_info.device_type,
            adapter_info.backend,
            adapter_info.driver,
            adapter_info.driver_info
        );
        let (device, queue) = request_device(&adapter)?;
        let timestamps = device.features().contains(wgpu::Features::TIMESTAMP_QUERY);

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .ok_or(RenderError::NoSurfaceFormat)?;
        let present_mode = match present {
            PresentChoice::Vsync => wgpu::PresentMode::Fifo,
            PresentChoice::Immediate => [wgpu::PresentMode::Immediate, wgpu::PresentMode::Mailbox]
                .into_iter()
                .find(|m| caps.present_modes.contains(m))
                .unwrap_or(wgpu::PresentMode::Fifo),
        };
        log::info!(
            "present mode: {present_mode:?}, format {format:?}, GPU timestamps: {timestamps}"
        );
        let size = window.inner_size();
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps
                .alpha_modes
                .first()
                .copied()
                .unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &surface_config);
        let depth_view = create_depth(&device, surface_config.width, surface_config.height);
        Ok(Self {
            device,
            queue,
            surface_config,
            adapter_info,
            timestamps,
            surface,
            depth_view,
            needs_reconfigure: false,
        })
    }

    pub fn present_mode(&self) -> wgpu::PresentMode {
        self.surface_config.present_mode
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.surface_config.format
    }

    pub fn size(&self) -> (u32, u32) {
        (self.surface_config.width, self.surface_config.height)
    }

    pub fn depth_view(&self) -> &wgpu::TextureView {
        &self.depth_view
    }

    /// Reconfigures for a new window size. Zero sizes (minimized) are ignored.
    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 || (w, h) == self.size() {
            return;
        }
        self.surface_config.width = w;
        self.surface_config.height = h;
        self.surface.configure(&self.device, &self.surface_config);
        self.depth_view = create_depth(&self.device, w, h);
    }

    fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.surface_config);
    }

    /// Acquires the next image, or `None` when this frame should be skipped.
    pub fn begin_frame(&mut self) -> Option<Frame> {
        use wgpu::CurrentSurfaceTexture as C;
        let texture = match self.surface.get_current_texture() {
            C::Success(t) => t,
            C::Suboptimal(t) => {
                // Use this one, reconfigure before the next.
                let view = t.texture.create_view(&Default::default());
                let frame = Frame { texture: t, view };
                self.needs_reconfigure = true;
                return Some(frame);
            }
            C::Outdated | C::Lost => {
                log::info!("surface outdated/lost; reconfiguring");
                self.reconfigure();
                return None;
            }
            C::Timeout | C::Occluded => return None,
            C::Validation => {
                log::error!("surface validation error on acquire");
                return None;
            }
        };
        let view = texture.texture.create_view(&Default::default());
        Some(Frame { texture, view })
    }

    /// Presents a frame after its commands were submitted.
    pub fn present(&mut self, frame: Frame) {
        self.queue.present(frame.texture);
        if self.needs_reconfigure {
            self.needs_reconfigure = false;
            self.reconfigure();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_choice_parse() {
        assert_eq!(
            "immediate".parse::<PresentChoice>().unwrap(),
            PresentChoice::Immediate
        );
        assert_eq!(
            "vsync".parse::<PresentChoice>().unwrap(),
            PresentChoice::Vsync
        );
        assert_eq!(
            "fifo".parse::<PresentChoice>().unwrap(),
            PresentChoice::Vsync
        );
        assert!("bogus".parse::<PresentChoice>().is_err());
    }
}
