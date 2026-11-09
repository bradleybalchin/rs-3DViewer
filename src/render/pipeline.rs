//! # Pipeline
//!
//! Shader modules and the creation of graphics pipelines

use ash::vk;

use crate::render::vertex::{GpuPoint};

// Create shader module from SPIR-V code
pub(super) fn create_shader_module(device : &ash::Device, code : &Vec<u32>) -> Result<vk::ShaderModule, String> {
    let shader_info = vk::ShaderModuleCreateInfo::default()
    .code(code);

    unsafe { device.create_shader_module(&shader_info, None) }
    .map_err(|e| format!("Failed to create shader module: {e}"))
}

// Configure shaders and fixed function stages of the graphics pipeline
pub(super) fn create_graphics_pipeline(
    device : &ash::Device, 
    descriptor_set_layout : vk::DescriptorSetLayout, 
    depth_format : vk::Format,
    color_format : vk::Format) -> Result<(vk::Pipeline, vk::PipelineLayout), String> {
    
    // Load shaders from SPIR-V files
    let vert_shader_code = ash::util::read_spv(&mut std::io::Cursor::new(&include_bytes!("shaders/shader.vert.spv")[..]))
    .map_err(|e| format!("Failed to read vertex shader SPIR-V: {e}"))?;
    let frag_shader_code = ash::util::read_spv(&mut std::io::Cursor::new(&include_bytes!("shaders/shader.frag.spv")[..]))
    .map_err(|e| format!("Failed to read fragment shader SPIR-V: {e}"))?;

    let vert_shader_module = create_shader_module(device, &vert_shader_code)?;
    let frag_shader_module = create_shader_module(device, &frag_shader_code)?;


    // Shader stage creation
    let vert_shader_stage_info = vk::PipelineShaderStageCreateInfo::default()
    .stage(vk::ShaderStageFlags::VERTEX)
    .module(vert_shader_module)
    .name(c"main");

    let frag_shader_stage_info = vk::PipelineShaderStageCreateInfo::default()
    .stage(vk::ShaderStageFlags::FRAGMENT)
    .module(frag_shader_module)
    .name(c"main");

    let shader_stages = [vert_shader_stage_info, frag_shader_stage_info];

    // Dynamic states (viewport and scissor) - allows changing viewport and scissor without recreating the pipeline
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state_info = vk::PipelineDynamicStateCreateInfo::default()
    .dynamic_states(&dynamic_states);

    // Vertex input state - describes the format of vertex data GPUPoint struct (position and color)
    let bindings = [GpuPoint::get_binding_description()];
    let attributes = GpuPoint::get_attribute_descriptions();
    let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::default()
    .vertex_binding_descriptions(&bindings)
    .vertex_attribute_descriptions(&attributes);

    // Input assembly state - describes how vertices are assembled into primitives (points, lines, triangles)
    let input_assembly_info = vk::PipelineInputAssemblyStateCreateInfo::default()
    .topology(vk::PrimitiveTopology::POINT_LIST)  // points
    .primitive_restart_enable(false);

    // Viewport and scissor state (dynamic)
    let viewport_state_info = vk::PipelineViewportStateCreateInfo::default()
    .viewport_count(1)
    .scissor_count(1);

    // Rasterization state - how primitives are rasterized (converted to fragments)
    let rasterizer_info = vk::PipelineRasterizationStateCreateInfo::default()
    .depth_clamp_enable(false)// clamp fragments beyond near and far planes instead of discarding 
    .rasterizer_discard_enable(false)// discard all primitives before rasterization (no fragments generated)
    .polygon_mode(vk::PolygonMode::FILL)// fill polygons (other options: LINE, POINT)
    .line_width(1.0)                    
    .cull_mode(vk::CullModeFlags::NONE)// no backface culling (for point clouds)
    .front_face(vk::FrontFace::COUNTER_CLOCKWISE)// front face is counter-clockwise (for point clouds)
    .depth_bias_enable(false)
    .depth_bias_constant_factor(0.0f32)
    .depth_bias_clamp(0.0f32)
    .depth_bias_slope_factor(0.0f32);

    // Multisampling - disabled here
    let multisampling_info = vk::PipelineMultisampleStateCreateInfo::default()
    .sample_shading_enable(false)
    .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    // Depth and Stencil Testing
    let depth_stencil_info = vk::PipelineDepthStencilStateCreateInfo::default()
    .depth_test_enable(true)
    .depth_write_enable(true)
    .depth_compare_op(vk::CompareOp::LESS)
    .depth_bounds_test_enable(false)
    .stencil_test_enable(false);

    // Color blending - disabled but still use write mask
    let blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
    .blend_enable(false)
    .color_write_mask(vk::ColorComponentFlags::RGBA)];

    let color_blend_info = vk::PipelineColorBlendStateCreateInfo::default()
    .attachments(&blend_attachments);

    // Pipeline layout - descriptor sets and push constants
    let binding = [descriptor_set_layout];
    let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
    .set_layouts(&binding)
    .push_constant_ranges(&[]);

    let pipeline_layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }
    .map_err(|e| {
    unsafe {    // prevent shaders from leaking on pipline creation failure
        device.destroy_shader_module(vert_shader_module, None);
        device.destroy_shader_module(frag_shader_module, None);
    }
    format!("Failed to create pipeline layout: {e}")
    })?;
        

    // Dynamic rendering - replaces traditional render pass and framebuffer
    let color_formats = [color_format]; // format of the color attachment
    let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
    .color_attachment_formats(&color_formats)
    .depth_attachment_format(depth_format); // format of the depth attachment

    // Graphics pipeline creation
    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
    .stages(&shader_stages)
    .vertex_input_state(&vertex_input_info)
    .input_assembly_state(&input_assembly_info)
    .viewport_state(&viewport_state_info)
    .rasterization_state(&rasterizer_info)
    .multisample_state(&multisampling_info)
    .depth_stencil_state(&depth_stencil_info)
    .color_blend_state(&color_blend_info)
    .dynamic_state(&dynamic_state_info)
    .layout(pipeline_layout)
    .push_next(&mut rendering_info);

    let result = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    };

    // Destroy Shader Modules
    unsafe {
        device.destroy_shader_module(vert_shader_module, None);
        device.destroy_shader_module(frag_shader_module, None);
    }

    match result {
        Ok(pipelines) => Ok((pipelines[0], pipeline_layout)),
        Err((_, e)) => {
            unsafe { device.destroy_pipeline_layout(pipeline_layout, None) };
            Err(format!("Failed to create graphics pipeline: {e}"))
        }
    }

}
