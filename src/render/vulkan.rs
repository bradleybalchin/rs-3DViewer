use ash::vk::{self, DeviceCreateInfo, QueueFlags};

pub struct VulkanRenderer {
    _entry : ash::Entry,                     // Vulkan Functions Library
    instance : ash::Instance,                // Vulkan Insance
    physical_device : vk::PhysicalDevice,    // phyiscal graphics device (GPU)
    device  : ash::Device,                    // Vulkan Logical Device
    queues : Queues                          // Queues retrieved from the logical device
}

impl VulkanRenderer {
    pub fn new() -> Result<Self, String>{
        let (entry, instance) = Self::create_instance()?;

        let physical_device = Self::select_physical_device(&instance)?;             // physical graphics device
        let indices = Self::find_queue_families(&instance, physical_device)?;   // indices of queue families being used
        let (device, queues) = Self::create_logical_device( &instance, &indices, physical_device)?;   // vulkan logical graphics device and its queues


        Ok(Self {
            _entry : entry,
            instance,
            physical_device,
            device,
            queues,
        })
    }

    // Create the vulkan instance
    fn create_instance() -> Result<(ash::Entry, ash::Instance), String> {
        let entry = unsafe { ash::Entry::load() }
            .map_err(|e| format!("failed to load Vulkan: {e}"))?;

        let app_info = vk::ApplicationInfo::default()
            .application_name(c"3DViewer")
            .api_version(vk::API_VERSION_1_2);

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
                    props.device_type == vk::PhysicalDeviceType::DISCRETE_GPU
                } );
            
            // if no discrete exists just return the first physical device
            Ok(discrete.unwrap_or(physical_devices[0]))   
    }

    // Create logical device - interface between physical graphics and application (queue family - group of GPU queues that share same capabilities)
    // Also retrieves the queues created with the device
    fn create_logical_device(instance : &ash::Instance, indices : &QueueFamilyIndices, physical_device : vk::PhysicalDevice) -> Result<(ash::Device, Queues), String> {
    
        let device_features = unsafe { instance.get_physical_device_features(physical_device) };

        let queue_priorities = [1.0f32];

        let graphics_queue_create_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(indices.graphics_family)
        .queue_priorities(&queue_priorities);

        let queue_create_info = [graphics_queue_create_info];


        let device_create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_create_info)
        .enabled_features(&device_features);

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
    
    pub fn render(&mut self, width: u32, height: u32, clear_color: [f32; 4]) -> Result<&[u8], vk::Result>{
        todo!()
    }



    // TODO : Destroy vulkan objects on dispose

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