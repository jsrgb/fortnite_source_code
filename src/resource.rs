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

pub struct ShaderLibrary {
    pub vertex: Retained<ProtocolObject<dyn MTLFunction>>,
    pub fragment: Retained<ProtocolObject<dyn MTLFunction>>,
    name: String,
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
            name,
        }
    }
}
