use glam::Mat4;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSUInteger;
use objc2_metal::*;
use std::ptr::NonNull;

use crate::resource::{Buffer, BufferKind};

#[derive(Copy, Clone)]
#[repr(C)]
pub struct Uniforms {
    pub view_proj: Mat4,
    pub model: Mat4,
    pub time: f32,
}

pub trait RenderPass {
    // TODO: make Generic
    fn render(
        &self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        uniforms: &Uniforms,
        model: &Asset,
        time: f32,
    );
}

// The pass owns the resources
pub struct SinglePass {
    pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    depth_stencil_state: Retained<ProtocolObject<dyn MTLDepthStencilState>>,
}

impl SinglePass {
    pub fn new(
        pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
        depth_stencil_state: Retained<ProtocolObject<dyn MTLDepthStencilState>>,
    ) -> Self {
        Self {
            pipeline,
            depth_stencil_state,
        }
    }
}

impl RenderPass for SinglePass {
    fn render(
        &self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        uniforms: &Uniforms,
        model: &Asset,
        _time: f32,
    ) {
        encoder.setRenderPipelineState(&self.pipeline);
        encoder.setDepthStencilState(Some(&self.depth_stencil_state));

        for mesh in &model.meshes {
            unsafe {
                // uplaod uniforms
                let m_uniforms = Uniforms {
                    view_proj: uniforms.view_proj,
                    time: uniforms.time,
                    model: mesh.model,
                };
                encoder.setVertexBytes_length_atIndex(
                    NonNull::from(&m_uniforms).cast(),
                    std::mem::size_of_val(&m_uniforms),
                    0,
                );
            }
            unsafe {
                for material in &mesh.materials {
                    if let Some(texture) = material {
                        encoder.setFragmentTexture_atIndex(Some(texture), 0);
                    } else {
                        encoder.setFragmentTexture_atIndex(None, 0);
                    }
                }
            }
            mesh.draw(encoder);
        }
    }
}

// Skybox render pass
pub struct SkyboxPass {
    pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    depth_stencil_state: Retained<ProtocolObject<dyn MTLDepthStencilState>>,
    cube_mesh: Mesh,
    cube_texture: Retained<ProtocolObject<dyn MTLTexture>>,
}

impl SkyboxPass {
    pub fn new(
        pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
        depth_stencil_state: Retained<ProtocolObject<dyn MTLDepthStencilState>>,
        cube_mesh: Mesh,
        cube_texture: Retained<ProtocolObject<dyn MTLTexture>>,
    ) -> Self {
        Self {
            pipeline,
            depth_stencil_state,
            cube_mesh,
            cube_texture,
        }
    }

    pub fn render(
        &self,
        encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        view_proj: Mat4,
    ) {
        encoder.setRenderPipelineState(&self.pipeline);
        encoder.setDepthStencilState(Some(&self.depth_stencil_state));

        // Set the view-projection matrix
        unsafe {
            encoder.setVertexBytes_length_atIndex(
                NonNull::from(&view_proj).cast(),
                std::mem::size_of_val(&view_proj),
                0,
            );
        }

        // Set the cube texture
        unsafe {
            encoder.setFragmentTexture_atIndex(Some(&self.cube_texture), 0);
        }

        // Draw the cube
        self.cube_mesh.draw(encoder);
    }
}

// Mesh, Asset, should be omved somewhere else. leave this file for MTL resources
pub struct Mesh {
    pub buffers: Vec<Buffer>,
    pub index_buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
    // TODO: Type alias or whatever its called again
    pub materials: Vec<Option<Retained<ProtocolObject<dyn MTLTexture>>>>,
    pub index_count: usize,
    pub primitive: MTLPrimitiveType,
    pub model: Mat4,
}

impl Mesh {
    pub fn new(
        buffers: Vec<Buffer>,
        index_buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
        materials: Vec<Option<Retained<ProtocolObject<dyn MTLTexture>>>>,
        index_count: usize,
        primitive: MTLPrimitiveType,
        model: Mat4,
    ) -> Self {
        Self {
            buffers,
            index_buffer,
            materials,
            index_count,
            primitive,
            model,
        }
    }

    pub fn draw(&self, encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>) {
        unsafe {
            for buffer in &self.buffers {
                encoder.setVertexBuffer_offset_atIndex(
                    Some(&buffer.buffer),
                    0,
                    buffer.binding as NSUInteger,
                );
            }
            encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                self.primitive,
                self.index_count,
                MTLIndexType::UInt32,
                &self.index_buffer,
                0,
            );
        }
    }
}

// i.e. glTF
pub struct Asset {
    // TODO: constructors
    pub meshes: Vec<Mesh>,
    // TODO: materials
    pub _name: String,
}

// Create a cube mesh for skybox rendering
pub fn create_cube_mesh(device: &Retained<ProtocolObject<dyn MTLDevice>>) -> Mesh {
    // Cube vertices (8 corners, size 1.0, centered at origin)
    #[rustfmt::skip]
    let positions: [f32; 24] = [
        -1.0,  1.0,  1.0, // 0: front-top-left
         1.0,  1.0,  1.0, // 1: front-top-right
         1.0, -1.0,  1.0, // 2: front-bottom-right
        -1.0, -1.0,  1.0, // 3: front-bottom-left
        -1.0,  1.0, -1.0, // 4: back-top-left
         1.0,  1.0, -1.0, // 5: back-top-right
         1.0, -1.0, -1.0, // 6: back-bottom-right
        -1.0, -1.0, -1.0, // 7: back-bottom-left
    ];

    // 36 indices for 12 triangles (2 per face, 6 faces)
    // Winding order is set to render from inside the cube
    #[rustfmt::skip]
    let indices: [u32; 36] = [
        // Front face (Z+)
        0, 1, 2,  0, 2, 3,
        // Back face (Z-)
        5, 4, 7,  5, 7, 6,
        // Top face (Y+)
        4, 5, 1,  4, 1, 0,
        // Bottom face (Y-)
        3, 2, 6,  3, 6, 7,
        // Right face (X+)
        1, 5, 6,  1, 6, 2,
        // Left face (X-)
        4, 0, 3,  4, 3, 7,
    ];

    // Create position buffer
    let position_buffer = unsafe {
        device
            .newBufferWithBytes_length_options(
                NonNull::new(positions.as_ptr() as *mut _).unwrap(),
                std::mem::size_of_val(&positions),
                MTLResourceOptions::StorageModeManaged,
            )
            .expect("Failed to create position buffer")
    };

    let buffer = Buffer {
        buffer: position_buffer,
        binding: BufferKind::POSITIONS,
    };

    // Create index buffer
    let index_buffer = unsafe {
        device
            .newBufferWithBytes_length_options(
                NonNull::new(indices.as_ptr() as *mut _).unwrap(),
                std::mem::size_of_val(&indices),
                MTLResourceOptions::StorageModeManaged,
            )
            .expect("Failed to create index buffer")
    };

    Mesh::new(
        vec![buffer],
        index_buffer,
        vec![],        // No materials for skybox
        indices.len(),
        MTLPrimitiveType::Triangle,
        Mat4::IDENTITY,
    )
}
