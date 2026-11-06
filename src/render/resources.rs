//! Resources
//! 
//! Functions for creating and copying buffers, finding memory types, creating image resources.
//! Structs for images and buffers data objects

use ash::vk;

use crate::render::command;
use crate::render::vertex::{UniformBufferObject, GpuPoint};

/// Create a buffer object
fn create_buffer(instance : &ash::Instance, device : &ash::Device, physical_device : vk::PhysicalDevice,
    size : u64, 
    usage : vk::BufferUsageFlags, 
    properties : vk::MemoryPropertyFlags) -> Result<Buffer, String> {

    let buffer_create_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let buffer = unsafe {device.create_buffer(&buffer_create_info, None)}
        .map_err(|e| format!("Failed to create buffer : {e}"))?; 

    let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
    let memory_type_index = find_memory_type(instance, physical_device, requirements.memory_type_bits, properties)?;
    let alloc_info = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type_index);

    let memory = unsafe { device.allocate_memory(&alloc_info, None) }
    .map_err(|e| {
        unsafe { device.destroy_buffer(buffer, None) };
        format!("Failed to allocate readback memory: {e}")
    })?;

    unsafe {
        device.bind_buffer_memory(buffer, memory, 0)
    }.map_err(|e| format!("failed to bind buffer memory {e}"))?;

    Ok(Buffer {buffer, memory, mapped : std::ptr::null()})
}


/// Copy contents of one buffer to another through a command buffer \ 
/// Staging buffer -> vertex buffer
fn copy_buffer(device : &ash::Device, command_pool : vk::CommandPool, queue : vk::Queue, buffer_src : vk::Buffer, buffer_dst : vk::Buffer, size : u64)
-> Result<(), String>{

    let copy_regions = [vk::BufferCopy2::default()
        .src_offset(0)
        .dst_offset(0)
        .size(size)];
    let copy_info = vk::CopyBufferInfo2::default()
        .src_buffer(buffer_src)
        .dst_buffer(buffer_dst)
        .regions(&copy_regions);

    // Make the copied data readable by the vertex input stage
    let barriers = [vk::BufferMemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::COPY)                  // Wait for copy to finish
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)              // Make transfer writes available
        .dst_stage_mask(vk::PipelineStageFlags2::VERTEX_ATTRIBUTE_INPUT)// vertex input waits for copy
        .dst_access_mask(vk::AccessFlags2::VERTEX_ATTRIBUTE_READ)       // make data visible to vertex read
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .buffer(buffer_dst)
        .size(vk::WHOLE_SIZE)];
    let dependency = vk::DependencyInfo::default().buffer_memory_barriers(&barriers);

    // record the copy into a one-off command buffer, submit it and wait for it to finish
    command::submit_one_time(device, command_pool, queue, |cmd| unsafe {
        device.cmd_copy_buffer2(cmd, &copy_info);
        device.cmd_pipeline_barrier2(cmd, &dependency); // vertex buffer reads later depend on this (copy being completed)
    })
}


/// Creates the vertex buffer - stores vertex data that can be read by the graphics card
pub(super) fn create_vertex_buffer(instance : &ash::Instance, device : &ash::Device, physical_device : vk::PhysicalDevice,
    command_pool : vk::CommandPool,
    queue : vk::Queue,
    vertices : &[GpuPoint]) -> Result<Option<Buffer>, String>{
    
    // Do not create buffer if no vertices
    if vertices.is_empty() {return Ok(None);} 
    
    let bytes : &[u8] = bytemuck::cast_slice(vertices);
    let size = bytes.len() as vk::DeviceSize;
    let usage = vk::BufferUsageFlags::TRANSFER_SRC;
    let properties = vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE;

    // Staging buffer
    let stage_buffer = create_buffer(instance, device, physical_device, size, usage, properties)?;

    // Copy vertex data into the buffer - mapping buffer memory to CPU accesssible memory
    unsafe {
        let mapped = device.map_memory(stage_buffer.memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())
            .map_err(|e| format!("Failed to set up staging buffer: {e}"))? as *mut u8;

        std::ptr::copy_nonoverlapping(bytes.as_ptr(), mapped, bytes.len());   
        device.unmap_memory(stage_buffer.memory);         
    }

    // Create the vertex memoy (GPU-local memory)
    let usage = vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::VERTEX_BUFFER;
    let properties = vk::MemoryPropertyFlags::DEVICE_LOCAL;
    let vertex_buffer = create_buffer(instance, device, physical_device, size, usage, properties)?;

    // copy vertex data from staging to 
    copy_buffer(device, command_pool, queue, stage_buffer.buffer, vertex_buffer.buffer, size)?;

    // destroy staging buffer
    unsafe {
        device.destroy_buffer(stage_buffer.buffer, None);
        device.free_memory(stage_buffer.memory, None);
    }

    Ok(Some(vertex_buffer))
}


