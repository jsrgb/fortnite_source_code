use glam::Vec3;

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
}
