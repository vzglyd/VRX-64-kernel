//! Slide loading, manifest validation, and SlideSpec decoding.
//!
//! This module handles:
//! - Parsing and validating slide manifests
//! - Reading SlideSpec from shared memory layout (wire format v2 only)
//! - Validating slides against hardware limits
//!
//! # Wire Format
//!
//! Only wire format version 2 (shared memory layout) is supported.
//! Legacy postcard-encoded slides (version 1) are NOT supported.

pub mod manifest;

pub use manifest::{ManifestAssets, ManifestRequirements, ManifestShaders, ManifestSidecar, SlideManifest};

use thiserror::Error;
use vzglyd_slide::Limits;

/// Wire format version for shared memory layout.
pub const WIRE_VERSION: u32 = 2;

/// Errors that can occur during slide spec decoding.
#[derive(Debug, Error)]
pub enum SpecDecodeError {
    /// Unsupported wire format version.
    #[error("unsupported slide wire version {0}, expected {WIRE_VERSION}")]
    UnsupportedVersion(u32),
    /// Invalid UTF-8 in string.
    #[error("invalid UTF-8 in string: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),
    /// Memory out of bounds.
    #[error("memory access out of bounds: {0}")]
    OutOfBounds(String),
}

/// A view into a slide spec that borrows from WASM memory.
///
/// This is used for zero-copy reading of shared memory layout (v2).
pub struct SlideSpecView<'a> {
    /// Slide name (borrowed from memory).
    pub name: &'a str,
    /// Resource limits.
    pub limits: Limits,
    /// Scene space (0 = Screen2D, 1 = World3D).
    pub scene_space: u32,
    /// Textures (borrowed from memory).
    pub textures: &'a [crate::shared_mem::TextureDescMem],
    /// Static meshes (borrowed from memory).
    pub static_meshes: &'a [crate::shared_mem::StaticMeshMem],
    /// Dynamic meshes (borrowed from memory).
    pub dynamic_meshes: &'a [crate::shared_mem::DynamicMeshMem],
    /// Draw specs (borrowed from memory).
    pub draws: &'a [crate::shared_mem::DrawSpecMem],
}

/// Reads a SlideSpec directly from shared memory (wire format v2).
///
/// # Arguments
/// * `memory` - The full WASM linear memory slice
/// * `header_ptr` - Pointer to the SlideSpecHeader
///
/// # Returns
/// * `Ok(SlideSpecView)` with borrowed views into memory
/// * `Err(SpecDecodeError)` if validation fails
pub fn decode_spec_from_memory<'a>(
    memory: &'a [u8],
    header_ptr: u32,
) -> Result<SlideSpecView<'a>, SpecDecodeError> {
    use crate::shared_mem::SlideSpecHeader;
    
    // Read header
    let header = read_struct::<SlideSpecHeader>(memory, header_ptr)?;
    
    // Validate version
    if header.version != crate::shared_mem::WIRE_VERSION {
        return Err(SpecDecodeError::UnsupportedVersion(header.version));
    }
    
    // Read name string
    let name = read_string(memory, header.name_ptr, header.name_len)?;
    
    // Read texture array
    let textures = read_array::<crate::shared_mem::TextureDescMem>(
        memory,
        header.textures_offset,
        header.textures_count,
    )?;
    
    // Read static mesh array
    let static_meshes = read_array::<crate::shared_mem::StaticMeshMem>(
        memory,
        header.static_meshes_offset,
        header.static_meshes_count,
    )?;
    
    // Read dynamic mesh array
    let dynamic_meshes = read_array::<crate::shared_mem::DynamicMeshMem>(
        memory,
        header.dynamic_meshes_offset,
        header.dynamic_meshes_count,
    )?;
    
    // Read draw spec array
    let draws = read_array::<crate::shared_mem::DrawSpecMem>(
        memory,
        header.draws_offset,
        header.draws_count,
    )?;
    
    // Convert limits from memory format
    let limits = Limits {
        max_vertices: header.limits.max_vertices,
        max_indices: header.limits.max_indices,
        max_static_meshes: header.limits.max_static_meshes,
        max_dynamic_meshes: header.limits.max_dynamic_meshes,
        max_textures: header.limits.max_textures,
        max_texture_bytes: header.limits.max_texture_bytes,
        max_texture_dim: header.limits.max_texture_dim,
    };
    
    Ok(SlideSpecView {
        name,
        limits,
        scene_space: header.scene_space,
        textures,
        static_meshes,
        dynamic_meshes,
        draws,
    })
}

/// Reads a POD struct from memory at the given pointer.
fn read_struct<'a, T: bytemuck::Pod>(
    memory: &'a [u8],
    ptr: u32,
) -> Result<&'a T, SpecDecodeError> {
    let start = ptr as usize;
    let end = start + core::mem::size_of::<T>();
    
    let bytes = memory
        .get(start..end)
        .ok_or_else(|| SpecDecodeError::OutOfBounds(format!("struct at {} out of bounds", ptr)))?;
    
    Ok(bytemuck::from_bytes(bytes))
}

/// Reads a string from memory.
fn read_string<'a>(
    memory: &'a [u8],
    ptr: u32,
    len: u32,
) -> Result<&'a str, SpecDecodeError> {
    if len == 0 {
        return Ok("");
    }
    
    let start = ptr as usize;
    let end = start + len as usize;
    
    let bytes = memory
        .get(start..end)
        .ok_or_else(|| SpecDecodeError::OutOfBounds(format!("string at {} out of bounds", ptr)))?;
    
    std::str::from_utf8(bytes).map_err(SpecDecodeError::InvalidUtf8)
}