/// Create a buffer the rendered image is copied into so the CPU can read its pixel
/// size - bytes to hold, width * height * 4 for an RGBA8 image, so must be recreated on resize
/// The memory stays mapped for the buffer's lifetime. The caller is responsible for unmapping and destroying it.
pub(super) fn create_readback_buffer(instance : &ash::Instance, device : &ash::Device, physical_device : vk::PhysicalDevice, size : vk::DeviceSize) -> Result<ReadbackBuffer, String> {

    let buffer_create_info = vk::BufferCreateInfo::default()
    .size(size)
    .usage(vk::BufferUsageFlags::TRANSFER_DST)
    .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let buffer = unsafe { device.create_buffer(&buffer_create_info, None)}
    .map_err(|e| format!("Failed to create readback buffer: {e}"))?;

    let requirements = unsafe {device.get_buffer_memory_requirements(buffer)};

    // Prefer cached memory as CPU reads from uncached memory are slow
    // Coherent means the memory never needs manually invalidating, and Vulkan guarantees the fallback exists
    let coherent = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
    let memory_type_index = find_memory_type(instance, physical_device, requirements.memory_type_bits, coherent | vk::MemoryPropertyFlags::HOST_CACHED)
    .or_else(|_| find_memory_type(instance, physical_device, requirements.memory_type_bits, coherent))
    .map_err(|e| {
        unsafe { device.destroy_buffer(buffer, None) };
        e
    })?;

    let alloc_info = vk::MemoryAllocateInfo::default()
    .allocation_size(requirements.size)
    .memory_type_index(memory_type_index);

    let memory = unsafe { device.allocate_memory(&alloc_info, None) }
    .map_err(|e| {
        unsafe { device.destroy_buffer(buffer, None) };
        format!("Failed to allocate readback memory: {e}")
    })?;

    let mapped = unsafe {
        device.bind_buffer_memory(buffer, memory, 0)
        .and_then(|_| device.map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty()))
    }.map_err(|e| {
        unsafe {
            device.destroy_buffer(buffer, None);
            device.free_memory(memory, None);
        }
        format!("Failed to set up readback buffer: {e}")
    })? as *const u8;

    Ok(ReadbackBuffer { buffer, memory, mapped })
}


 /// Create uniform buffers for storing shader uniform values
pub(super) fn create_uniform_buffers(instance : &ash::Instance, device : &ash::Device, physical_device : vk::PhysicalDevice, max_frames_in_flight:u32)
    -> Result<Vec<Buffer>, String>{

    let size =  size_of::<UniformBufferObject>() as vk::DeviceSize;
    let mut uniform_buffers: Vec<Buffer> = vec![];
    let usage = vk::BufferUsageFlags::UNIFORM_BUFFER;
    let properties = vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE;

    for _ in 0..max_frames_in_flight {
        let mut buffer = create_buffer(instance, device, physical_device, size, usage, properties)?;
        buffer.mapped = unsafe {device.map_memory(buffer.memory, 0, size, vk::MemoryMapFlags::empty())
                                    .map_err(|e| format!("Failed to map uniform buffer memory {e}"))}? as *mut u8;
        uniform_buffers.push(buffer);
    }
    
    Ok(uniform_buffers)
}

