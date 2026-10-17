use std::{path::Path, sync::Arc};

use iced::wgpu;

pub struct Material {
    pub pipeline: Arc<wgpu::RenderPipeline>,
    pub texture_bind_group: Option<wgpu::BindGroup>,
}

impl Material {
    pub fn new(
        pipeline: Arc<wgpu::RenderPipeline>,
        texture_bind_group: Option<wgpu::BindGroup>,
    ) -> Self {
        Self {
            pipeline,
            texture_bind_group,
        }
    }

    pub fn unlit(pipeline: Arc<wgpu::RenderPipeline>) -> Self {
        Self {
            pipeline,
            texture_bind_group: None,
        }
    }

    pub fn from_image_path(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: Arc<wgpu::RenderPipeline>,
        layout: &wgpu::BindGroupLayout,
        path: impl AsRef<Path>,
    ) -> Self {
        let image_data = image::open(path.as_ref()).map(|img| img.to_rgba8()).ok();

        let (dimensions, rgba) = match &image_data {
            Some(img) => (img.dimensions(), img.as_raw().as_slice()),
            None => {
                tracing::warn!(
                    "Could not load texture from {:?}, using fallback white texture",
                    path.as_ref()
                );
                ((1, 1), [255u8, 255, 255, 255].as_slice())
            }
        };

        let texture_size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Material Diffuse Texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * dimensions.0),
                rows_per_image: Some(dimensions.1),
            },
            texture_size,
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material Texture Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        Self {
            pipeline,
            texture_bind_group: Some(texture_bind_group),
        }
    }
}
