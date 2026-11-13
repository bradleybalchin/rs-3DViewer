//! # Vertex
//! 
//! Gpu point and binding / attribute descriptions, uniform buffer object

use std::{hash::{Hash, Hasher}};

use ash::vk;
use glam::{Mat4};
use bytemuck::{Pod, Zeroable};

// NOTE data in structure needs to be alligned in specific way (https://www.khronos.org/registry/vulkan/specs/1.3-extensions/html/chap15.html#interfaces-resources-layout)
// must match UBO in shader.vert
// mat4 is 64 bytes, so 3 in a rpw need no padding
// C representation -> defined layout. With this layout, the size of items is also stable as long as all fields have a stable size.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct UniformBufferObject
{
    pub(super) model : Mat4,
    pub(super) view  : Mat4,
    pub(super) proj  : Mat4,
}

// single point (position + colour)
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GpuPoint {
    pub pos: [f32; 3], // relative to `origin`
    pub color: u32,    // packed RGBA8
}

impl GpuPoint {
    pub fn new(pos: [f32; 3], color: u32) -> Self {
        Self { pos, color }
    }

    pub fn get_binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::default()
            // index of bindings in the array of bindings
            .binding(0)
            // bytes per entry
            .stride(std::mem::size_of::<GpuPoint>() as u32)
            // move to next data entry after each vertex (vs each instance of a vertex)
            .input_rate(vk::VertexInputRate::VERTEX)
    }

    pub fn get_attribute_descriptions() -> [vk::VertexInputAttributeDescription; 2] {
        [
            // position
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(0)
                .format(vk::Format::R32G32B32_SFLOAT) // position
                .offset(std::mem::offset_of!(GpuPoint, pos) as u32),
            // Colour
            vk::VertexInputAttributeDescription::default()
                .binding(0)
                .location(1)
                .format(vk::Format::R8G8B8A8_UNORM) // color
                .offset(std::mem::offset_of!(GpuPoint, color) as u32),
        ]
    }

}

impl PartialEq for GpuPoint {
    fn eq(&self, other: &Self) -> bool {
        self.pos == other.pos && self.color == other.color
    }
}

// For this to hold, must ensure strictly 3D coordinates (positions never contain NaN)
// Or can enforce NaN the same in partialEq using bytes
impl Eq for GpuPoint {}

impl Hash for GpuPoint {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Convert floats to bits before passing to the hasher
        self.pos[0].to_bits().hash(state);
        self.pos[1].to_bits().hash(state);
        self.pos[2].to_bits().hash(state);
        self.color.hash(state);
    }
}