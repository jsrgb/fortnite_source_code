use glam::{Mat4, Vec3};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{ns_string, NSString, NSUInteger, NSURL};
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
        time: f32,
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
                if let Some(texture) = &mesh.materials {
                    encoder.setFragmentTexture_atIndex(Some(texture), 0);
                } else {
                    encoder.setFragmentTexture_atIndex(None, 0);
                }
            }
            mesh.draw(encoder);
        }
    }
}

// Mesh, Asset, should be omved somewhere else. leave this file for MTL resources
pub struct Mesh {
    pub buffers: Vec<Buffer>,
    pub index_buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
    // TODO: List of materials
    pub materials: Option<Retained<ProtocolObject<dyn MTLTexture>>>,
    pub index_count: usize,
    pub primitive: MTLPrimitiveType,
    pub model: Mat4,
}

impl Mesh {
    pub fn new(
        buffers: Vec<Buffer>,
        index_buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
        material: Option<Retained<ProtocolObject<dyn MTLTexture>>>,
        index_count: usize,
        primitive: MTLPrimitiveType,
        model: Mat4,
    ) -> Self {
        Self {
            buffers,
            index_buffer,
            materials: material, // TODO:
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
    pub name: String,
}
