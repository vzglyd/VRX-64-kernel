//! Shared memory layouts for zero-copy WASM communication.
//!
//! This module defines `#[repr(C)]` structures that slides write to WASM linear memory
//! and the kernel reads directly without serialization/deserialization.
//!
//! # Wire Format Version 2
//!
//! The shared memory layout replaces postcard serialization (version 1) with
//! explicit offsets and sizes. The header contains pointers into linear memory
//! where strings and arrays are stored sequentially.
//!
//! ```text
//! WASM Linear Memory Layout:
//! ┌─────────────────┬─────────────────┬─────────────────┬─────────────────┐
//! │ SlideSpecHeader │ String data     │ Array data      │ Vertex data     │
//! │ (fixed size)    │ (sequential)    │ (sequential)    │ (sequential)    │
//! └─────────────────┴─────────────────┴─────────────────┴─────────────────┘
//!      │                    │                 │                 │
//!      │                    ▼                 ▼                 ▼
//!      │              name_ptr          textures_offset   vertices_ptr
//!      │              name_len          textures_count    vertices_count
//!      ▼
//! header_ptr (from vzglyd_spec_ptr)
//! ```
//!
//! # Usage in Slides
//!
//! Slides use `SharedMemoryBuilder` to construct the shared memory layout,
//! then export the header pointer via `vzglyd_spec_ptr()`.
//!
//! See the kernel documentation for complete examples.

use std::collections::VecDeque;

/// Wire format version for shared memory layout.
pub const WIRE_VERSION: u32 = 2;

/// Maximum size for a single string in shared memory (64KB).
pub const MAX_STRING_LEN: u32 = 64 * 1024;

/// Maximum number of elements in any array (1M elements).
pub const MAX_ARRAY_LEN: u32 = 1024 * 1024;

/// Slide specification header in shared memory.
///
/// This struct has a fixed size and contains offsets/lengths for
/// variable-size data stored elsewhere in WASM linear memory.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SlideSpecHeader {
    /// Wire format version (must be 2).
    pub version: u32,
    /// Pointer to slide name string.
    pub name_ptr: u32,
    /// Length of slide name string in bytes.
    pub name_len: u32,
    /// Resource limits (inline, fixed size).
    pub limits: LimitsMem,
    /// Scene space (0 = Screen2D, 1 = World3D).
    pub scene_space: u32,
    /// Offset to camera path (0 = None).
    pub camera_path_offset: u32,
    /// Number of textures.
    pub textures_count: u32,
    /// Offset to texture array.
    pub textures_offset: u32,
    /// Number of static meshes.
    pub static_meshes_count: u32,
    /// Offset to static mesh array.
    pub static_meshes_offset: u32,
    /// Number of dynamic mesh slots.
    pub dynamic_meshes_count: u32,
    /// Offset to dynamic mesh array.
    pub dynamic_meshes_offset: u32,
    /// Number of draw specs.
    pub draws_count: u32,
    /// Offset to draw spec array.
    pub draws_offset: u32,
    /// Number of overlay vertices (0 = no overlay).
    pub overlay_vertices_count: u32,
    /// Offset to overlay vertices.
    pub overlay_vertices_offset: u32,
    /// Number of overlay indices.
    pub overlay_indices_count: u32,
    /// Offset to overlay indices.
    pub overlay_indices_offset: u32,
    /// Reserved for future use (padding).
    pub _reserved: [u32; 8],
}

impl Default for SlideSpecHeader {
    fn default() -> Self {
        Self {
            version: WIRE_VERSION,
            name_ptr: 0,
            name_len: 0,
            limits: LimitsMem::default(),
            scene_space: 0,
            camera_path_offset: 0,
            textures_count: 0,
            textures_offset: 0,
            static_meshes_count: 0,
            static_meshes_offset: 0,
            dynamic_meshes_count: 0,
            dynamic_meshes_offset: 0,
            draws_count: 0,
            draws_offset: 0,
            overlay_vertices_count: 0,
            overlay_vertices_offset: 0,
            overlay_indices_count: 0,
            overlay_indices_offset: 0,
            _reserved: [0; 8],
        }
    }
}

/// Resource limits (matches vzglyd_slide::Limits layout).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LimitsMem {
    pub max_vertices: u32,
    pub max_indices: u32,
    pub max_static_meshes: u32,
    pub max_dynamic_meshes: u32,
    pub max_textures: u32,
    pub max_texture_bytes: u32,
    pub max_texture_dim: u32,
    pub _padding: u32,
}

/// Texture descriptor in shared memory.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TextureDescMem {
    /// Pointer to label string.
    pub label_ptr: u32,
    /// Length of label string.
    pub label_len: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Format (0 = Rgba8Unorm).
    pub format: u32,
    /// Wrap mode U (0 = Repeat, 1 = ClampToEdge).
    pub wrap_u: u32,
    /// Wrap mode V.
    pub wrap_v: u32,
    /// Wrap mode W.
    pub wrap_w: u32,
    /// Mag filter (0 = Nearest, 1 = Linear).
    pub mag_filter: u32,
    /// Min filter.
    pub min_filter: u32,
    /// Mip filter.
    pub mip_filter: u32,
    /// Pointer to RGBA8 pixel data.
    pub data_ptr: u32,
    /// Length of pixel data in bytes.
    pub data_len: u32,
}

