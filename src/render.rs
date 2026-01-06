use crate::resource::Asset;
use glam::{Mat4, Vec3};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::*;
use std::ptr::NonNull;

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
