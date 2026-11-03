use bytemuck::{Pod, Zeroable};
use ash::{ext::physical_device_drm, vk::{self, DescriptorType, Format, FormatFeatureFlags, ImageTiling, Pipeline, PipelineLayout, QueueFlags}};


pub struct VulkanRenderer {
    _entry : ash::Entry,                              // Vulkan Functions Library
    instance : ash::Instance,                         // Vulkan Insance
    physical_device : vk::PhysicalDevice,             // physical graphics device (GPU)
    device  : ash::Device,                            // Vulkan Logical Device
    queues : Queues,                                  // Queues retrieved from the logical device
    descriptor_set_layout : vk::DescriptorSetLayout,  // descriptor set layout for describing resources to shaders (uniforms, textures, etc.)
    graphics_pipeline : Pipeline,                     // Graphics pipeline object (shaders and fixed functions stages)
    graphics_pipeline_layout : PipelineLayout,        // Graphics pipeline layout
    command_pool : vk::CommandPool,                   // Memory pool command buffers are allocated from
    command_buffers : Vec<vk::CommandBuffer>,         // allocated command buffers
    depth_format : Format,                            // format shared by the pipeline and depth image
    depth : Option<DepthResources>,                   // depth buffer (None until the render size is known)
}

impl VulkanRenderer {

    // TODO : enable validation layers for debugging
    // TODO : dispose of previous objects if error occurs during new()
    pub fn new() -> Result<Self, String>{
        const MAX_FRAMES_IN_FLIGHT : u32 = 2;   // number of buffered GPU frames

        let (entry, instance) = Self::create_instance()?;

        let physical_device = Self::select_physical_device(&instance)?;             // physical graphics device
        let indices = Self::find_queue_families(&instance, physical_device)?;   // indices of queue families being used
        let (device, queues) = Self::create_logical_device( &instance, &indices, physical_device)?;   // vulkan logical graphics device and its queues
        
        // descriptor set layout for describing resources to shaders (uniforms, textures, etc.)
        let descriptor_set_layout = Self::create_descriptor_set_layout(&device)?;

        // Depth format is chosen here because the pipeline and the depth image must use the same one
        let depth_format = Self::find_depth_format(&instance, physical_device)?;

        // Define the graphics pipline - shaders, descriptor set layouts, and fixed function stages
        let (graphics_pipeline,graphics_pipeline_layout) = Self::create_graphics_pipeline(&device, descriptor_set_layout, depth_format)?;

        // Create the command pool and allocate command buffers from it.
        let command_pool = Self::create_command_pool(&device, &indices)?;
        let command_buffers = Self::create_command_buffers(MAX_FRAMES_IN_FLIGHT, &device, command_pool)?;

        Ok(Self {
            _entry : entry,
            instance,
            physical_device,
            device,
            queues,
            descriptor_set_layout,
            graphics_pipeline,
            graphics_pipeline_layout,
            command_pool,
            command_buffers,
            depth_format,
            depth : None,           // none until render starts
        })
    }

    // Create the vulkan instance
    fn create_instance() -> Result<(ash::Entry, ash::Instance), String> {
        let entry = unsafe { ash::Entry::load() }
            .map_err(|e| format!("failed to load Vulkan: {e}"))?;

        let app_info = vk::ApplicationInfo::default()
            .application_name(c"3DViewer")
            .api_version(vk::API_VERSION_1_3);

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info);

        let instance = unsafe { entry.create_instance(&create_info, None) }
            .map_err(|e| format!("failed to create instance: {e}"))?;

