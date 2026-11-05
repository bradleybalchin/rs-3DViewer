use bytemuck::{Pod, Zeroable};
use ash::vk::{self, DescriptorType, Extent3D, Format, FormatFeatureFlags, ImageTiling, Pipeline, PipelineLayout, QueueFlags};
use eframe::wgpu::wgc::resource::CreateExternalTextureError::CreateBuffer;
use std::ffi::{CStr, c_void};
use std::time::{Duration, Instant};

const VALIDATION_LAYER : &CStr = c"VK_LAYER_KHRONOS_validation";

pub struct VulkanRenderer {
    _entry : ash::Entry,                              // Vulkan Functions Library
    instance : ash::Instance,                         // Vulkan Insance
    debug_messenger : Option<DebugMessenger>,         // prints validation messages (None if validation is disabled)
    physical_device : vk::PhysicalDevice,             // physical graphics device (GPU)
    device  : ash::Device,                            // Vulkan Logical Device
    queues : Queues,                                  // Queues retrieved from the logical device
    descriptor_set_layout : vk::DescriptorSetLayout,  // descriptor set layout for describing resources to shaders (uniforms, textures, etc.)
    graphics_pipeline : Pipeline,                     // Graphics pipeline object (shaders and fixed functions stages)
    graphics_pipeline_layout : PipelineLayout,        // Graphics pipeline layout
    command_pool : vk::CommandPool,                   // Memory pool command buffers are allocated from
    command_buffers : Vec<vk::CommandBuffer>,         // allocated command buffers
    depth_format : Format,                            // format shared by the pipeline and depth image
    color_format : Format,                            // format shared by pipeline and colour image      
    depth : Option<ImageResources>,                   // depth buffer (None until the render size is known)
    color : Option<ImageResources>,                   // color buffer (Rendered image)
    readback : Option<ReadbackBuffer>,                // Readback buffer for sending color image to CPU
    render_size : (u32,u32),                          // size of render (width, height)
    fence : vk::Fence,
    timings : RenderTimings,                          // time spent in each part of render()
}

impl VulkanRenderer {

