//! Device, surface and frame acquisition.

use std::collections::VecDeque;
use std::str::FromStr;
use std::sync::Arc;

use rivet_platform::{
    AdapterDesc, AdapterError, AdapterPreference, candidate_order, select_adapter,
};
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
    #[error(
        "{adapter} can render but cannot present to this display ({detail}). The monitor is \
         probably attached to another GPU: pick that GPU with --adapter, or measure this one \
         with --bench <secs> --offscreen"
    )]
    CannotPresent { adapter: String, detail: String },
}

/// One frame's color target: a swapchain image, or the offscreen image.
pub struct Frame {
    pub view: wgpu::TextureView,
    surface_texture: Option<wgpu::SurfaceTexture>,
}

enum Target {
    Window {
        surface: wgpu::Surface<'static>,
        needs_reconfigure: bool,
    },
    /// Renders into a texture that is never shown; frames in flight are capped at two,
    /// like a swapchain, so timing stays honest.
    Offscreen {
        texture: wgpu::Texture,
        in_flight: VecDeque<wgpu::SubmissionIndex>,
    },
}

/// GPU device plus the target frames are drawn into.
pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub adapter_info: wgpu::AdapterInfo,
    pub timestamps: bool,
    target: Target,
    depth_view: wgpu::TextureView,
}

const OFFSCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const MAX_OFFSCREEN_IN_FLIGHT: usize = 2;

