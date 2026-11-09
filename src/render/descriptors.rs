//! # Descriptors
//! 
//! Descriptor set layouts, pools, and sets

use ash::vk;

use crate::render::resources::{Buffer};
use crate::render::vertex::{UniformBufferObject};

/// Create descriptor set layout for describing resources to shaders (uniforms, textures, etc.)
pub(super) fn create_descriptor_set_layout(device : &ash::Device) -> Result<vk::DescriptorSetLayout, String> {

    let ubo_binding = vk::DescriptorSetLayoutBinding::default()
    .binding(0)
    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
    .descriptor_count(1)            
    .stage_flags(vk::ShaderStageFlags::VERTEX);         // vertex shader

    let bindings = [ubo_binding];

    // Create descriptor set layout
    let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
    .bindings(&bindings);

    unsafe { device.create_descriptor_set_layout(&layout_info, None) }
    .map_err(|e| format!("Failed to create descriptor set layout: {e}"))
}

/// Create descriptor pool where desciptor sets are allocated from
pub(super) fn create_descriptor_pool(device : &ash::Device, max_frames_in_flight:u32) -> Result<vk::DescriptorPool,String>{

    let pool_sizes = [vk::DescriptorPoolSize::default()
        .ty(vk::DescriptorType::UNIFORM_BUFFER)
        .descriptor_count(max_frames_in_flight)];
    let pool_create_info = vk::DescriptorPoolCreateInfo::default()
        .pool_sizes(&pool_sizes)
        .max_sets(max_frames_in_flight);

    unsafe {device.create_descriptor_pool(&pool_create_info, None)
        .map_err(|e| format!("Failed to create descriptor pool {e}")) } 
    
}

/// Create descriptor sets (for holding descriptors, such as uniform buffers)
pub(super) fn create_descriptor_sets(
    device : &ash::Device, 
    max_frames_in_flight:u32, 
    descriptor_set_layout : vk::DescriptorSetLayout, 
    descriptor_pool : vk::DescriptorPool,
    uniform_buffers : &[Buffer])
    -> Result<Vec<vk::DescriptorSet>,String>{
    
    let set_layouts = vec![descriptor_set_layout;max_frames_in_flight as usize];
    let alloc_info = vk::DescriptorSetAllocateInfo::default()
        .descriptor_pool(descriptor_pool)
        .set_layouts(&set_layouts);

    let descriptor_sets = unsafe {
        device.allocate_descriptor_sets(&alloc_info)
            .map_err(|e| format!("Failed to allocate decriptor sets : {e}"))
    }?;

    // populate each desciptor set with uniform buffers
    for i in 0..max_frames_in_flight {
        let buffer_info = [vk::DescriptorBufferInfo::default()
            .buffer(uniform_buffers[i as usize].buffer)
            .offset(0)
            .range(size_of::<UniformBufferObject>() as u64)];    

        let descriptor_write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_sets[i as usize])
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(&buffer_info);

        unsafe {device.update_descriptor_sets(&[descriptor_write], &[])};

    }

    Ok(descriptor_sets)
}