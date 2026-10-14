use std::path::Path;

use iced::wgpu::{self, util::DeviceExt};

use crate::data::{Mesh, Vertex};

/// Lädt eine .obj-Datei und erstellt daraus eine GPU-Mesh-Struktur.
pub fn load_mesh_from_obj(
    device: &wgpu::Device,
    path: impl AsRef<Path>,
) -> Result<Mesh, Box<dyn std::error::Error>> {
    // 1. OBJ-Datei laden. tobj spaltet die Datei automatisch in "Models" (Teilnetze) auf.
    // Für dieses Beispiel fusionieren wir sie oder laden das erste gefundene Modell.
    let (models, _materials) = tobj::load_obj(
        path.as_ref(),
        &tobj::LoadOptions {
            triangulate: true,  // Erzwingt, dass alle Polygone Dreiecke sind
            single_index: true, // Vereint Positions-, Normalen- und UV-Indizes
            ..Default::default()
        },
    )?;

    // Wir nehmen das erste Modell aus der Datei (oder kombinieren sie bei Bedarf)
    let mesh_data = &models
        .first()
        .ok_or("Die OBJ-Datei enthält keine Geometrie.")?
        .mesh;

    // 2. Rohdaten in unsere Vertex-Struktur überführen
    let mut vertices: Vec<Vertex> = Vec::new();
    let num_vertices = mesh_data.positions.len() / 3;

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
            ] // Y-Achse für WGPU oft flippen
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

    // 3. Indizes konvertieren (tobj gibt u32 aus, Ihr Code verlangt u16)
    // HINWEIS: Wenn Ihr Modell mehr als 65.535 Vertices hat, müssen Sie in Ihrem
    // `render_pass.set_index_buffer` auf `wgpu::IndexFormat::Uint32` umstellen!
    let indices: Vec<u16> = mesh_data.indices.iter().map(|&idx| idx as u16).collect();

    let index_count = indices.len() as u32;

    // 4. WGPU Vertex Buffer erstellen
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("{:?} Vertex Buffer", path.as_ref())),
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });

    // 5. WGPU Index Buffer erstellen
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("{:?} Index Buffer", path.as_ref())),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    Ok(Mesh {
        vertex_buffer,
        index_buffer,
        index_count,
    })
}
