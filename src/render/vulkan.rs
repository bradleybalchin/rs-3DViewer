use ash::vk;

pub struct VulkanRenderer {
    _entry : ash::Entry,                     // Vulkan Functions Library
    instance : ash::Instance,                // Vulkan Insance
    physical_device : vk::PhysicalDevice,    // phyiscal graphics device (GPU)
}

impl VulkanRenderer {
    pub fn new() -> Result<Self, String>{
        let (entry, instance) = Self::create_instance()?;
        let physical_device = Self::select_physical_device(&instance)?;


        Ok(Self {
            _entry : entry,
            instance,
            physical_device
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

    pub fn render(&mut self, width: u32, height: u32, clear_color: [f32; 4]) -> Result<&[u8], vk::Result>{
        todo!()
    }
}