/// Static mesh in shared memory.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct StaticMeshMem {
    /// Pointer to label string.
    pub label_ptr: u32,
    /// Length of label string.
    pub label_len: u32,
    /// Pointer to vertex array.
    pub vertices_ptr: u32,
    /// Number of vertices (not bytes).
    pub vertices_count: u32,
    /// Pointer to index array.
    pub indices_ptr: u32,
    /// Number of indices.
    pub indices_count: u32,
}

/// Dynamic mesh slot in shared memory.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DynamicMeshMem {
    /// Pointer to label string.
    pub label_ptr: u32,
    /// Length of label string.
    pub label_len: u32,
    /// Maximum vertex capacity.
    pub max_vertices: u32,
    /// Pointer to static index array.
    pub indices_ptr: u32,
    /// Number of indices.
    pub indices_count: u32,
}

/// Draw specification in shared memory.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawSpecMem {
    /// Pointer to label string.
    pub label_ptr: u32,
    /// Length of label string.
    pub label_len: u32,
    /// Source type (0 = Static, 1 = Dynamic).
    pub source_type: u32,
    /// Source index (which mesh to draw).
    pub source_index: u32,
    /// Pipeline (0 = Opaque, 1 = Transparent).
    pub pipeline: u32,
    /// First index to draw.
    pub index_start: u32,
    /// Number of indices to draw.
    pub index_count: u32,
}

/// Camera path in shared memory.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraPathMem {
    /// Whether the path loops.
    pub looped: u32,
    /// Number of keyframes.
    pub keyframes_count: u32,
    /// Offset to keyframe array.
    pub keyframes_offset: u32,
}

/// Camera keyframe in shared memory.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraKeyframeMem {
    pub time: f32,
    pub position: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    pub fov_y_deg: f32,
}

/// Runtime overlay header (for per-frame updates).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RuntimeOverlayHeader {
    /// Pointer to vertex array.
    pub vertices_ptr: u32,
    /// Number of vertices.
    pub vertices_count: u32,
    /// Pointer to index array.
    pub indices_ptr: u32,
    /// Number of indices.
    pub indices_count: u32,
}

/// Runtime mesh update header.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RuntimeMeshHeader {
    /// Index into dynamic mesh array.
    pub mesh_index: u32,
    /// Pointer to vertex array.
    pub vertices_ptr: u32,
    /// Number of vertices.
    pub vertices_count: u32,
    /// Number of indices to use (from static index buffer).
    pub index_count: u32,
}

/// Runtime mesh set header.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RuntimeMeshSetHeader {
    /// Number of mesh updates.
    pub meshes_count: u32,
    /// Offset to mesh header array.
    pub meshes_offset: u32,
}

/// Builder for constructing shared memory layouts.
///
/// This helper manages sequential allocation of strings and arrays
/// in WASM linear memory, returning offsets for the header.
pub struct SharedMemoryBuilder {
    /// The memory buffer (header + data).
    buffer: Vec<u8>,
    /// Current offset for string data.
    string_offset: u32,
    /// Current offset for array data.
    array_offset: u32,
    /// Header position (reserved at start).
    header_size: u32,
}

impl SharedMemoryBuilder {
    /// Creates a new builder with default capacity.
    pub fn new() -> Self {
        Self::with_capacity(8192)
    }

    /// Creates a new builder with specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        // Reserve space for header at the start
        let header_size = core::mem::size_of::<SlideSpecHeader>() as u32;
        let mut buffer = Vec::with_capacity(capacity);
        buffer.resize(header_size as usize, 0u8);

