use iced::wgpu;
use std::path::Path;
use wgpu::util::DeviceExt;

use crate::data::mesh::{Mesh, Vertex};

pub fn load_mesh_from_obj(
    device: &wgpu::Device,
    path: impl AsRef<Path>,
) -> Result<Mesh, Box<dyn std::error::Error>> {
    // 1. OBJ-Datei laden. tobj spaltet die Datei automatisch in "Models" (Teilnetze) auf.
    let (models, _materials) = tobj::load_obj(
        path.as_ref(),
        &tobj::LoadOptions {
            triangulate: true,
            single_index: true,
            ..Default::default()
        },
    )?;

    if models.is_empty() {
        return Err("No models found in OBJ file".into());
    }

    let mesh_data = &models[0].mesh;

    // 2. Vertices aus den interleaved Daten von tobj zusammenbauen
    let num_vertices = mesh_data.positions.len() / 3;
    let mut vertices: Vec<Vertex> = Vec::with_capacity(num_vertices);

    for i in 0..num_vertices {
        let pos = [
            mesh_data.positions[i * 3],
            mesh_data.positions[i * 3 + 1],
            mesh_data.positions[i * 3 + 2],
        ];

        let tex = if !mesh_data.texcoords.is_empty() {
            [
                mesh_data.texcoords[i * 2],
                1.0 - mesh_data.texcoords[i * 2 + 1],
            ] // Y-Achse für WGPU flippen
        } else {
            [0.0, 0.0]
        };

        let norm = if !mesh_data.normals.is_empty() {
            [
                mesh_data.normals[i * 3],
                mesh_data.normals[i * 3 + 1],
                mesh_data.normals[i * 3 + 2],
            ]
        } else {
            [0.0, 0.0, 0.0]
        };

        vertices.push(Vertex {
            position: pos,
            tex_coords: tex,
            normal: norm,
        });
    }

    // 3. Indizes konvertieren: Bei > 65535 Vertices Uint32 verwenden
    let needs_u32 = num_vertices > u16::MAX as usize
        || mesh_data.indices.iter().any(|&idx| idx > u16::MAX as u32);

    let (index_buffer, index_format) = if needs_u32 {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{:?} Index Buffer (u32)", path.as_ref())),
            contents: bytemuck::cast_slice(&mesh_data.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        (buffer, wgpu::IndexFormat::Uint32)
    } else {
        let indices_u16: Vec<u16> = mesh_data.indices.iter().map(|&idx| idx as u16).collect();
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{:?} Index Buffer (u16)", path.as_ref())),
            contents: bytemuck::cast_slice(&indices_u16),
            usage: wgpu::BufferUsages::INDEX,
        });
        (buffer, wgpu::IndexFormat::Uint16)
    };

    let index_count = mesh_data.indices.len() as u32;

    // 4. WGPU Vertex Buffer erstellen
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("{:?} Vertex Buffer", path.as_ref())),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });

    Ok(Mesh {
        vertex_buffer,
        index_buffer,
        index_count,
        index_format,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_load_country_outlines_obj() {
        let (models, _) = tobj::load_obj(
            "assets/country_outlines.obj",
            &tobj::LoadOptions {
                triangulate: true,
                single_index: true,
                ..Default::default()
            },
        )
        .expect("country_outlines.obj should parse");

        assert_eq!(models.len(), 1);
        let mesh = &models[0].mesh;
        println!(
            "Country outlines loaded: {} vertices, {} indices (triangles: {})",
            mesh.positions.len() / 3,
            mesh.indices.len(),
            mesh.indices.len() / 3
        );
        assert!(mesh.positions.len() / 3 > 0);
        assert!(!mesh.indices.is_empty());
    }
}
