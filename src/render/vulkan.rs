use ash::vk;

pub struct VulkanRenderer {

}

impl VulkanRenderer {
    pub fn new() -> Result<Self, String>{
        todo!()
    }


    pub fn render(&mut self, width: u32, height: u32, clear_color: [f32; 4]) -> Result<&[u8], vk::Result>{
        todo!()
    }
}