        Ok((entry, instance))
    }

    // Select physical graphics device being targeted for rendering
    fn select_physical_device(instance : &ash::Instance) -> Result<vk::PhysicalDevice, String> {
       
            let physical_devices = unsafe { instance.enumerate_physical_devices()}
                .map_err(|e| format!("failed to enumurate devices : {e}"))?;

            println!("{:?} Devices Found", physical_devices.len());
            if physical_devices.is_empty() {
                return Err("Failed to Find GPU with Vulkan Support".into());
            }

            // Find discrete gpu if exists
            let discrete = physical_devices.iter().copied()
                .find(|&pd| {
                    let props = unsafe { instance.get_physical_device_properties(pd) };
                    props.device_type == vk::PhysicalDeviceType::DISCRETE_GPU &&
                    props.api_version >= vk::API_VERSION_1_3
                } );
            
            // if no discrete exists just return the first physical device
            Ok(discrete.unwrap_or(physical_devices[0]))   
    }

    // Create logical device - interface between physical graphics and application (queue family - group of GPU queues that share same capabilities)
    // Also retrieves the queues created with the device
    // Only does graphics queue for now
    fn create_logical_device(instance : &ash::Instance, indices : &QueueFamilyIndices, physical_device : vk::PhysicalDevice) -> Result<(ash::Device, Queues), String> {
    
        let device_features = unsafe { instance.get_physical_device_features(physical_device) };


        // Define queue creation info (for device)
        let queue_priorities = [1.0f32];    // also determines number of queues created

        let graphics_queue_create_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(indices.graphics_family)
        .queue_priorities(&queue_priorities);

        let queue_create_info = [graphics_queue_create_info];

        // Vulkan 1.3 features - dynamic rendering (allows rendering without a render pass or framebuffer)
        let mut features13 = vk::PhysicalDeviceVulkan13Features::default()
        .dynamic_rendering(true);

        // define and create the logical device
        let device_create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_create_info)
        .enabled_features(&device_features)
        .push_next(&mut features13);

        
        let device = unsafe { 
            instance.create_device(physical_device, &device_create_info, None) 
        }.map_err(|e| format!("Failed to create logical device {e}"))?;

        // Get Graphics Queues
        let graphics = (0..graphics_queue_create_info.queue_count)
            .map(|i| unsafe { device.get_device_queue(indices.graphics_family, i) })
            .collect();

        // TODO : Get Present Queues 
        let present = Vec::new();

        // TODO : Get Compute Queues 
        let compute = Vec::new();

        Ok((device, Queues { graphics, present, compute }))

    }

    // Find queue families for graphics and presentation
    fn find_queue_families(instance : &ash::Instance, physical_device : vk::PhysicalDevice) -> Result<QueueFamilyIndices, String> {
        
        let queue_families = unsafe{instance.get_physical_device_queue_family_properties(physical_device)};
        if queue_families.is_empty() { return Err("Failed to Find Queue Families".into()); }
        
    
        let i_graphics = 
        queue_families.iter().position(|f| f.queue_flags.contains(QueueFlags::GRAPHICS) && f.queue_count > 0)
        .ok_or("No queue families with graphics support detected")?;
        
        let i_compute = queue_families.iter().position(|f| f.queue_flags.contains(QueueFlags::COMPUTE) && f.queue_count > 0)
        .ok_or("No queue families with compute support detected")?;

        Ok(QueueFamilyIndices { graphics_family: i_graphics as u32, present_family: 0 as u32, compute_family: i_compute as u32 })
    }
    
    // Create descriptor set layout for describing resources to shaders (uniforms, textures, etc.)
    fn create_descriptor_set_layout(device : &ash::Device) -> Result<vk::DescriptorSetLayout, String> {

        let ubo_binding = vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(DescriptorType::UNIFORM_BUFFER)
        .descriptor_count(1)            
        .stage_flags(vk::ShaderStageFlags::VERTEX);         // vertex shader

        let bindings = [ubo_binding];

        // Create descriptor set layout
        let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
        .bindings(&bindings);

        unsafe { device.create_descriptor_set_layout(&layout_info, None) }
        .map_err(|e| format!("Failed to create descriptor set layout: {e}"))
    }

    // Create shader module from SPIR-V code
    fn create_shader_module(device : &ash::Device, code : &Vec<u32>) -> Result<vk::ShaderModule, String> {
        let shader_info = vk::ShaderModuleCreateInfo::default()
        .code(code);

        unsafe { device.create_shader_module(&shader_info, None) }
        .map_err(|e| format!("Failed to create shader module: {e}"))
    }

    // Configure shaders and fixed function stages of the graphics pipeline
    fn create_graphics_pipeline(device : &ash::Device, descriptor_set_layout : vk::DescriptorSetLayout, depth_format : Format) -> Result<(Pipeline, PipelineLayout), String> {
        
        // Load shaders from SPIR-V files
        let vert_shader_code = ash::util::read_spv(&mut std::io::Cursor::new(&include_bytes!("shaders/shader.vert.spv")[..]))
        .map_err(|e| format!("Failed to read vertex shader SPIR-V: {e}"))?;
        let frag_shader_code = ash::util::read_spv(&mut std::io::Cursor::new(&include_bytes!("shaders/shader.frag.spv")[..]))
        .map_err(|e| format!("Failed to read fragment shader SPIR-V: {e}"))?;

        let vert_shader_module = Self::create_shader_module(device, &vert_shader_code)?;
        let frag_shader_module = Self::create_shader_module(device, &frag_shader_code)?;


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
        let color_formats = [vk::Format::R8G8B8A8_UNORM]; // format of the color attachment
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

    // memory pool for command buffers to be allocated from
    fn create_command_pool(device : &ash::Device, indices : &QueueFamilyIndices) -> Result<vk::CommandPool, String> {
        let pool_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)    // command buffers can be rerecorded individually
            .queue_family_index(indices.graphics_family);

        unsafe {
            device.create_command_pool(&pool_info, None)
            .map_err(|e| format!("Failed to create command pool {e}"))   
        }

    }

    // Records commands which are submitted to device execution queues
    // Create a command buffer per frame in flight
    fn create_command_buffers(max_frames_in_flight:u32, device : &ash::Device, command_pool : vk::CommandPool) -> Result<Vec<vk::CommandBuffer>, String>{

        let alloc_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(max_frames_in_flight);

        unsafe {
            device.allocate_command_buffers(&alloc_info)
            .map_err(|e| format!("Failed to allocate command buffer {e}"))
        }

    }

    // Resources and memory allocation for the depth buffer
    // The image is tied to the render size, so it needs recreating when the viewport is resized
    fn create_depth_resources(instance : &ash::Instance, device : &ash::Device, physical_device : vk::PhysicalDevice, format : Format, width : u32, height : u32) -> Result<DepthResources, String> {

        // Define and create the image
        let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D { width, height, depth: 1 })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)      // must match the pipeline's rasterization samples
        .tiling(ImageTiling::OPTIMAL)               // must match the tiling used in find_depth_format
        .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)   // only used by the graphics queue
        .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe { device.create_image(&image_info, None) }
        .map_err(|e| format!("Failed to create depth image: {e}"))?;

        // Allocate memory for the image and bind it
        let memory_requirements = unsafe { device.get_image_memory_requirements(image) };

        let memory_type_index = Self::find_memory_type(instance, physical_device, memory_requirements.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)
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
            format!("Failed to allocate depth image memory: {e}")
        })?;

        // Define and create the image view
        let view_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .subresource_range(vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::DEPTH)
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
            format!("Failed to set up depth image: {e}")
        })?;

        Ok(DepthResources { image, memory, view })
    }

    // Find a memory type allowed by type_bits that has all of the requested properties
    fn find_memory_type(instance : &ash::Instance, physical_device : vk::PhysicalDevice, type_bits : u32, properties : vk::MemoryPropertyFlags) -> Result<u32, String> {
        let memory_properties = unsafe { instance.get_physical_device_memory_properties(physical_device) };

        (0..memory_properties.memory_type_count)
        .find(|&i| {
            type_bits & (1 << i) != 0 &&
            memory_properties.memory_types[i as usize].property_flags.contains(properties)
        })
        .ok_or_else(|| "Failed to find a suitable memory type".to_string())
    }

    // Find a supported depth format for the depth buffer
    fn find_depth_format(instance : &ash::Instance, physical_device : vk::PhysicalDevice) -> Result<Format, String>{
        Self::find_supported_format(
            instance,
            physical_device,
            vec![Format::D32_SFLOAT, Format::D32_SFLOAT_S8_UINT, Format::D24_UNORM_S8_UINT],
            ImageTiling::OPTIMAL,
            FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT
        )
    }

    // find supported format from a list of candidates that matches a correct list of propertiy flags
    fn find_supported_format(instance : &ash::Instance, physical_device : vk::PhysicalDevice, candidates : Vec<Format>, tiling : ImageTiling, features : FormatFeatureFlags)
    -> Result<Format, String> {
        for candidate in candidates {
            let props = unsafe { instance.get_physical_device_format_properties(physical_device, candidate) };
            
            if tiling == ImageTiling::LINEAR && (props.linear_tiling_features & features) == features {
                return Ok(candidate)
            } else if tiling == ImageTiling::OPTIMAL && (props.optimal_tiling_features & features) == features  {
                return Ok(candidate)
            }
        }

        Err("Failed to find supported format".into())
    }

    pub fn render(&mut self, width: u32, height: u32, clear_color: [f32; 4]) -> Result<&[u8], vk::Result>{
        todo!()
    }
}

