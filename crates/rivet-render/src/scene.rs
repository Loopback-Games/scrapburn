//! The frame renderer: shadow pass, HDR scene (meshes + sky), bloom, tonemap, FXAA.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
use rivet_assets::Rgba8Image;
use wgpu::util::DeviceExt;

use crate::mesh::{GpuMesh, Vertex};
use crate::preset::{Preset, QualitySettings, bloom_sizes, scaled_size};
use crate::{DEPTH_FORMAT, GpuTimer, noise, shadow, texture};

const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const LDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MODEL_STRIDE: u64 = 256;
const MAX_MATERIALS: usize = 16;
/// Bloom starts at this HDR brightness (soft knee below it).
const BLOOM_THRESHOLD: f32 = 1.4;
const BLOOM_KNEE: f32 = 0.7;

/// Sun, sky and fog. Colors are linear; `Default` is a warm desert afternoon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lighting {
    /// Direction the sunlight travels.
    pub sun_dir: Vec3,
    pub sun_color: Vec3,
    pub sky_zenith: Vec3,
    pub sky_horizon: Vec3,
    pub ambient_ground: Vec3,
    pub fog_color: Vec3,
    pub fog_density: f32,
    pub fog_height_falloff: f32,
    pub cloud_coverage: f32,
    pub exposure: f32,
    pub bloom_intensity: f32,
    pub vignette: f32,
    pub saturation: f32,
}

impl Default for Lighting {
    fn default() -> Self {
        Self {
            sun_dir: Vec3::new(-0.45, -0.62, 0.64).normalize(),
            sun_color: Vec3::new(3.1, 2.75, 2.25),
            sky_zenith: Vec3::new(0.16, 0.36, 0.85),
            sky_horizon: Vec3::new(0.70, 0.74, 0.80),
            ambient_ground: Vec3::new(0.30, 0.22, 0.15),
            fog_color: Vec3::new(0.74, 0.68, 0.60),
            fog_density: 0.0011,
            fog_height_falloff: 0.03,
            cloud_coverage: 0.55,
            exposure: 0.85,
            bloom_intensity: 0.06,
            vignette: 0.35,
            saturation: 1.08,
        }
    }
}

/// One frame's camera and timing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub view: Mat4,
    pub proj: Mat4,
    pub eye: Vec3,
    /// Center of the shadow map (usually the player's kart).
    pub focus: Vec3,
    pub time: f32,
}

/// One mesh instance to draw.
pub struct Draw<'a> {
    pub mesh: &'a GpuMesh,
    pub model: Mat4,
    pub casts_shadow: bool,
}

/// Per-layer material shading: specular strength, gloss, triplanar scale (1/m).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialShading {
    pub specular: f32,
    pub gloss: f32,
    pub triplanar_scale: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: Mat4,
    inv_view_proj: Mat4,
    light_view_proj: Mat4,
    eye: Vec4,
    sun_dir: Vec4,
    sun_color: Vec4,
    sky_zenith: Vec4,
    sky_horizon: Vec4,
    ambient_ground: Vec4,
    fog: Vec4,
    fog_params: Vec4,
    cam_right: Vec4,
    cam_up: Vec4,
}

/// One camera-facing particle. Color is linear HDR; alpha is opacity (alpha mode) or a glow
/// multiplier (additive mode).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct ParticleInstance {
    pub pos: [f32; 3],
    pub size: f32,
    pub color: [f32; 4],
}

impl ParticleInstance {
    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<ParticleInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Float32x4],
    };
}

/// Everything drawn in one frame.
pub struct FrameContent<'a> {
    pub draws: &'a [Draw<'a>],
    pub additive: &'a [ParticleInstance],
    pub alpha: &'a [ParticleInstance],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct PostParams {
    texel: Vec4,
    a: Vec4,
    b: Vec4,
}

