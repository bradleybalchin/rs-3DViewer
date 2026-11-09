//! # Vulkan
//!
//! Offscreen Vulkan renderer for the point cloud viewer.
//!
//! egui owns the window, so there is no swapchain. Each frame is drawn into a colour image,
//! copied into a CPU-visible readback buffer, and returned as RGBA8 bytes for the viewport
//! to show as an egui texture:
//!
//! ```text
//! colour image (GPU) -> readback buffer -> &[u8] -> egui texture
//! ```
//!
//! Uses Vulkan 1.3 with dynamic rendering (no render passes or framebuffers) and
//! synchronization2 for barriers. Rendering is synchronous: `render()` waits for the GPU to
//! finish before returning, so only one frame is ever in flight.

use ash::vk::{self, Extent3D, Format, Pipeline, PipelineLayout};
use glam::{Mat4, Vec3, camera};
use std::time::{Instant};

use crate::render::context::{self, DebugMessenger, Queues};
use crate::render::pipeline::{self};
use crate::render::descriptors::{self};
use crate::render::resources::{self, ImageResources, ReadbackBuffer, Buffer};
use crate::render::vertex::{GpuPoint, UniformBufferObject};
use crate::render::command::{self};
use crate::render::timings::RenderTimings;

/// ## Vulkan Renderer
///
/// Owns every Vulkan object used to render the 3D scene, and destroys them all when dropped.
///
/// Objects that don't depend on the render size are created in `new()`. The size-dependent
/// render targets (colour, depth and readback) are created on the first `render()` call,
/// and recreated whenever the requested size changes.
pub struct VulkanRenderer {
    _entry : ash::Entry,                              // loaded Vulkan library, must outlive everything else
    instance : ash::Instance,                         // connection between the application and Vulkan
    debug_messenger : Option<DebugMessenger>,         // prints validation messages (None if validation is disabled)
    physical_device : vk::PhysicalDevice,             // GPU being rendered with
    device  : ash::Device,                            // logical device, the interface to the GPU that creates most objects
    queues : Queues,                                  // queues that command buffers are submitted to
    descriptor_set_layout : vk::DescriptorSetLayout,  // describes the resources shaders can access (uniform buffer)
    graphics_pipeline : Pipeline,                     // shaders and fixed function state used to draw points
    graphics_pipeline_layout : PipelineLayout,        // descriptor set layouts and push constants the pipeline uses
    command_pool : vk::CommandPool,                   // memory pool that command buffers are allocated from
    command_buffers : Vec<vk::CommandBuffer>,         // command buffers that frames are recorded into
    depth_format : Format,                            // depth format shared by the pipeline and depth image
    color_format : Format,                            // colour format shared by the pipeline and colour image
    depth : Option<ImageResources>,                   // depth image for depth testing (None until the render size is known)
    color : Option<ImageResources>,                   // colour image the frame is drawn into (None until the render size is known)
    readback : Option<ReadbackBuffer>,                // CPU-visible buffer the colour image is copied into (None until the render size is known)
    render_size : (u32,u32),                          // (width, height) of the current render targets, (0, 0) if they don't exist
    fence : vk::Fence,                                // signalled when the GPU finishes a submitted frame

    vertex_buffer : Option<Buffer>,
    point_count : u32,                                // number of points in the vertex buffer
    scene_radius : f32,                               // distance of the furthest point from the origin, used to frame the camera
    uniform_buffers : Vec<Buffer>,
    descriptor_pool : vk::DescriptorPool,
    descriptor_sets : Vec<vk::DescriptorSet>,

    timings : RenderTimings,                          // time spent in each part of render()
}

impl VulkanRenderer {

