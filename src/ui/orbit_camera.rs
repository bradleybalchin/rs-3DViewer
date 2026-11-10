use glam::{Mat4, Vec3, camera};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

const FOV_Y: f32 = FRAC_PI_4; // 45 degrees
const ORBIT_SPEED: f32 = 0.01; // radians per point dragged
const ZOOM_SPEED: f32 = 0.002;

/// Orbits around a target point, Z up to match LAS
pub struct OrbitCamera {
    target: Vec3,
    yaw: f32,      // angle around Z
    pitch: f32,    // angle above the XY plane
    distance: f32, // from eye to target
    radius: f32,   // scene size, scales zoom limits and clip planes
}

impl OrbitCamera {
    pub fn new() -> Self {
        let mut camera = Self { target: Vec3::ZERO, yaw: 0.0, pitch: 0.0, distance: 1.0, radius: 1.0 };
        camera.frame(1.0);
        camera
    }

    /// Reset the view so a scene of this radius is fully visible
    pub fn frame(&mut self, radius: f32) {
        self.radius = radius.max(1.0);
        self.target = Vec3::ZERO;
        self.yaw = -FRAC_PI_4;
        self.pitch = 0.6155; // about 35 degrees
        self.distance = self.radius * 2.6;
    }

    pub fn reset(&mut self) {
        self.frame(self.radius);
    }

    fn eye(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        self.target + self.distance * Vec3::new(cos_pitch * cos_yaw, cos_pitch * sin_yaw, sin_pitch)
    }
    
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * ORBIT_SPEED;
        // stop short of straight up or down, where look_at breaks
        let limit = FRAC_PI_2 - 0.01;
        self.pitch = (self.pitch + dy * ORBIT_SPEED).clamp(-limit, limit);
    }

    /// viewport_height is in the same units as dx and dy, so the scene tracks the cursor
    pub fn pan(&mut self, dx: f32, dy: f32, viewport_height: f32) {
        let forward = (self.target - self.eye()).normalize();
        let right = forward.cross(Vec3::Z).normalize();
        let up = right.cross(forward);
        let world_per_point = 2.0 * self.distance * (FOV_Y * 0.5).tan() / viewport_height;
        self.target += (up * dy - right * dx) * world_per_point;
    }

    /// Zoom the camera towards/away from the center of the orbit
    pub fn zoom(&mut self, scroll: f32) {
        let scaled = self.distance * (-scroll * ZOOM_SPEED).exp();
        self.distance = scaled.clamp(self.radius * 0.01, self.radius * 20.0);
    }

    /// Returns (view, proj)
    pub fn matrices(&self, aspect: f32) -> (Mat4, Mat4) {
        let view = camera::rh::view::look_at_mat4(self.eye(), self.target, Vec3::Z);
        let near = self.distance * 0.01;                // near plane
        let far = self.distance + self.radius * 4.0;    // far plane
        (view, camera::rh::proj::vulkan::perspective(FOV_Y, aspect, near, far))
    }
}