//! GPU texture uploads: the material texture array and small single textures.

use rivet_assets::Rgba8Image;

/// All material layers in one 2D array texture with mipmaps. Every layer must have the same
/// size; `skip_mips` drops that many top levels (lower presets use half-resolution textures).
pub fn material_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layers: &[Vec<Rgba8Image>],
    skip_mips: u32,
) -> wgpu::TextureView {
    let levels = layers[0].len() as u32;
    let skip = skip_mips.min(levels - 1);
    let base = &layers[0][skip as usize];
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("materials"),
        size: wgpu::Extent3d {
            width: base.width,
            height: base.height,
            depth_or_array_layers: layers.len() as u32,
        },
        mip_level_count: levels - skip,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, chain) in layers.iter().enumerate() {
        assert_eq!(
            chain.len() as u32,
            levels,
            "every material layer needs the same mip count"
        );
        for (level, img) in chain.iter().enumerate().skip(skip as usize) {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32 - skip,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &img.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(img.width * 4),
                    rows_per_image: Some(img.height),
                },
                wgpu::Extent3d {
                    width: img.width,
                    height: img.height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    })
}

/// A single-level 2D texture from raw texels.
pub fn texture_2d(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    format: wgpu::TextureFormat,
    (width, height): (u32, u32),
    bytes_per_texel: u32,
    data: &[u8],
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * bytes_per_texel),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&Default::default())
}
