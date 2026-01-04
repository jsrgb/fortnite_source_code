use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSUInteger;
use objc2_metal::*;

pub struct Device {
    pub device: Retained<ProtocolObject<dyn MTLDevice>>,
    pub command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
}

#[derive(Copy, Clone)]
pub enum BufferKind {
    POSITIONS = 1,
    UV = 2,
}

pub struct Buffer {
    pub buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
    // NOTE: bindless coming soon
    pub binding: BufferKind,
}

impl Buffer {
    // TODO: Think about making more generic in the future
    pub fn new(
        device: &Retained<ProtocolObject<dyn MTLDevice>>,
        length: usize,
        vertex_size: usize,
        storage_mode: MTLResourceOptions,
        bindslot: BufferKind,
        // TODO: buffer name. How can we name and track resources?
    ) -> Buffer {
        Buffer {
            buffer: device
                .newBufferWithLength_options((length * vertex_size) as NSUInteger, storage_mode)
                .expect("Failed to create buffer"),
            binding: bindslot,
        }
    }
}

pub struct Mesh {
    pub buffers: Vec<Buffer>,
    pub index_buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
    // TODO: List of materials
    pub materials: Option<Retained<ProtocolObject<dyn MTLTexture>>>,
    pub index_count: usize,
    pub primitive: MTLPrimitiveType,
}

impl Mesh {
    pub fn new(
        buffers: Vec<Buffer>,
        index_buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
        material: Option<Retained<ProtocolObject<dyn MTLTexture>>>,
        index_count: usize,
        primitive: MTLPrimitiveType,
    ) -> Self {
        Self {
            buffers,
            index_buffer,
            materials: material, // TODO:
            index_count,
            primitive,
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
