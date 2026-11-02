use ash::vk::{self, DescriptorType, Pipeline, QueueFlags};

pub struct VulkanRenderer {
    _entry : ash::Entry,                     // Vulkan Functions Library
    instance : ash::Instance,                // Vulkan Insance
    physical_device : vk::PhysicalDevice,    // phyiscal graphics device (GPU)
    device  : ash::Device,                    // Vulkan Logical Device
    queues : Queues,                          // Queues retrieved from the logical device
    descriptor_set : vk::DescriptorSetLayout,  // descriptor set layout for describing resources to shaders (uniforms, textures, etc.)
}

impl VulkanRenderer {
    pub fn new() -> Result<Self, String>{
        let (entry, instance) = Self::create_instance()?;

        let physical_device = Self::select_physical_device(&instance)?;             // physical graphics device
        let indices = Self::find_queue_families(&instance, physical_device)?;   // indices of queue families being used
        let (device, queues) = Self::create_logical_device( &instance, &indices, physical_device)?;   // vulkan logical graphics device and its queues
        
        let descriptor_set = Self::create_descriptor_set_layout(&device)?;  // descriptor set layout for describing resources to shaders (uniforms, textures, etc.)


        Ok(Self {
            _entry : entry,
            instance,
            physical_device,
            device,
            queues,
            descriptor_set
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
    fn create_shader_module(device : &ash::Device, code : &[u8]) -> Result<vk::ShaderModule, String> {
        let shader_info = vk::ShaderModuleCreateInfo::default()
        .code(bytemuck::cast_slice(code));

        unsafe { device.create_shader_module(&shader_info, None) }
        .map_err(|e| format!("Failed to create shader module: {e}"))
    }

    // Configure shaders and fixed function stages of the graphics pipeline
    fn create_graphics_pipeline(device : &ash::Device) -> Result<Pipeline, String> {
        
        // Load shaders from SPIR-V files
        let vert_shader_code = include_bytes!("shaders/shader.vert.spv");
        let frag_shader_code = include_bytes!("shaders/shader.frag.spv");

        let vert_shader_module = Self::create_shader_module(device, vert_shader_code)?;
        let frag_shader_module = Self::create_shader_module(device, frag_shader_code)?;

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

        

        todo!()
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