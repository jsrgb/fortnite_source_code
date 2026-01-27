use glam::{Mat4, Vec3};

pub struct Camera {
    pub position: Vec3,
    pub front: Vec3,
    pub up: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

impl Camera {
    pub fn new(position: Vec3, front: Vec3, up: Vec3, yaw: f32, pitch: f32) -> Self {
        Self {
            position,
            front,
            up,
            yaw,
            pitch,
        }
    }

    pub fn view_proj(&self, fov: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let view = Mat4::look_at_rh(self.position, self.position + self.front, self.up);
        let projection = Mat4::perspective_rh(fov.to_radians(), aspect, near, far);
        projection * view
    }
}