    // TODO : dispose of previous objects if error occurs during new()
    // TODO : better error handling to remove need for map_err on every ash error result
    pub fn new(debug : bool) -> Result<Self, String>{
        const MAX_FRAMES_IN_FLIGHT : u32 = 1;   // number of buffered GPU frames

        let (entry, instance) = context::create_instance(debug)?;

        // Prints validation messages, only exists when debug is enabled
        let debug_messenger = if debug { Some(context::create_debug_messenger(&entry, &instance)?) } else { None };

        let physical_device = context::select_physical_device(&instance)?;             // physical graphics device
        let indices = context::find_queue_families(&instance, physical_device)?;   // indices of queue families being used
        let (device, queues) = context::create_logical_device( &instance, &indices, physical_device)?;   // vulkan logical graphics device and its queues
        
        // descriptor set layout for describing resources to shaders (uniforms, textures, etc.)
        let descriptor_set_layout = descriptors::create_descriptor_set_layout(&device)?;

        // Depth format is chosen here because the pipeline and the depth image must use the same one
        let depth_format = resources::find_depth_format(&instance, physical_device)?;
        let color_format = Format:: R8G8B8A8_UNORM;

        // Define the graphics pipline - shaders, descriptor set layouts, and fixed function stages
        let (graphics_pipeline,graphics_pipeline_layout) = pipeline::create_graphics_pipeline(&device, descriptor_set_layout, depth_format, color_format)?;

        // Create the command pool and allocate command buffers from it.
        let command_pool = command::create_command_pool(&device, &indices)?;
        let command_buffers = command::create_command_buffers(MAX_FRAMES_IN_FLIGHT, &device, command_pool)?;

        // fence to synchronise GPU and CPU
        let fence = command::create_fence(&device)?;

        // Vertex Buffer - create when vertices are loaded in

        // Uniform Buffer
        let uniform_buffers = resources::create_uniform_buffers(&instance, &device, physical_device, MAX_FRAMES_IN_FLIGHT)?;

        // Descriptor Sets and Pools
        let descriptor_pool = descriptors::create_descriptor_pool(&device, MAX_FRAMES_IN_FLIGHT)?;
        let descriptor_sets = descriptors::create_descriptor_sets(
            &device, 
            MAX_FRAMES_IN_FLIGHT, 
            descriptor_set_layout, 
            descriptor_pool, 
            &uniform_buffers)?;

        

        // create depth, colour, and readback buffer on render since size of window is not known
        Ok(Self {
            _entry : entry,
            instance,
            debug_messenger,
            physical_device,
            device,
            queues,
            descriptor_set_layout,
            graphics_pipeline,
            graphics_pipeline_layout,
            command_pool,
            command_buffers,
            depth_format,
            color_format,
            depth : None,           // none until render starts
            color : None,      // none until render starts
            readback : None,
            render_size : (0,0),
            fence,

            vertex_buffer : None,
            point_count : 0,
            scene_radius : 0.0,
            uniform_buffers,
            descriptor_pool,
            descriptor_sets,

            timings : RenderTimings::default(),
        })
    }

    
    /// Upload vertex data to create a new vertex buffer object from which vertices are rendered from
    pub fn upload_vertices(&mut self, points: &[GpuPoint])
        -> Result<(), String>{
        unsafe {
            self.device.device_wait_idle().map_err(|e| format!("failed to wait for device idle: {e}"))?;
            // if a previous vertex buffer exists, destory it
            if let Some(buffer) = self.vertex_buffer.take() {
                self.device.destroy_buffer(buffer.buffer, None);
                self.device.free_memory(buffer.memory, None);
            }

        }
        
        self.vertex_buffer = resources::create_vertex_buffer(
            &self.instance, 
            &self.device, 
            self.physical_device, 
            self.command_pool, 
            self.queues.graphics[0], 
            points)?;

        self.point_count = points.len() as u32;
        self.scene_radius = points.iter().map(|p| Vec3::from(p.pos).length()).fold(0.0, f32::max);

        println!("[Renderer] {:?} Vertices Loaded", points.len());
        Ok(())

    }

