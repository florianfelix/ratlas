use std::path::Path;

use iced::wgpu::{self, util::DeviceExt};

use crate::data::load_mesh_from_obj;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub tex_coords: [f32; 2],
    pub normal: [f32; 3],
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
        wgpu::VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: wgpu::VertexFormat::Float32x3,
        },
        wgpu::VertexAttribute {
            offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
            shader_location: 1,
            format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
            offset: (std::mem::size_of::<[f32; 3]>() + std::mem::size_of::<[f32; 2]>())
                as wgpu::BufferAddress,
            shader_location: 2,
            format: wgpu::VertexFormat::Float32x3,
        },
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

// 1. Holds the raw GPU geometry data (Vertices and Indices).
pub struct Mesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

impl Mesh {
    pub fn from_obj(
        device: &wgpu::Device,
        path: impl AsRef<Path>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        load_mesh_from_obj(device, path)
    }

    pub fn fallback_triangle(device: &wgpu::Device) -> Self {
        let vertices = [
            Vertex {
                position: [0.0, 0.6, 0.0],
                tex_coords: [0.5, 1.0],
                normal: [0.0, 0.0, 1.0],
            },
            Vertex {
                position: [-0.6, -0.6, 0.0],
                tex_coords: [0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
            },
            Vertex {
                position: [0.6, -0.6, 0.0],
                tex_coords: [1.0, 0.0],
                normal: [0.0, 0.0, 1.0],
            },
        ];
        let indices: [u16; 3] = [0, 1, 2];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Fallback Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Fallback Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            vertex_buffer,
            index_buffer,
            index_count: 3,
        }
    }
}