fn create_offscreen(device: &wgpu::Device, w: u32, h: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen color"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: OFFSCREEN_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
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

/// Creates a device on `adapter` and configures `surface`, catching "can render, can't
/// present" (e.g. an iGPU when the monitor is attached to the dGPU).
fn try_window(
    adapter: &wgpu::Adapter,
    surface: &wgpu::Surface<'static>,
    present: PresentChoice,
    w: u32,
    h: u32,
) -> Result<(wgpu::Device, wgpu::Queue, wgpu::SurfaceConfiguration), RenderError> {
    let (device, queue) = request_device(adapter)?;
    let caps = surface.get_capabilities(adapter);
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
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: w,
        height: h,
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
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    surface.configure(&device, &config);
    if let Some(err) = pollster::block_on(scope.pop()) {
        return Err(RenderError::CannotPresent {
            adapter: adapter.get_info().name,
            detail: err
                .to_string()
                .lines()
                .last()
                .unwrap_or("")
                .trim()
                .to_string(),
        });
    }
    Ok((device, queue, config))
}

impl GpuContext {
    /// `offscreen` renders into a hidden texture instead of the window (benchmarks on a GPU
    /// that cannot present to this display).
    pub fn new(
        window: Arc<Window>,
        pref: &AdapterPreference,
        present: PresentChoice,
        offscreen: bool,
    ) -> Result<Self, RenderError> {
        let instance = wgpu::Instance::new(
            wgpu::InstanceDescriptor::new_with_display_handle(Box::new(window.clone())).with_env(),
        );
        let all = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::PRIMARY));
        let size = window.inner_size();
        let (w, h) = (size.width.max(1), size.height.max(1));

        let (adapter, device, queue, target, surface_config) = if offscreen {
            let adapter = pick(all, pref)?;
            let (device, queue) = request_device(&adapter)?;
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: OFFSCREEN_FORMAT,
                width: w,
                height: h,
                present_mode: wgpu::PresentMode::Immediate,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
                color_space: wgpu::SurfaceColorSpace::Auto,
            };
            let texture = create_offscreen(&device, w, h);
            let target = Target::Offscreen {
                texture,
                in_flight: VecDeque::new(),
            };
            (adapter, device, queue, target, config)
        } else {
            let surface = instance.create_surface(window.clone())?;
            let supported: Vec<wgpu::Adapter> = all
                .into_iter()
                .filter(|a| a.is_surface_supported(&surface))
                .collect();
            let descs: Vec<AdapterDesc> = supported
                .iter()
                .map(|a| AdapterDesc::from_info(&a.get_info()))
                .collect();
            let order = candidate_order(&descs, pref, &rivet_platform::display_gpus());
            if order.is_empty() {
                select_adapter(&descs, pref)?; // produces the descriptive NoMatch error
            }
            let mut chosen = None;
            for (n, &i) in order.iter().enumerate() {
                let adapter = &supported[i];
                match try_window(adapter, &surface, present, w, h) {
                    Ok((device, queue, config)) => {
                        chosen = Some((i, device, queue, config));
                        break;
                    }
                    Err(e @ RenderError::CannotPresent { .. })
                        if *pref == AdapterPreference::Auto && n + 1 < order.len() =>
                    {
                        log::warn!("{e}; trying the next GPU");
                    }
                    Err(e) => return Err(e),
                }
            }
            let (i, device, queue, config) = chosen.ok_or(RenderError::NoSurfaceFormat)?;
            let adapter = supported
                .into_iter()
                .nth(i)
                .expect("index from candidate_order");
            let target = Target::Window {
                surface,
                needs_reconfigure: false,
            };
            (adapter, device, queue, target, config)
        };
        let adapter_info = adapter.get_info();
        log::info!(
            "GPU: {} ({:?}, {:?}, driver {} {})",
            adapter_info.name,
            adapter_info.device_type,
            adapter_info.backend,
            adapter_info.driver,
            adapter_info.driver_info
        );
        let timestamps = device.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let depth_view = create_depth(&device, w, h);
        let ctx = Self {
            device,
            queue,
            surface_config,
            adapter_info,
            timestamps,
            target,
            depth_view,
        };
        log::info!(
            "present: {}, format {:?}, GPU timestamps: {timestamps}",
            ctx.present_label(),
            ctx.format()
        );
        Ok(ctx)
    }

    /// `"Fifo"`, `"Immediate"`, `"Mailbox"`, or `"Offscreen"`.
    pub fn present_label(&self) -> String {
        match self.target {
            Target::Offscreen { .. } => "Offscreen".to_string(),
            Target::Window { .. } => format!("{:?}", self.surface_config.present_mode),
        }
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

    /// Resizes the target. Zero sizes (minimized) are ignored.
    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 || (w, h) == self.size() {
            return;
        }
        self.surface_config.width = w;
        self.surface_config.height = h;
        match &mut self.target {
            Target::Window { surface, .. } => surface.configure(&self.device, &self.surface_config),
            Target::Offscreen { texture, .. } => *texture = create_offscreen(&self.device, w, h),
        }
        self.depth_view = create_depth(&self.device, w, h);
    }

    /// Acquires the next color target, or `None` when this frame should be skipped.
    pub fn begin_frame(&mut self) -> Option<Frame> {
        use wgpu::CurrentSurfaceTexture as C;
        let (surface, needs_reconfigure) = match &mut self.target {
            Target::Offscreen { texture, .. } => {
                return Some(Frame {
                    view: texture.create_view(&Default::default()),
                    surface_texture: None,
                });
            }
            Target::Window {
                surface,
                needs_reconfigure,
            } => (surface, needs_reconfigure),
        };
        let texture = match surface.get_current_texture() {
            C::Success(t) => t,
            C::Suboptimal(t) => {
                // Use this one, reconfigure after presenting it.
                *needs_reconfigure = true;
                t
            }
            C::Outdated | C::Lost => {
                log::info!("surface outdated/lost; reconfiguring");
                surface.configure(&self.device, &self.surface_config);
                return None;
            }
            C::Timeout | C::Occluded => return None,
            C::Validation => {
                log::error!("surface validation error on acquire");
                return None;
            }
        };
        let view = texture.texture.create_view(&Default::default());
        Some(Frame {
            view,
            surface_texture: Some(texture),
        })
    }

    /// Submits a frame's commands. Offscreen, it also caps frames in flight.
    pub fn submit(&mut self, commands: wgpu::CommandBuffer) {
        let index = self.queue.submit([commands]);
        if let Target::Offscreen { in_flight, .. } = &mut self.target {
            in_flight.push_back(index);
            while in_flight.len() > MAX_OFFSCREEN_IN_FLIGHT {
                let oldest = in_flight.pop_front();
                let _ = self.device.poll(wgpu::PollType::Wait {
                    submission_index: oldest,
                    timeout: None,
                });
            }
        }
    }

    /// Reads back the offscreen image as tightly packed RGBA8 (sRGB). `None` for a window target.
    pub fn capture_offscreen(&self) -> Option<(u32, u32, Vec<u8>)> {
        let Target::Offscreen { texture, .. } = &self.target else {
            return None;
        };
        let (w, h) = self.size();
        let padded = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture"),
            size: padded as u64 * h as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let index = self.queue.submit([enc.finish()]);
        buffer.map_async(wgpu::MapMode::Read, .., |_| {});
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(index),
                timeout: None,
            })
            .ok()?;
        let view = buffer.get_mapped_range(..).ok()?;
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for row in view.chunks(padded as usize) {
            out.extend_from_slice(&row[..(w * 4) as usize]);
        }
        Some((w, h, out))
    }

    /// Presents a submitted frame (no-op offscreen).
    pub fn present(&mut self, frame: Frame) {
        let Some(texture) = frame.surface_texture else {
            return;
        };
        self.queue.present(texture);
        if let Target::Window {
            surface,
            needs_reconfigure,
        } = &mut self.target
            && *needs_reconfigure
        {
            *needs_reconfigure = false;
            surface.configure(&self.device, &self.surface_config);
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