    // Write the camera matrices for this frame into the uniform buffer
    // The memory is host coherent and written before the queue submit, so no barrier is needed
    // Fixed camera for now, placed so the whole point cloud is in view
    fn update_uniform_buffer(&self, width : u32, height : u32) {
        let radius = self.scene_radius.max(1.0);
        let eye = Vec3::new(radius, -radius, radius) * 1.5;
        let view = camera::rh::view::look_at_mat4(eye, Vec3::ZERO, Vec3::Z);     // LAS is Z up
        let proj = camera::rh::proj::vulkan::perspective(45f32.to_radians(), width as f32 / height as f32, radius * 0.01, radius * 10.0);   // depth 0..1 and Y down

        let ubo = UniformBufferObject { model : Mat4::IDENTITY, view, proj };
        let bytes = bytemuck::bytes_of(&ubo);
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.uniform_buffers[0].mapped, bytes.len()) };
    }

    pub fn render(&mut self, width: u32, height: u32, clear_color: [f32; 4]) -> Result<&[u8], String> {
        let start = Instant::now();

        // if size changed, update the render targets
        if self.render_size != (width, height) {
            self.recreate_render_targets(width, height)?;
        }
        let after_resize = Instant::now();

        self.update_uniform_buffer(width, height);
        self.record_command_buffer(width, height, clear_color)?;
        let after_record = Instant::now();

        // Submit the commands and wait for the GPU to finish, so the pixels are ready to read
        let command_buffers = [self.command_buffers[0]];
        let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);

        let (after_submit, after_wait) = unsafe {
            self.device.queue_submit(self.queues.graphics[0], &[submit_info], self.fence)
            .map_err(|e| format!("Failed to submit command buffer: {e}"))?;
            let after_submit = Instant::now();

            self.device.wait_for_fences(&[self.fence], true, u64::MAX)
            .map_err(|e| format!("Failed to wait for fence: {e}"))?;
            let after_wait = Instant::now();

            self.device.reset_fences(&[self.fence])
            .map_err(|e| format!("Failed to reset fence: {e}"))?;

            (after_submit, after_wait)
        };

        self.timings.resize += after_resize - start;
        self.timings.record += after_record - after_resize;
        self.timings.submit += after_submit - after_record;
        self.timings.wait += after_wait - after_submit;
        self.timings.total += start.elapsed();
        self.timings.frames += 1;
        self.timings.report_if_due();

        let readback = self.readback.as_ref().ok_or("Readback buffer missing")?;
        let byte_count = width as usize * height as usize * 4;

        Ok(unsafe { std::slice::from_raw_parts(readback.mapped, byte_count) })
    }

    // Record the commands for one frame: clear the colour and depth images, draw the points, then copy the colour image to the readback buffer
    fn record_command_buffer(&self, width : u32, height : u32, clear_color : [f32; 4]) -> Result<(), String> {
        let color = self.color.as_ref().ok_or("Color image missing")?;
        let depth = self.depth.as_ref().ok_or("Depth image missing")?;
        let readback = self.readback.as_ref().ok_or("Readback buffer missing")?;
        let cmd = self.command_buffers[0];

        let color_range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);

        // Barriers on a combined depth stencil format must include both aspects
        let depth_aspect = if self.depth_format == Format::D32_SFLOAT {
            vk::ImageAspectFlags::DEPTH
        } else {
            vk::ImageAspectFlags::DEPTH | vk::ImageAspectFlags::STENCIL
        };
        let depth_range = vk::ImageSubresourceRange::default()
            .aspect_mask(depth_aspect)
            .level_count(1)
            .layer_count(1);

        // Prepare the images for drawing. Their old contents aren't needed, so the old layout can be UNDEFINED
        let attachment_barriers = [
            vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::NONE)
                .src_access_mask(vk::AccessFlags2::NONE)
                .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
                .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(color.image)
                .subresource_range(color_range),
            vk::ImageMemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::NONE)
                .src_access_mask(vk::AccessFlags2::NONE)
                .dst_stage_mask(vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS)
                .dst_access_mask(vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(depth.image)
                .subresource_range(depth_range),
        ];
        let attachment_dependency = vk::DependencyInfo::default().image_memory_barriers(&attachment_barriers);

        // Clear both images when rendering starts. Colour is kept for the copy, depth isn't needed after the frame
        let color_attachments = [vk::RenderingAttachmentInfo::default()
            .image_view(color.view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue { color : vk::ClearColorValue { float32 : clear_color } })];

        let depth_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(depth.view)
            .image_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .clear_value(vk::ClearValue { depth_stencil : vk::ClearDepthStencilValue { depth : 1.0, stencil : 0 } });

        let rendering_info = vk::RenderingInfo::default()
            .render_area(vk::Rect2D { offset : vk::Offset2D::default(), extent : vk::Extent2D { width, height } })
            .layer_count(1)
            .color_attachments(&color_attachments)
            .depth_attachment(&depth_attachment);

        // Wait for rendering to finish, then prepare the colour image to be copied from
        let copy_barriers = [vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::COPY)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(color.image)
            .subresource_range(color_range)];
        let copy_dependency = vk::DependencyInfo::default().image_memory_barriers(&copy_barriers);

        // Row length and image height are left at 0, meaning the pixels are tightly packed in the buffer
        let copy_region = vk::BufferImageCopy::default()
            .image_subresource(vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1))
            .image_extent(Extent3D { width, height, depth : 1 });

        // Make the copied pixels visible to the CPU
        let host_barriers = [vk::BufferMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COPY)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::HOST)
            .dst_access_mask(vk::AccessFlags2::HOST_READ)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .buffer(readback.buffer)
            .size(vk::WHOLE_SIZE)];
        let host_dependency = vk::DependencyInfo::default().buffer_memory_barriers(&host_barriers);

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe {
            // Beginning also resets the previous frame's recording, as the pool has RESET_COMMAND_BUFFER
            self.device.begin_command_buffer(cmd, &begin_info)
            .map_err(|e| format!("Failed to begin command buffer: {e}"))?;

            self.device.cmd_pipeline_barrier2(cmd, &attachment_dependency);

            self.device.cmd_begin_rendering(cmd, &rendering_info);

            // Nothing to draw until a file has been loaded, the frame is just the clear colour
            if let Some(vertex_buffer) = &self.vertex_buffer {
                // Positive height, as the projection matrix already flips Y
                let viewport = vk::Viewport { x : 0.0, y : 0.0, width : width as f32, height : height as f32, min_depth : 0.0, max_depth : 1.0 };
                let scissor = vk::Rect2D { offset : vk::Offset2D::default(), extent : vk::Extent2D { width, height } };

                self.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.graphics_pipeline);
                self.device.cmd_set_viewport(cmd, 0, &[viewport]);
                self.device.cmd_set_scissor(cmd, 0, &[scissor]);
                self.device.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::GRAPHICS, self.graphics_pipeline_layout, 0, &[self.descriptor_sets[0]], &[]);
                self.device.cmd_bind_vertex_buffers(cmd, 0, &[vertex_buffer.buffer], &[0]);
                self.device.cmd_draw(cmd, self.point_count, 1, 0, 0);
            }

            self.device.cmd_end_rendering(cmd);

            self.device.cmd_pipeline_barrier2(cmd, &copy_dependency);
            self.device.cmd_copy_image_to_buffer(cmd, color.image, vk::ImageLayout::TRANSFER_SRC_OPTIMAL, readback.buffer, &[copy_region]);
            self.device.cmd_pipeline_barrier2(cmd, &host_dependency);

            self.device.end_command_buffer(cmd)
            .map_err(|e| format!("Failed to end command buffer: {e}"))
        }
    }

    // update render targets (readbackbuffer, depth, color) for new width / height
    fn recreate_render_targets(&mut self, width : u32, height : u32) -> Result<(), String> {

        // The old targets can't be destroyed while the GPU is still using them
        unsafe { self.device.device_wait_idle() }
        .map_err(|e| format!("Failed to wait for device idle: {e}"))?;

        self.destroy_render_targets();

        // Marks the targets as missing, so if a create below fails the next frame retries instead of using them
        self.render_size = (0, 0);

        self.color = Some(resources::create_image_resources(
            &self.instance,
            &self.device,
            self.physical_device,
            self.color_format,
            width,
            height,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
            vk::ImageAspectFlags::COLOR)?);

        self.depth = Some(resources::create_image_resources(
            &self.instance,
            &self.device,
            self.physical_device,
            self.depth_format,
            width,
            height,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            vk::ImageAspectFlags::DEPTH)?);

        // 4 bytes per pixel for RGBA8
        let readback_size = width as vk::DeviceSize * height as vk::DeviceSize * 4;
        self.readback = Some(resources::create_readback_buffer(
            &self.instance,
            &self.device,
            self.physical_device,
            readback_size)?);

        self.render_size = (width, height);

        Ok(())
    }

    // Destroy the render targets that exist. The GPU must not be using them when this is called
    fn destroy_render_targets(&mut self) {
        unsafe {
            if let Some(readback) = self.readback.take() {
                self.device.unmap_memory(readback.memory);
                self.device.destroy_buffer(readback.buffer, None);
                self.device.free_memory(readback.memory, None);
            }

            if let Some(color) = self.color.take() {
                self.device.destroy_image_view(color.view, None);
                self.device.destroy_image(color.image, None);
                self.device.free_memory(color.memory, None);
            }

            if let Some(depth) = self.depth.take() {
                self.device.destroy_image_view(depth.view, None);
                self.device.destroy_image(depth.image, None);
                self.device.free_memory(depth.memory, None);
            }
        }
    }

}

