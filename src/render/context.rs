//! # Context
//!
//! Holds functions and structs relating to instance, validation, physical device selection, queue families, and creating the logical device

use ash::vk;
use std::ffi::{CStr, c_void};

const VALIDATION_LAYER : &CStr = c"VK_LAYER_KHRONOS_validation";

// Check the validation layer is installed
pub(super) fn validation_layer_available(entry : &ash::Entry) -> Result<bool, String> {
    let layers = unsafe { entry.enumerate_instance_layer_properties() }
        .map_err(|e| format!("Failed to enumerate instance layers: {e}"))?;

    Ok(layers.iter().any(|layer| layer.layer_name_as_c_str() == Ok(VALIDATION_LAYER)))
}

// Which validation messages to receive and the function that handles them
pub(super) fn debug_messenger_info<'a>() -> vk::DebugUtilsMessengerCreateInfoEXT<'a> {
    vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR)
        .message_type(vk::DebugUtilsMessageTypeFlagsEXT::GENERAL | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE)
        .pfn_user_callback(Some(debug_callback))
}

// Create the messenger that sends validation messages to debug_callback
pub(super) fn create_debug_messenger(entry : &ash::Entry, instance : &ash::Instance) -> Result<DebugMessenger, String> {
    let loader = ash::ext::debug_utils::Instance::new(entry, instance);

    let messenger = unsafe { loader.create_debug_utils_messenger(&debug_messenger_info(), None) }
        .map_err(|e| format!("Failed to create debug messenger: {e}"))?;

    Ok(DebugMessenger { loader, messenger })
}

// Create the vulkan instance
pub(super) fn create_instance(validation : bool) -> Result<(ash::Entry, ash::Instance), String> {
    let entry = unsafe { ash::Entry::load() }
        .map_err(|e| format!("failed to load Vulkan: {e}"))?;

    let app_info = vk::ApplicationInfo::default()
        .application_name(c"3DViewer")
        .api_version(vk::API_VERSION_1_3);

    // These must outlive create_instance, as create_info only stores pointers to them
    let layer_names = [VALIDATION_LAYER.as_ptr()];
    let extension_names = [ash::ext::debug_utils::NAME.as_ptr()];
    let mut debug_info = debug_messenger_info();

    let mut create_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info);

    if validation {
        if !validation_layer_available(&entry)? {
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
pub(super) fn select_physical_device(instance : &ash::Instance) -> Result<vk::PhysicalDevice, String> {
    
        let physical_devices = unsafe { instance.enumerate_physical_devices()}
            .map_err(|e| format!("failed to enumurate devices : {e}"))?;
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
pub(super) fn create_logical_device(instance : &ash::Instance, indices : &QueueFamilyIndices, physical_device : vk::PhysicalDevice) -> Result<(ash::Device, Queues), String> {

    let device_features = vk::PhysicalDeviceFeatures::default().large_points(true);

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
pub(super) fn find_queue_families(instance : &ash::Instance, physical_device : vk::PhysicalDevice) -> Result<QueueFamilyIndices, String> {
    
    let queue_families = unsafe{instance.get_physical_device_queue_family_properties(physical_device)};
    if queue_families.is_empty() { return Err("Failed to Find Queue Families".into()); }
    

    let i_graphics = 
    queue_families.iter().position(|f| f.queue_flags.contains(vk::QueueFlags::GRAPHICS) && f.queue_count > 0)
    .ok_or("No queue families with graphics support detected")?;
    
    let i_compute = queue_families.iter().position(|f| f.queue_flags.contains(vk::QueueFlags::COMPUTE) && f.queue_count > 0)
    .ok_or("No queue families with compute support detected")?;

    Ok(QueueFamilyIndices { graphics_family: i_graphics as u32, present_family: 0 as u32, compute_family: i_compute as u32 })
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
pub(super) struct DebugMessenger {
    pub(super) loader : ash::ext::debug_utils::Instance,
    pub(super) messenger : vk::DebugUtilsMessengerEXT,
}


// Indicies of used Queue Families
pub(super) struct QueueFamilyIndices {
    pub(super) graphics_family : u32,
    pub(super) present_family : u32,
    pub(super) compute_family :u32,
}

// Queues from the logical device, grouped by role
pub(super) struct Queues {
    pub(super) graphics : Vec<vk::Queue>,
    pub(super) present  : Vec<vk::Queue>,
    pub(super) compute  : Vec<vk::Queue>,
}