        Self {
            buffer,
            string_offset: header_size,
            array_offset: header_size,
            header_size,
        }
    }

    /// Returns the current buffer length.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Returns true if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Writes a string to memory, returns (ptr, len).
    pub fn write_string(&mut self, s: &str) -> (u32, u32) {
        let bytes = s.as_bytes();
        let len = bytes.len() as u32;

        assert!(len <= MAX_STRING_LEN, "String too long: {} bytes", len);

        let ptr = self.string_offset;
        self.buffer.extend_from_slice(bytes);
        self.string_offset += len;

        (ptr, len)
    }

    /// Writes an array of POD types to memory, returns (ptr, count).
    pub fn write_array<T: bytemuck::Pod>(&mut self, items: &[T]) -> (u32, u32) {
        let count = items.len() as u32;

        assert!(
            count <= MAX_ARRAY_LEN,
            "Array too long: {} elements",
            count
        );

        let ptr = self.array_offset;
        let bytes = bytemuck::cast_slice(items);
        self.buffer.extend_from_slice(bytes);
        self.array_offset += bytes.len() as u32;

        (ptr, count)
    }

    /// Writes raw bytes to memory at a specific alignment.
    pub fn write_bytes(&mut self, bytes: &[u8], align: usize) -> (u32, u32) {
        // Align the offset
        let offset = self.array_offset as usize;
        let aligned_offset = (offset + align - 1) & !(align - 1);
        let padding = aligned_offset - offset;

        for _ in 0..padding {
            self.buffer.push(0);
        }

        let ptr = (aligned_offset as u32).max(self.array_offset);
        self.buffer.extend_from_slice(bytes);
        let len = bytes.len() as u32;
        self.array_offset = ptr + len;

        (ptr, len)
    }

    /// Builds the final memory layout with the given header.
    pub fn build(mut self, header: SlideSpecHeader) -> SharedMemoryLayout {
        // Write header at the start
        let header_bytes = bytemuck::bytes_of(&header);
        self.buffer[..self.header_size as usize].copy_from_slice(header_bytes);

        SharedMemoryLayout {
            buffer: self.buffer,
            header_size: self.header_size,
        }
    }

    /// Returns the current string offset (for manual header construction).
    pub fn current_string_offset(&self) -> u32 {
        self.string_offset
    }

    /// Returns the current array offset (for manual header construction).
    pub fn current_array_offset(&self) -> u32 {
        self.array_offset
    }
}

impl Default for SharedMemoryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Final shared memory layout ready for export.
pub struct SharedMemoryLayout {
    buffer: Vec<u8>,
    header_size: u32,
}

impl SharedMemoryLayout {
    /// Returns a pointer to the header.
    pub fn header_ptr(&self) -> *const u8 {
        self.buffer.as_ptr()
    }

    /// Returns a pointer to the header as a typed pointer.
    pub fn header(&self) -> &SlideSpecHeader {
        unsafe { &*(self.buffer.as_ptr() as *const SlideSpecHeader) }
    }

    /// Returns the full buffer (for embedding in static).
    pub fn into_buffer(self) -> Vec<u8> {
        self.buffer
    }

    /// Returns the buffer as a slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.buffer
    }

    /// Returns the header size.
    pub fn header_size(&self) -> u32 {
        self.header_size
    }

    /// Returns the total memory size.
    pub fn total_size(&self) -> usize {
        self.buffer.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_size_is_fixed() {
        // Header should be a reasonable fixed size
        let size = core::mem::size_of::<SlideSpecHeader>();
        assert!(size <= 256, "Header too large: {} bytes", size);
    }

    #[test]
    fn builder_writes_strings() {
        let mut builder = SharedMemoryBuilder::new();
        let (ptr1, len1) = builder.write_string("hello");
        let (ptr2, len2) = builder.write_string("world");

        assert_eq!(ptr1, builder.header_size);
        assert_eq!(len1, 5);
        assert_eq!(ptr2, ptr1 + len1);
        assert_eq!(len2, 5);
    }

    #[test]
    fn builder_writes_arrays() {
        let mut builder = SharedMemoryBuilder::new();
        let data = vec![1u32, 2, 3, 4, 5];
        let (ptr, count) = builder.write_array(&data);

        assert_eq!(count, 5);
        assert!(ptr >= builder.header_size);
    }

    #[test]
    fn builder_produces_valid_layout() {
        let mut builder = SharedMemoryBuilder::new();
        let (name_ptr, name_len) = builder.write_string("Test");

        let header = SlideSpecHeader {
            name_ptr,
            name_len,
            ..Default::default()
        };

        let layout = builder.build(header);

        // Verify header is at the start
        assert_eq!(layout.header().version, WIRE_VERSION);
        assert_eq!(layout.header().name_ptr, name_ptr);
        assert_eq!(layout.header().name_len, name_len);
    }

    #[test]
    fn string_roundtrip() {
        let mut builder = SharedMemoryBuilder::new();
        let test_string = "Hello, World! This is a test string.";
        let (ptr, len) = builder.write_string(test_string);

        let layout = builder.build(SlideSpecHeader::default());

        // Read back the string
        let start = ptr as usize;
        let end = start + len as usize;
        let bytes = &layout.as_slice()[start..end];
        let read_string = std::str::from_utf8(bytes).unwrap();

        assert_eq!(read_string, test_string);
    }

    #[test]
    fn array_roundtrip() {
        let mut builder = SharedMemoryBuilder::new();
        let test_data: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let (ptr, count) = builder.write_array(&test_data);

        let layout = builder.build(SlideSpecHeader::default());

        // Read back the array
        let start = ptr as usize;
        let end = start + (count as usize * core::mem::size_of::<f32>());
        let bytes = &layout.as_slice()[start..end];
        let read_data: &[f32] = bytemuck::cast_slice(bytes);

        assert_eq!(read_data, &test_data);
    }
}
