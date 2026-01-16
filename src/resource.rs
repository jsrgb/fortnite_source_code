use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{ns_string, NSString, NSUInteger, NSURL};
use objc2_metal::*;

pub struct Device {
    pub device: Retained<ProtocolObject<dyn MTLDevice>>,
    pub command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
}

#[derive(Copy, Clone)]
pub enum BufferKind {
    POSITIONS = 1,
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

pub struct ShaderLibrary {
    pub vertex: Retained<ProtocolObject<dyn MTLFunction>>,
    pub fragment: Retained<ProtocolObject<dyn MTLFunction>>,
    _name: String,
}

impl ShaderLibrary {
    pub fn new(
        name: String,
        filepath: String,
        device: &Retained<ProtocolObject<dyn MTLDevice>>,
    ) -> Self {
        let path = NSString::from_str(&filepath);
        let url = { NSURL::fileURLWithPath(&path) };
        let library = device
            .newLibraryWithURL_error(&url)
            .expect("Failed to compile shaders");

        // fixme: (im lazy)
        let vertex_fn = library
            .newFunctionWithName(ns_string!("vertex_main"))
            .expect("could not create vertex fn");
        let fragment_fn = library
            .newFunctionWithName(ns_string!("fragment_main"))
            .expect("could not create fragment fn");

        Self {
            vertex: vertex_fn,
            fragment: fragment_fn,
            _name: name,
        }
    }
}

pub struct VertexAttribute {
    pub format: MTLVertexFormat,
    pub offset: u8,
    pub index: u8,
    pub buffer_id: u8,
}

pub struct VertexDescriptor {
    pub attributes: Vec<VertexAttribute>,
}

impl VertexDescriptor {
    pub fn new(attributes: Vec<VertexAttribute>) -> Retained<MTLVertexDescriptor> {
        let _desc = MTLVertexDescriptor::new();

        unsafe {
            for attr in attributes {
                let a = _desc
                    .attributes()
                    .objectAtIndexedSubscript(attr.index as NSUInteger);
                a.setFormat(attr.format);
                a.setOffset(attr.offset as NSUInteger);
                a.setBufferIndex(attr.buffer_id as NSUInteger);

                // TODO: track stride here
            }
        }

        let stride = std::mem::size_of::<[f32; 8]>() as NSUInteger;

        unsafe {
            let layout = _desc.layouts().objectAtIndexedSubscript(1);
            layout.setStride(stride);
            layout.setStepFunction(MTLVertexStepFunction::PerVertex);
            layout.setStepRate(1);
        }

        return _desc;
    }
}