// Destroy vulkan objects, children before parents (e.g., device before instance)
// _entry is dropped after this runs, keeping the Vulkan library loaded for these calls
impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        unsafe {
            // Wait for the GPU to finish any submitted work
            if let Err(e) = self.device.device_wait_idle() {
                eprintln!("failed to wait for device idle: {e}");
            }
        
        
            self.device.destroy_descriptor_pool(self.descriptor_pool, None);

            for uniform_buffer in &self.uniform_buffers {
                self.device.unmap_memory(uniform_buffer.memory);
                self.device.destroy_buffer(uniform_buffer.buffer, None);
                self.device.free_memory(uniform_buffer.memory, None);
            }
            
            if let Some(vertex) = self.vertex_buffer.take() {
                self.device.destroy_buffer(vertex.buffer, None);
                self.device.free_memory(vertex.memory, None);
            }

            self.device.destroy_fence(self.fence, None);

            self.destroy_render_targets();

            self.device.destroy_command_pool(self.command_pool, None);

            self.device.destroy_pipeline(self.graphics_pipeline, None);
            self.device.destroy_pipeline_layout(self.graphics_pipeline_layout, None);

            self.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);

            self.device.destroy_device(None);

            if let Some(debug) = self.debug_messenger.take() {
                debug.loader.destroy_debug_utils_messenger(debug.messenger, None);
            }

            self.instance.destroy_instance(None);
        }
    }
}