struct Targets {
    scene_size: (u32, u32),
    output_size: (u32, u32),
    scene_color: wgpu::TextureView,
    scene_depth: wgpu::TextureView,
    bloom: Vec<(wgpu::TextureView, (u32, u32))>,
    ldr: Option<wgpu::TextureView>,
    /// prefilter, down[1..], up[..], composite, fxaa
    post_groups: PostGroups,
}

struct PostGroups {
    prefilter: Option<wgpu::BindGroup>,
    down: Vec<wgpu::BindGroup>,
    up: Vec<wgpu::BindGroup>,
    composite: wgpu::BindGroup,
    composite_params: wgpu::Buffer,
    fxaa: Option<wgpu::BindGroup>,
}

pub struct SceneRenderer {
    settings: QualitySettings,
    globals_buf: wgpu::Buffer,
    shadow_globals_buf: wgpu::Buffer,
    model_buf: wgpu::Buffer,
    model_capacity: u64,
    model_bgl: wgpu::BindGroupLayout,
    model_bg: wgpu::BindGroup,
    scene_bg: wgpu::BindGroup,
    shadow_bg: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    post_sampler: wgpu::Sampler,
    post_bgl: wgpu::BindGroupLayout,
    black_view: wgpu::TextureView,
    mesh_pipeline: wgpu::RenderPipeline,
    sky_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    prefilter_pipeline: wgpu::RenderPipeline,
    down_pipeline: wgpu::RenderPipeline,
    up_pipeline: wgpu::RenderPipeline,
    composite_ldr_pipeline: wgpu::RenderPipeline,
    composite_out_pipeline: wgpu::RenderPipeline,
    fxaa_pipeline: wgpu::RenderPipeline,
    particle_add_pipeline: wgpu::RenderPipeline,
    particle_alpha_pipeline: wgpu::RenderPipeline,
    particle_buf: wgpu::Buffer,
    particle_capacity: u64,
    targets: Targets,
    pub lighting: Lighting,
}

fn uniform_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    dynamic: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: dynamic,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry(
    binding: u32,
    dim: wgpu::TextureViewDimension,
    sample: wgpu::TextureSampleType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: sample,
            view_dimension: dim,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32, ty: wgpu::SamplerBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(ty),
        count: None,
    }
}

fn render_texture(
    device: &wgpu::Device,
    label: &str,
    (w, h): (u32, u32),
    format: wgpu::TextureFormat,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

fn buffer(
    device: &wgpu::Device,
    label: &str,
    size: u64,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[allow(clippy::too_many_arguments)]
fn fullscreen_pipeline(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    fs: &str,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_fullscreen"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn model_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buf: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("models"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: buf,
                offset: 0,
                size: wgpu::BufferSize::new(64),
            }),
        }],
    })
}