// Destroy vulkan objects, children before parents (device before instance)
// _entry is dropped after this runs, keeping the Vulkan library loaded for these calls
impl Drop for VulkanRenderer {
    fn drop(&mut self) {
        unsafe {
            // Wait for the GPU to finish any submitted work
            if let Err(e) = self.device.device_wait_idle() {
                eprintln!("failed to wait for device idle: {e}");
            }

            if let Some(depth) = self.depth.take() {
                self.device.destroy_image_view(depth.view, None);
                self.device.destroy_image(depth.image, None);
                self.device.free_memory(depth.memory, None);
            }

            self.device.destroy_command_pool(self.command_pool, None);

            self.device.destroy_pipeline(self.graphics_pipeline, None);
            self.device.destroy_pipeline_layout(self.graphics_pipeline_layout, None);

            self.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);

            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

// Indicies of used Queue Families
struct QueueFamilyIndices {
    graphics_family : u32,
    present_family : u32,
    compute_family :u32,
}

// Queues from the logical device, grouped by role
struct Queues {
    graphics : Vec<vk::Queue>,
    present  : Vec<vk::Queue>,
    compute  : Vec<vk::Queue>,
}

// Depth buffer image, its backing memory, and the view used to attach it when rendering
struct DepthResources {
    image : vk::Image,
    memory : vk::DeviceMemory,
    view : vk::ImageView,
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