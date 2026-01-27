use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::MTLTexture;

use crate::render::Mesh;

pub struct Skybox {
    pub mesh: Mesh,
    pub texture: Retained<ProtocolObject<dyn MTLTexture>>,
}

pub struct World {
    pub meshes: Vec<Mesh>,
    //pub skybox: Option<Skybox>,
    // lights
    // transforms
}
