//! # Command
//! 
//! Handles the creation of command pools, allocation of command buffers, one-off submits and fences

use ash::vk;
use crate::render::context::QueueFamilyIndices;

/// create memory pool for command buffers to be allocated from
pub(super) fn create_command_pool(device : &ash::Device, indices : &QueueFamilyIndices) -> Result<vk::CommandPool, String> {
    let pool_info = vk::CommandPoolCreateInfo::default()
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)    // command buffers can be rerecorded individually
        .queue_family_index(indices.graphics_family);

    unsafe {
        device.create_command_pool(&pool_info, None)
        .map_err(|e| format!("Failed to create command pool {e}"))   
    }

}

// Records commands which are submitted to device execution queues
/// Create a command buffer per frame in flight
pub(super) fn create_command_buffers(max_frames_in_flight:u32, device : &ash::Device, command_pool : vk::CommandPool) -> Result<Vec<vk::CommandBuffer>, String>{

    let alloc_info = vk::CommandBufferAllocateInfo::default()
    .command_pool(command_pool)
    .level(vk::CommandBufferLevel::PRIMARY)
    .command_buffer_count(max_frames_in_flight);

    unsafe {
        device.allocate_command_buffers(&alloc_info)
        .map_err(|e| format!("Failed to allocate command buffer {e}"))
    }

}


/// Record and submit a one-off command buffer, waiting for it to finish.
/// Used for uploads (staging -> vertex buffer copies), not per-frame rendering.
pub(super) fn submit_one_time(
    device : &ash::Device,
    command_pool : vk::CommandPool,
    queue : vk::Queue,
    record : impl FnOnce(vk::CommandBuffer),
) -> Result<(), String> {
    let alloc_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    let cmd = unsafe { device.allocate_command_buffers(&alloc_info) }
        .map_err(|e| format!("Failed to allocate command buffer: {e}"))?[0];

    let result = (|| unsafe {
        let begin = vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        device.begin_command_buffer(cmd, &begin).map_err(|e| format!("Failed to begin command buffer: {e}"))?;
        // do requested command buffer here (record)
        record(cmd);
        device.end_command_buffer(cmd).map_err(|e| format!("Failed to end command buffer: {e}"))?;

        let cmd_infos = [vk::CommandBufferSubmitInfo::default().command_buffer(cmd)];
        let submit = vk::SubmitInfo2::default().command_buffer_infos(&cmd_infos);
        device.queue_submit2(queue, &[submit], vk::Fence::null()).map_err(|e| format!("Failed to submit: {e}"))?;
        device.queue_wait_idle(queue).map_err(|e| format!("Failed to wait for queue: {e}"))
    })();

    unsafe { device.free_command_buffers(command_pool, &[cmd]) };   // freed even if a step above failed
    result
}

/// Create the fence that lets the CPU wait for submitted GPU work to finish (created unsignalled)
pub(super) fn create_fence(device : &ash::Device) -> Result<vk::Fence, String> {
    unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None) }
    .map_err(|e| format!("Failed to create fence: {e}"))
}