/// Create a 2D render target: the image, its device-local memory and a view of the whole image
/// 
/// usage  - how the image will be used (e.g. COLOR_ATTACHMENT, DEPTH_STENCIL_ATTACHMENT) \
/// aspect - which part of the image the view accesses (COLOR or DEPTH) \
/// Fixed to the given size, so must be recreated on resize. The caller is responsible for destroying the returned resources.
pub(super) fn create_image_resources(
    instance : &ash::Instance, 
    device : &ash::Device, 
    physical_device : vk::PhysicalDevice,
    format : vk::Format,
    width :  u32,
    height : u32,
    usage : vk::ImageUsageFlags,
    aspect : vk::ImageAspectFlags) -> Result<ImageResources,String>{

        let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D{ width, height, depth:1})
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)   // must match find_depth_format
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);

    let image = unsafe { device.create_image(&image_info, None) }
    .map_err(|e| format!("Failed to create image: {e}"))?;

    // Allocate memory for the image and bind it
    let memory_requirements = unsafe { device.get_image_memory_requirements(image) };

        let memory_type_index = find_memory_type(instance, physical_device, memory_requirements.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)
    .map_err(|e| {
        unsafe { device.destroy_image(image, None) };
        e
    })?;

    let alloc_info = vk::MemoryAllocateInfo::default()
    .allocation_size(memory_requirements.size)
    .memory_type_index(memory_type_index);

    let memory = unsafe { device.allocate_memory(&alloc_info, None) }
    .map_err(|e| {
        unsafe { device.destroy_image(image, None) };
        format!("Failed to allocate image memory: {e}")
    })?;

        // Define and create the image view
    let view_info = vk::ImageViewCreateInfo::default()
    .image(image)
    .view_type(vk::ImageViewType::TYPE_2D)
    .format(format)
    .subresource_range(vk::ImageSubresourceRange::default()
        .aspect_mask(aspect)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1));

    
    let view = unsafe {
        device.bind_image_memory(image, memory, 0)
        .and_then(|_| device.create_image_view(&view_info, None))
    }.map_err(|e| {
        unsafe {
            device.destroy_image(image, None);
            device.free_memory(memory, None);
        }
        format!("Failed to set up image: {e}")
    })?;

    Ok(ImageResources { image, memory, view })
}


// Find a memory type allowed by type_bits that has all of the requested properties
pub(super) fn find_memory_type(instance : &ash::Instance, physical_device : vk::PhysicalDevice, type_bits : u32, properties : vk::MemoryPropertyFlags) -> Result<u32, String> {
    let memory_properties = unsafe { instance.get_physical_device_memory_properties(physical_device) };

    (0..memory_properties.memory_type_count)
    .find(|&i| {
        type_bits & (1 << i) != 0 &&
        memory_properties.memory_types[i as usize].property_flags.contains(properties)
    })
    .ok_or_else(|| "Failed to find a suitable memory type".to_string())
}

// Find a supported depth format for the depth buffer
pub(super) fn find_depth_format(instance : &ash::Instance, physical_device : vk::PhysicalDevice) -> Result<vk::Format, String> {
    find_supported_format(
        instance,
        physical_device,
        &[vk::Format::D32_SFLOAT, vk::Format::D32_SFLOAT_S8_UINT, vk::Format::D24_UNORM_S8_UINT],
        vk::ImageTiling::OPTIMAL,
        vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT,
    )
}

// Find the first format in candidates that supports all of the requested features with the given tiling
fn find_supported_format(instance : &ash::Instance, physical_device : vk::PhysicalDevice,
    candidates : &[vk::Format],
    tiling : vk::ImageTiling,
    features : vk::FormatFeatureFlags) -> Result<vk::Format, String> {

    candidates.iter().copied()
    .find(|&candidate| {
        let props = unsafe { instance.get_physical_device_format_properties(physical_device, candidate) };
        let supported = match tiling {
            vk::ImageTiling::LINEAR => props.linear_tiling_features,
            vk::ImageTiling::OPTIMAL => props.optimal_tiling_features,
            _ => vk::FormatFeatureFlags::empty(),
        };
        supported.contains(features)
    })
    .ok_or_else(|| "Failed to find supported format".to_string())
}


// Image, its backing memory, and the view used to attach it when rendering
pub(super) struct ImageResources {
    pub(super) image : vk::Image,
    pub(super) memory : vk::DeviceMemory,
    pub(super) view : vk::ImageView,
}

// Buffer the rendered image is copied into, its CPU-visible memory, and the pointer to read it through
#[derive(Clone, Copy)]
pub(super) struct ReadbackBuffer {
    pub(super) buffer : vk::Buffer,
    pub(super) memory : vk::DeviceMemory,
    pub(super) mapped : *const u8,
}

#[derive(Clone, Copy)]
pub(super) struct Buffer {
    pub(super) buffer : vk::Buffer,
    pub(super) memory : vk::DeviceMemory,
    pub(super) mapped : *const u8
}