impl SceneRenderer {
    /// `materials`: one mip chain per texture layer, all the same size. `shading[i]` describes
    /// layer `i` (missing entries default to matte).
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        output_format: wgpu::TextureFormat,
        output_size: (u32, u32),
        preset: Preset,
        materials: &[Vec<Rgba8Image>],
        shading: &[MaterialShading],
    ) -> Self {
        let settings = preset.settings();
        let src = |body: &str| format!("{}\n{}", include_str!("globals.wgsl"), body);
        let scene_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene.wgsl"),
            source: wgpu::ShaderSource::Wgsl(src(include_str!("scene.wgsl")).into()),
        });
        let shadow_module = device.create_shader_module(wgpu::include_wgsl!("shadow.wgsl"));
        let post_module = device.create_shader_module(wgpu::include_wgsl!("post.wgsl"));

        let vf = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let scene_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene layout"),
            entries: &[
                uniform_entry(0, vf, false),
                texture_entry(
                    1,
                    wgpu::TextureViewDimension::D2Array,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                sampler_entry(2, wgpu::SamplerBindingType::Filtering),
                texture_entry(
                    3,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Depth,
                ),
                sampler_entry(4, wgpu::SamplerBindingType::Comparison),
                uniform_entry(5, wgpu::ShaderStages::FRAGMENT, false),
                texture_entry(
                    6,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
            ],
        });
        let model_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model layout"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX, true)],
        });
        let shadow_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow layout"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX, false)],
        });
        let post_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post layout"),
            entries: &[
                texture_entry(
                    0,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
                sampler_entry(1, wgpu::SamplerBindingType::Filtering),
                uniform_entry(2, wgpu::ShaderStages::FRAGMENT, false),
                texture_entry(
                    3,
                    wgpu::TextureViewDimension::D2,
                    wgpu::TextureSampleType::Float { filterable: true },
                ),
            ],
        });

        let scene_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene pipeline layout"),
            bind_group_layouts: &[Some(&scene_bgl), Some(&model_bgl)],
            immediate_size: 0,
        });
        let sky_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky pipeline layout"),
            bind_group_layouts: &[Some(&scene_bgl)],
            immediate_size: 0,
        });
        let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow pipeline layout"),
            bind_group_layouts: &[Some(&shadow_bgl), Some(&model_bgl)],
            immediate_size: 0,
        });
        let post_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post pipeline layout"),
            bind_group_layouts: &[Some(&post_bgl)],
            immediate_size: 0,
        });

        let mesh_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState {
                module: &scene_module,
                entry_point: Some("vs_main"),
                buffers: &[Some(Vertex::LAYOUT)],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &scene_module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky"),
            layout: Some(&sky_layout),
            vertex: wgpu::VertexState {
                module: &scene_module,
                entry_point: Some("vs_sky"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &scene_module,
                entry_point: Some("fs_sky"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HDR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&shadow_layout),
            vertex: wgpu::VertexState {
                module: &shadow_module,
                entry_point: Some("vs_shadow"),
                buffers: &[Some(Vertex::LAYOUT)],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: SHADOW_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: Default::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        let particle_pipeline = |label: &str, fs: &str, blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&sky_layout),
                vertex: wgpu::VertexState {
                    module: &scene_module,
                    entry_point: Some("vs_particle"),
                    buffers: &[Some(ParticleInstance::LAYOUT)],
                    compilation_options: Default::default(),
                },
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &scene_module,
                    entry_point: Some(fs),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: HDR_FORMAT,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let add_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let particle_add_pipeline =
            particle_pipeline("particles additive", "fs_particle_add", add_blend);
        let particle_alpha_pipeline = particle_pipeline(
            "particles alpha",
            "fs_particle_alpha",
            wgpu::BlendState::ALPHA_BLENDING,
        );
        let particle_capacity = 2048;
        let particle_buf = buffer(
            device,
            "particles",
            particle_capacity * std::mem::size_of::<ParticleInstance>() as u64,
            wgpu::BufferUsages::VERTEX,
        );
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::REPLACE,
        };
        let prefilter_pipeline = fullscreen_pipeline(
            device,
            "bloom prefilter",
            &post_layout,
            &post_module,
            "fs_prefilter",
            HDR_FORMAT,
            None,
        );
        let down_pipeline = fullscreen_pipeline(
            device,
            "bloom down",
            &post_layout,
            &post_module,
            "fs_down",
            HDR_FORMAT,
            None,
        );
        let up_pipeline = fullscreen_pipeline(
            device,
            "bloom up",
            &post_layout,
            &post_module,
            "fs_up",
            HDR_FORMAT,
            Some(additive),
        );
        let composite_ldr_pipeline = fullscreen_pipeline(
            device,
            "composite ldr",
            &post_layout,
            &post_module,
            "fs_composite",
            LDR_FORMAT,
            None,
        );
        let composite_out_pipeline = fullscreen_pipeline(
            device,
            "composite out",
            &post_layout,
            &post_module,
            "fs_composite",
            output_format,
            None,
        );
        let fxaa_pipeline = fullscreen_pipeline(
            device,
            "fxaa",
            &post_layout,
            &post_module,
            "fs_fxaa",
            output_format,
            None,
        );

        let globals_buf = buffer(
            device,
            "globals",
            std::mem::size_of::<Globals>() as u64,
            wgpu::BufferUsages::UNIFORM,
        );
        let shadow_globals_buf = buffer(device, "shadow globals", 64, wgpu::BufferUsages::UNIFORM);
        let model_capacity = 64;
        let model_buf = buffer(
            device,
            "models",
            model_capacity * MODEL_STRIDE,
            wgpu::BufferUsages::UNIFORM,
        );
        let model_bg = model_bind_group(device, &model_bgl, &model_buf);

        let material_buf = buffer(
            device,
            "material params",
            (MAX_MATERIALS * 16) as u64,
            wgpu::BufferUsages::UNIFORM,
        );
        let mut params = [Vec4::new(0.0, 16.0, 0.125, 0.0); MAX_MATERIALS];
        for (i, s) in shading.iter().take(MAX_MATERIALS).enumerate() {
            params[i] = Vec4::new(s.specular, s.gloss, s.triplanar_scale, 0.0);
        }
        queue.write_buffer(&material_buf, 0, bytemuck::cast_slice(&params));

        let materials_view =
            texture::material_array(device, queue, materials, settings.texture_skip_mips);
        let cloud_size = 256;
        let clouds = noise::tileable_fbm(cloud_size, 5, 1337);
        let clouds_view = texture::texture_2d(
            device,
            queue,
            "clouds",
            wgpu::TextureFormat::R8Unorm,
            (cloud_size as u32, cloud_size as u32),
            1,
            &clouds,
        );
        let black_view = texture::texture_2d(
            device,
            queue,
            "black",
            wgpu::TextureFormat::Rgba8Unorm,
            (1, 1),
            4,
            &[0, 0, 0, 255],
        );

        let mesh_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("material sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: settings.anisotropy,
            ..Default::default()
        });
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let post_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let (shadow_view, scene_bg, shadow_bg) = Self::shadow_resources(
            device,
            &settings,
            &scene_bgl,
            &shadow_bgl,
            &globals_buf,
            &shadow_globals_buf,
            &materials_view,
            &mesh_sampler,
            &shadow_sampler,
            &material_buf,
            &clouds_view,
        );

        let targets = Self::make_targets(
            device,
            &settings,
            output_size,
            &post_bgl,
            &post_sampler,
            &black_view,
        );
        Self {
            settings,
            globals_buf,
            shadow_globals_buf,
            model_buf,
            model_capacity,
            model_bgl,
            model_bg,
            scene_bg,
            shadow_bg,
            shadow_view,
            post_sampler,
            post_bgl,
            black_view,
            mesh_pipeline,
            sky_pipeline,
            shadow_pipeline,
            prefilter_pipeline,
            down_pipeline,
            up_pipeline,
            composite_ldr_pipeline,
            composite_out_pipeline,
            fxaa_pipeline,
            particle_add_pipeline,
            particle_alpha_pipeline,
            particle_buf,
            particle_capacity,
            targets,
            lighting: Lighting::default(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn shadow_resources(
        device: &wgpu::Device,
        settings: &QualitySettings,
        scene_bgl: &wgpu::BindGroupLayout,
        shadow_bgl: &wgpu::BindGroupLayout,
        globals_buf: &wgpu::Buffer,
        shadow_globals_buf: &wgpu::Buffer,
        materials_view: &wgpu::TextureView,
        mesh_sampler: &wgpu::Sampler,
        shadow_sampler: &wgpu::Sampler,
        material_buf: &wgpu::Buffer,
        clouds_view: &wgpu::TextureView,
    ) -> (wgpu::TextureView, wgpu::BindGroup, wgpu::BindGroup) {
        let s = settings.shadow_size;
        let shadow_view = render_texture(device, "shadow map", (s, s), SHADOW_FORMAT);
        let scene_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene bind group"),
            layout: scene_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(materials_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(mesh_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: material_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(clouds_view),
                },
            ],
        });
        let shadow_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow bind group"),
            layout: shadow_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: shadow_globals_buf.as_entire_binding(),
            }],
        });
        (shadow_view, scene_bg, shadow_bg)
    }

    fn post_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        src: &wgpu::TextureView,
        src2: &wgpu::TextureView,
        params: PostParams,
    ) -> (wgpu::BindGroup, wgpu::Buffer) {
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("post params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post bind group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(src2),
                },
            ],
        });
        (bg, buf)
    }

    fn make_targets(
        device: &wgpu::Device,
        settings: &QualitySettings,
        output_size: (u32, u32),
        post_bgl: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        black: &wgpu::TextureView,
    ) -> Targets {
        let scene_size = scaled_size(output_size.0, output_size.1, settings.render_scale);
        let scene_color = render_texture(device, "scene color", scene_size, HDR_FORMAT);
        let scene_depth = render_texture(device, "scene depth", scene_size, DEPTH_FORMAT);
        let bloom: Vec<_> = bloom_sizes(scene_size.0, scene_size.1, settings.bloom_levels)
            .into_iter()
            .map(|s| (render_texture(device, "bloom", s, HDR_FORMAT), s))
            .collect();
        let ldr = settings
            .fxaa
            .then(|| render_texture(device, "ldr", output_size, LDR_FORMAT));
        // Texel size of each pass's source; threshold/knee for the prefilter.
        let p = |src: (u32, u32)| PostParams {
            texel: Vec4::new(1.0 / src.0 as f32, 1.0 / src.1 as f32, 0.0, 0.0),
            a: Vec4::new(BLOOM_THRESHOLD, BLOOM_KNEE, 0.0, 1.0),
            b: Vec4::ZERO,
        };
        let prefilter = bloom.first().map(|_| {
            Self::post_group(
                device,
                post_bgl,
                sampler,
                &scene_color,
                black,
                p(scene_size),
            )
            .0
        });
        let down = (1..bloom.len())
            .map(|i| {
                Self::post_group(
                    device,
                    post_bgl,
                    sampler,
                    &bloom[i - 1].0,
                    black,
                    p(bloom[i - 1].1),
                )
                .0
            })
            .collect();
        let up = (1..bloom.len())
            .rev()
            .map(|i| {
                Self::post_group(device, post_bgl, sampler, &bloom[i].0, black, p(bloom[i].1)).0
            })
            .collect();
        let bloom_src = bloom.first().map_or(black, |b| &b.0);
        let (composite, composite_params) = Self::post_group(
            device,
            post_bgl,
            sampler,
            &scene_color,
            bloom_src,
            p(scene_size),
        );
        let fxaa = ldr
            .as_ref()
            .map(|l| Self::post_group(device, post_bgl, sampler, l, black, p(output_size)).0);
        Targets {
            scene_size,
            output_size,
            scene_color,
            scene_depth,
            bloom,
            ldr,
            post_groups: PostGroups {
                prefilter,
                down,
                up,
                composite,
                composite_params,
                fxaa,
            },
        }
    }

    pub fn settings(&self) -> QualitySettings {
        self.settings
    }

    pub fn scene_size(&self) -> (u32, u32) {
        self.targets.scene_size
    }

    pub fn resize(&mut self, device: &wgpu::Device, output_size: (u32, u32)) {
        if output_size == self.targets.output_size || output_size.0 == 0 || output_size.1 == 0 {
            return;
        }
        self.targets = Self::make_targets(
            device,
            &self.settings,
            output_size,
            &self.post_bgl,
            &self.post_sampler,
            &self.black_view,
        );
    }

    fn write_models(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, draws: &[Draw<'_>]) {
        let n = draws.len() as u64;
        if n > self.model_capacity {
            self.model_capacity = n.next_power_of_two();
            self.model_buf = buffer(
                device,
                "models",
                self.model_capacity * MODEL_STRIDE,
                wgpu::BufferUsages::UNIFORM,
            );
            self.model_bg = model_bind_group(device, &self.model_bgl, &self.model_buf);
        }
        let mut bytes = vec![0u8; (n * MODEL_STRIDE) as usize];
        for (i, d) in draws.iter().enumerate() {
            let o = i * MODEL_STRIDE as usize;
            bytes[o..o + 64].copy_from_slice(bytemuck::bytes_of(&d.model));
        }
        if !bytes.is_empty() {
            queue.write_buffer(&self.model_buf, 0, &bytes);
        }
    }

    /// Exposure, bloom, vignette and saturation come from `lighting` and may change per frame.
    fn write_post_params(&self, queue: &wgpu::Queue) {
        let l = &self.lighting;
        let s = self.targets.scene_size;
        let bloom = if self.targets.bloom.is_empty() {
            0.0
        } else {
            l.bloom_intensity
        };
        let p = PostParams {
            texel: Vec4::new(1.0 / s.0 as f32, 1.0 / s.1 as f32, 0.0, 0.0),
            a: Vec4::new(BLOOM_THRESHOLD, BLOOM_KNEE, bloom, l.exposure),
            b: Vec4::new(l.vignette, l.saturation, 0.0, 0.0),
        };
        queue.write_buffer(
            &self.targets.post_groups.composite_params,
            0,
            bytemuck::bytes_of(&p),
        );
    }

    /// Renders the 3D frame into `output` (the HUD is drawn afterwards by the caller).
    // A frame genuinely needs device, queue, encoder, target, view, draws and timer.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        view: &View,
        content: &FrameContent<'_>,
        mut timer: Option<&mut GpuTimer>,
    ) {
        let draws = content.draws;
        let l = self.lighting;
        let radius = 70.0;
        let light_vp =
            shadow::light_view_proj(l.sun_dir, view.focus, radius, self.settings.shadow_size);
        let view_proj = view.proj * view.view;
        let globals = Globals {
            view_proj,
            inv_view_proj: view_proj.inverse(),
            light_view_proj: light_vp,
            eye: view.eye.extend(view.time),
            sun_dir: l
                .sun_dir
                .normalize()
                .extend(1.0 / self.settings.shadow_size as f32),
            sun_color: l.sun_color.extend(0.0),
            sky_zenith: l.sky_zenith.extend(0.0),
            sky_horizon: l.sky_horizon.extend(l.cloud_coverage),
            ambient_ground: l.ambient_ground.extend(0.0),
            fog: l.fog_color.extend(l.fog_density),
            fog_params: Vec4::new(l.fog_height_falloff, 0.0, 0.0, 0.0),
            cam_right: view.view.row(0).truncate().normalize().extend(0.0),
            cam_up: view.view.row(1).truncate().normalize().extend(0.0),
        };
        queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));
        queue.write_buffer(&self.shadow_globals_buf, 0, bytemuck::bytes_of(&light_vp));
        self.write_models(device, queue, draws);
        self.write_post_params(queue);
        let n_add = content.additive.len() as u64;
        let n_alpha = content.alpha.len() as u64;
        if n_add + n_alpha > self.particle_capacity {
            self.particle_capacity = (n_add + n_alpha).next_power_of_two();
            self.particle_buf = buffer(
                device,
                "particles",
                self.particle_capacity * std::mem::size_of::<ParticleInstance>() as u64,
                wgpu::BufferUsages::VERTEX,
            );
        }
        let stride = std::mem::size_of::<ParticleInstance>() as u64;
        if n_add > 0 {
            queue.write_buffer(
                &self.particle_buf,
                0,
                bytemuck::cast_slice(content.additive),
            );
        }
        if n_alpha > 0 {
            queue.write_buffer(
                &self.particle_buf,
                n_add * stride,
                bytemuck::cast_slice(content.alpha),
            );
        }

        // --- Shadow pass ---
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: timer.as_deref_mut().and_then(GpuTimer::begin_writes),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.shadow_pipeline);
            pass.set_bind_group(0, &self.shadow_bg, &[]);
            for (i, d) in draws.iter().enumerate().filter(|(_, d)| d.casts_shadow) {
                pass.set_bind_group(1, &self.model_bg, &[(i as u64 * MODEL_STRIDE) as u32]);
                pass.set_vertex_buffer(0, d.mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(d.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..d.mesh.index_count, 0, 0..1);
            }
        }

        // --- Scene pass (HDR) ---
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.scene_color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.scene_depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.mesh_pipeline);
            pass.set_bind_group(0, &self.scene_bg, &[]);
            for (i, d) in draws.iter().enumerate() {
                pass.set_bind_group(1, &self.model_bg, &[(i as u64 * MODEL_STRIDE) as u32]);
                pass.set_vertex_buffer(0, d.mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(d.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..d.mesh.index_count, 0, 0..1);
            }
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(0, &self.scene_bg, &[]);
            pass.draw(0..3, 0..1);
            if n_alpha > 0 {
                pass.set_pipeline(&self.particle_alpha_pipeline);
                pass.set_vertex_buffer(0, self.particle_buf.slice(n_add * stride..));
                pass.draw(0..6, 0..n_alpha as u32);
            }
            if n_add > 0 {
                pass.set_pipeline(&self.particle_add_pipeline);
                pass.set_vertex_buffer(0, self.particle_buf.slice(..n_add * stride));
                pass.draw(0..6, 0..n_add as u32);
            }
        }

        // --- Bloom ---
        let pg = &self.targets.post_groups;
        let bloom = &self.targets.bloom;
        let fullscreen =
            |encoder: &mut wgpu::CommandEncoder,
             label: &str,
             target: &wgpu::TextureView,
             load: wgpu::LoadOp<wgpu::Color>,
             pipeline: &wgpu::RenderPipeline,
             bg: &wgpu::BindGroup,
             ts: Option<wgpu::RenderPassTimestampWrites<'_>>| {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(label),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: ts,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, bg, &[]);
                pass.draw(0..3, 0..1);
            };
        let clear = wgpu::LoadOp::Clear(wgpu::Color::BLACK);
        if let Some(pre) = &pg.prefilter {
            fullscreen(
                encoder,
                "bloom prefilter",
                &bloom[0].0,
                clear,
                &self.prefilter_pipeline,
                pre,
                None,
            );
            for (i, bg) in pg.down.iter().enumerate() {
                fullscreen(
                    encoder,
                    "bloom down",
                    &bloom[i + 1].0,
                    clear,
                    &self.down_pipeline,
                    bg,
                    None,
                );
            }
            for (k, bg) in pg.up.iter().enumerate() {
                let dst = bloom.len() - 2 - k;
                fullscreen(
                    encoder,
                    "bloom up",
                    &bloom[dst].0,
                    wgpu::LoadOp::Load,
                    &self.up_pipeline,
                    bg,
                    None,
                );
            }
        }

        // --- Composite (+ FXAA) ---
        match (&self.targets.ldr, &pg.fxaa) {
            (Some(ldr), Some(fxaa)) => {
                fullscreen(
                    encoder,
                    "composite",
                    ldr,
                    clear,
                    &self.composite_ldr_pipeline,
                    &pg.composite,
                    None,
                );
                let end = timer.as_deref().and_then(GpuTimer::end_writes);
                fullscreen(
                    encoder,
                    "fxaa",
                    output,
                    clear,
                    &self.fxaa_pipeline,
                    fxaa,
                    end,
                );
            }
            _ => {
                let end = timer.as_deref().and_then(GpuTimer::end_writes);
                fullscreen(
                    encoder,
                    "composite",
                    output,
                    clear,
                    &self.composite_out_pipeline,
                    &pg.composite,
                    end,
                );
            }
        }
    }
}