/// Reads an array of POD types from memory.
fn read_array<'a, T: bytemuck::Pod>(
    memory: &'a [u8],
    offset: u32,
    count: u32,
) -> Result<&'a [T], SpecDecodeError> {
    if count == 0 {
        return Ok(&[]);
    }
    
    let start = offset as usize;
    let byte_size = count as usize * core::mem::size_of::<T>();
    let end = start + byte_size;
    
    let bytes = memory
        .get(start..end)
        .ok_or_else(|| SpecDecodeError::OutOfBounds(format!("array at {} out of bounds", offset)))?;
    
    Ok(bytemuck::cast_slice(bytes))
}

/// Reads a runtime overlay from shared memory.
///
/// # Arguments
/// * `memory` - WASM linear memory
/// * `ptr` - Pointer to RuntimeOverlayHeader
/// * `len` - Size of header (0 = no overlay)
///
/// # Returns
/// * `Ok(Some(OverlayView))` if overlay exists
/// * `Ok(None)` if len is 0
/// * `Err(SpecDecodeError)` if validation fails
pub fn read_overlay_from_memory<'a, V: bytemuck::Pod>(
    memory: &'a [u8],
    ptr: u32,
    len: u32,
) -> Result<Option<OverlayView<'a, V>>, SpecDecodeError> {
    use crate::shared_mem::RuntimeOverlayHeader;
    
    if len == 0 {
        return Ok(None);
    }
    
    let header = read_struct::<RuntimeOverlayHeader>(memory, ptr)?;
    
    let vertices = read_array::<V>(
        memory,
        header.vertices_ptr,
        header.vertices_count,
    )?;
    
    let indices = read_array::<u16>(
        memory,
        header.indices_ptr,
        header.indices_count,
    )?;
    
    Ok(Some(OverlayView { vertices, indices }))
}

/// A view into a runtime overlay.
pub struct OverlayView<'a, V> {
    /// Vertex slice (borrowed from memory).
    pub vertices: &'a [V],
    /// Index slice (borrowed from memory).
    pub indices: &'a [u16],
}

/// Reads runtime mesh updates from shared memory.
///
/// # Arguments
/// * `memory` - WASM linear memory
/// * `ptr` - Pointer to RuntimeMeshSetHeader
/// * `len` - Size of header (0 = no updates)
///
/// # Returns
/// * `Ok(Some(MeshSetView))` if updates exist
/// * `Ok(None)` if len is 0
/// * `Err(SpecDecodeError)` if validation fails
pub fn read_dynamic_meshes_from_memory<'a, V: bytemuck::Pod>(
    memory: &'a [u8],
    ptr: u32,
    len: u32,
) -> Result<Option<MeshSetView<'a, V>>, SpecDecodeError> {
    use crate::shared_mem::{RuntimeMeshHeader, RuntimeMeshSetHeader};
    
    if len == 0 {
        return Ok(None);
    }
    
    let set_header = read_struct::<RuntimeMeshSetHeader>(memory, ptr)?;
    
    let mesh_headers = read_array::<RuntimeMeshHeader>(
        memory,
        set_header.meshes_offset,
        set_header.meshes_count,
    )?;
    
    let mut meshes = Vec::with_capacity(mesh_headers.len());
    
    for mesh_header in mesh_headers {
        let vertices = read_array::<V>(
            memory,
            mesh_header.vertices_ptr,
            mesh_header.vertices_count,
        )?;
        
        meshes.push(MeshUpdateView {
            mesh_index: mesh_header.mesh_index,
            vertices,
            index_count: mesh_header.index_count,
        });
    }
    
    Ok(Some(MeshSetView { meshes }))
}

/// A view into a set of mesh updates.
pub struct MeshSetView<'a, V> {
    /// Individual mesh updates.
    pub meshes: Vec<MeshUpdateView<'a, V>>,
}

/// A view into a single mesh update.
pub struct MeshUpdateView<'a, V> {
    /// Index into the dynamic mesh array.
    pub mesh_index: u32,
    /// Vertex slice (borrowed from memory).
    pub vertices: &'a [V],
    /// Number of indices to use.
    pub index_count: u32,
}

/// Returns the standard Pi 4 limits.
pub fn pi4_limits() -> Limits {
    Limits::pi4()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_mem::{SharedMemoryBuilder, SlideSpecHeader, LimitsMem};

    #[test]
    fn unsupported_version() {
        let mut builder = SharedMemoryBuilder::new();
        let (name_ptr, name_len) = builder.write_string("Test");
        
        let header = SlideSpecHeader {
            version: 99,  // Wrong version
            name_ptr,
            name_len,
            ..Default::default()
        };
        
        let layout = builder.build(header);
        let memory = layout.as_slice();
        
        let result = decode_spec_from_memory(memory, 0);
        assert!(matches!(result, Err(SpecDecodeError::UnsupportedVersion(99))));
    }

    #[test]
    fn shared_memory_decode() {
        // Build a shared memory layout
        let mut builder = SharedMemoryBuilder::new();
        let (name_ptr, name_len) = builder.write_string("Test Slide");
        
        let header = SlideSpecHeader {
            version: WIRE_VERSION,
            name_ptr,
            name_len,
            limits: LimitsMem {
                max_vertices: 60000,
                max_indices: 120000,
                ..Default::default()
            },
            scene_space: 0, // Screen2D
            ..Default::default()
        };
        
        let layout = builder.build(header);
        let memory = layout.as_slice();
        
        // Decode from memory
        let result = decode_spec_from_memory(memory, 0);
        assert!(result.is_ok());
        
        let spec_view = result.unwrap();
        assert_eq!(spec_view.name, "Test Slide");
        assert_eq!(spec_view.limits.max_vertices, 60000);
        assert_eq!(spec_view.scene_space, 0);
    }
}