    // TODO : dispose of previous objects if error occurs during new()
    pub fn new(debug : bool) -> Result<Self, String>{
        const MAX_FRAMES_IN_FLIGHT : u32 = 1;   // number of buffered GPU frames

        let (entry, instance) = Self::create_instance(debug)?;

        // Prints validation messages, only exists when debug is enabled
        let debug_messenger = if debug { Some(Self::create_debug_messenger(&entry, &instance)?) } else { None };

        let physical_device = Self::select_physical_device(&instance)?;             // physical graphics device
        let indices = Self::find_queue_families(&instance, physical_device)?;   // indices of queue families being used
        let (device, queues) = Self::create_logical_device( &instance, &indices, physical_device)?;   // vulkan logical graphics device and its queues
        
        // descriptor set layout for describing resources to shaders (uniforms, textures, etc.)
        let descriptor_set_layout = Self::create_descriptor_set_layout(&device)?;

        // Depth format is chosen here because the pipeline and the depth image must use the same one
        let depth_format = Self::find_depth_format(&instance, physical_device)?;
        let color_format = Format:: R8G8B8A8_UNORM;

        // Define the graphics pipline - shaders, descriptor set layouts, and fixed function stages
        let (graphics_pipeline,graphics_pipeline_layout) = Self::create_graphics_pipeline(&device, descriptor_set_layout, depth_format, color_format)?;

        // Create the command pool and allocate command buffers from it.
        let command_pool = Self::create_command_pool(&device, &indices)?;
        let command_buffers = Self::create_command_buffers(MAX_FRAMES_IN_FLIGHT, &device, command_pool)?;

        // fence to synchronise GPU and CPU
        let fence = Self::create_fence(&device)?;

        // Vertex Buffer

        // Uniform Buffer
        
        // Descriptor Sets and Pools

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
            timings : RenderTimings::default(),
        })
    }

    // Check the validation layer is installed
    fn validation_layer_available(entry : &ash::Entry) -> Result<bool, String> {
        let layers = unsafe { entry.enumerate_instance_layer_properties() }
            .map_err(|e| format!("Failed to enumerate instance layers: {e}"))?;

        Ok(layers.iter().any(|layer| layer.layer_name_as_c_str() == Ok(VALIDATION_LAYER)))
    }

    // Which validation messages to receive and the function that handles them
    fn debug_messenger_info<'a>() -> vk::DebugUtilsMessengerCreateInfoEXT<'a> {
        vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR)
            .message_type(vk::DebugUtilsMessageTypeFlagsEXT::GENERAL | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE)
            .pfn_user_callback(Some(debug_callback))
    }

    // Create the messenger that sends validation messages to debug_callback
    fn create_debug_messenger(entry : &ash::Entry, instance : &ash::Instance) -> Result<DebugMessenger, String> {
        let loader = ash::ext::debug_utils::Instance::new(entry, instance);

        let messenger = unsafe { loader.create_debug_utils_messenger(&Self::debug_messenger_info(), None) }
            .map_err(|e| format!("Failed to create debug messenger: {e}"))?;

        Ok(DebugMessenger { loader, messenger })
    }

    // Create the vulkan instance
    fn create_instance(validation : bool) -> Result<(ash::Entry, ash::Instance), String> {
        let entry = unsafe { ash::Entry::load() }
            .map_err(|e| format!("failed to load Vulkan: {e}"))?;

        let app_info = vk::ApplicationInfo::default()
            .application_name(c"3DViewer")
            .api_version(vk::API_VERSION_1_3);

        // These must outlive create_instance, as create_info only stores pointers to them
        let layer_names = [VALIDATION_LAYER.as_ptr()];
        let extension_names = [ash::ext::debug_utils::NAME.as_ptr()];
        let mut debug_info = Self::debug_messenger_info();

        let mut create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info);

        if validation {
            if !Self::validation_layer_available(&entry)? {
                return Err("Validation layer requested but not available".into());
            }

            // Builder methods return the updated struct, so the result has to be assigned back
            create_info = create_info
                .enabled_layer_names(&layer_names)
                .enabled_extension_names(&extension_names)
                .push_next(&mut debug_info);    // also reports problems during instance creation and destruction
        }

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
        .dynamic_rendering(true)
        .synchronization2(true);

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
    fn create_graphics_pipeline(
        device : &ash::Device, 
        descriptor_set_layout : vk::DescriptorSetLayout, 
        depth_format : Format,
        color_format : Format) -> Result<(Pipeline, PipelineLayout), String> {
        
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


    // Create a 2D render target: the image, its device-local memory and a view of the whole image
    // usage  - how the image will be used (e.g. COLOR_ATTACHMENT, DEPTH_STENCIL_ATTACHMENT)
    // aspect - which part of the image the view accesses (COLOR or DEPTH)
    // Fixed to the given size, so must be recreated on resize. The caller is responsible for destroying the returned resources.
    fn create_image_resources(
        instance : &ash::Instance, 
        device : &ash::Device, 
        physical_device : vk::PhysicalDevice,
        format : Format,
        width :  u32,
        height : u32,
        usage : vk::ImageUsageFlags,
        aspect : vk::ImageAspectFlags) -> Result<ImageResources,String>{

         let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(Extent3D{ width, height, depth:1})
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(ImageTiling::OPTIMAL)   // must match find_depth_format
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe { device.create_image(&image_info, None) }
        .map_err(|e| format!("Failed to create image: {e}"))?;

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


    // Create a buffer the rendered image is copied into so the CPU can read its pixels
    // size - bytes to hold, width * height * 4 for an RGBA8 image, so must be recreated on resize
    // The memory stays mapped for the buffer's lifetime. The caller is responsible for unmapping and destroying it.
    fn create_readback_buffer(instance : &ash::Instance, device : &ash::Device, physical_device : vk::PhysicalDevice, size : vk::DeviceSize) -> Result<ReadbackBuffer, String> {

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
        let memory_type_index = Self::find_memory_type(instance, physical_device, requirements.memory_type_bits, coherent | vk::MemoryPropertyFlags::HOST_CACHED)
        .or_else(|_| Self::find_memory_type(instance, physical_device, requirements.memory_type_bits, coherent))
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


    // Create the fence that lets the CPU wait for submitted GPU work to finish
    fn create_fence(device : &ash::Device) -> Result<vk::Fence, String> {
        unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None) }
        .map_err(|e| format!("Failed to create fence: {e}"))
    }

    /// Creates the vertex buffer - stores vertex data that can be read by the graphics card
    fn create_vertex_buffer(instance : &ash::Instance, device : &ash::Device, physical_device : vk::PhysicalDevice,
        command_pool : vk::CommandPool,
        queue : vk::Queue,
        vertices : &[GpuPoint]) -> Result<Buffer, String>{

        let bytes : &[u8] = bytemuck::cast_slice(vertices);
        let size = bytes.len() as vk::DeviceSize;
        let usage = vk::BufferUsageFlags::TRANSFER_SRC;
        let properties = vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE;

        // Staging buffer
        let stage_buffer = Self::create_buffer(instance, device, physical_device, size, usage, properties)?;

        // Copy vertex data into the buffer - mapping buffer memory to CPU accesssible memory
        unsafe {
            let mapped = device.map_memory(stage_buffer.memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())
                .map_err(|e| format!("Failed to set up staging buffer: {e}"))? as *mut u8;

            std::ptr::copy_nonoverlapping(bytes.as_ptr(), mapped, bytes.len());   
            device.unmap_memory(stage_buffer.memory);         
        }

        // Create the vertex memoy (GPU-local memory)
        let usage = vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::VERTEX_BUFFER;
        let propertes = vk::MemoryPropertyFlags::DEVICE_LOCAL;
        let vertex_buffer = Self::create_buffer(instance, device, physical_device, size, usage, properties)?;

        // copy vertex data from staging to 
        Self::copy_buffer(device, command_pool, queue, stage_buffer, vertex_buffer, size);

        // destroy staging buffer
        unsafe {
            device.destroy_buffer(stage_buffer.buffer, None);
            device.free_memory(stage_buffer.memory, None);
        }

        Ok(vertex_buffer)
    }

    ///
    fn create_uniform_buffer(){
        todo!()
    }

    ///
    fn create_descriptor_pool(){
        todo!()
    }

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
        let memory_type_index = Self::find_memory_type(instance, physical_device, requirements.memory_type_bits, properties)?;
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

        Ok(Buffer {buffer, memory})
    }

    // Copy contents of one buffer to another throough a command buffer
    fn copy_buffer(device : &ash::Device, command_pool : vk::CommandPool, queue : vk::Queue, buffer_src : Buffer, buffer_dst : Buffer, size : u64){
        todo!()
    }

    pub fn render(&mut self, width: u32, height: u32, clear_color: [f32; 4]) -> Result<&[u8], String> {
        let start = Instant::now();

        // if size changed, update the render targets
        if self.render_size != (width, height) {
            self.recreate_render_targets(width, height)?;
        }
        let after_resize = Instant::now();

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

    // Record the commands for one frame: clear the colour and depth images, then copy the colour image to the readback buffer
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

            // Point drawing will go between these two calls
            self.device.cmd_begin_rendering(cmd, &rendering_info);
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

        self.color = Some(Self::create_image_resources(
            &self.instance,
            &self.device,
            self.physical_device,
            self.color_format,
            width,
            height,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
            vk::ImageAspectFlags::COLOR)?);

        self.depth = Some(Self::create_image_resources(
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
        self.readback = Some(Self::create_readback_buffer(
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

// Called by the validation layer whenever it has a message to report
unsafe extern "system" fn debug_callback(
    severity : vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type : vk::DebugUtilsMessageTypeFlagsEXT,
    callback_data : *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user_data : *mut c_void,
) -> vk::Bool32 {
    let message = unsafe { (*callback_data).message_as_c_str() }
        .map(CStr::to_string_lossy)
        .unwrap_or_default();

    eprintln!("[Vulkan {severity:?} {message_type:?}] {message}");

    vk::FALSE  // required return false
}

// Debug utils function loader and the messenger created from it
struct DebugMessenger {
    loader : ash::ext::debug_utils::Instance,
    messenger : vk::DebugUtilsMessengerEXT,
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

// Time spent in each part of render(), added up until the next report
#[derive(Default)]
struct RenderTimings {
    frames : u32,
    resize : Duration,
    record : Duration,
    submit : Duration,
    wait : Duration,
    total : Duration,
}

impl RenderTimings {
    const FRAMES_PER_REPORT : u32 = 10;

    // Print the average per frame and start counting again, once enough frames have been added
    fn report_if_due(&mut self) {
        if self.frames < Self::FRAMES_PER_REPORT { return; }

        let average_ms = |time : Duration| time.as_secs_f64() * 1000.0 / self.frames as f64;
        eprintln!(
            "[render avg over {} frames] resize {:.3} ms | record {:.3} ms | submit {:.3} ms | gpu wait {:.3} ms | total {:.3} ms",
            self.frames,
            average_ms(self.resize),
            average_ms(self.record),
            average_ms(self.submit),
            average_ms(self.wait),
            average_ms(self.total),
        );

        *self = Self::default();
    }
}

// Buffer the rendered image is copied into, its CPU-visible memory, and the pointer to read it through
#[derive(Clone, Copy)]
struct ReadbackBuffer {
    buffer : vk::Buffer,
    memory : vk::DeviceMemory,
    mapped : *const u8,
}

#[derive(Clone, Copy)]
struct Buffer {
    buffer : vk::Buffer,
    memory : vk::DeviceMemory,
}

// Image, its backing memory, and the view used to attach it when rendering
struct ImageResources {
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