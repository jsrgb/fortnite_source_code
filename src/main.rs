#![deny(unsafe_op_in_unsafe_fn)]

mod platform;

// TODO: What?
use objc2::AnyThread;

use crate::platform::Delegate;
use crate::platform::Ivars;
use crate::platform::KEYSTATE;

use objc2::MainThreadOnly;
use std::fmt::Debug;
use std::ptr::NonNull;

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{msg_send, MainThreadMarker};

use glam::{Mat4, Vec2, Vec3, Vec4};

use objc2_foundation::{ns_string, NSDate, NSPoint, NSRect, NSSize, NSUInteger, NSURL};

// TODO: Move and improve
const KEY_W: u16 = 13;
const KEY_A: u16 = 0;
const KEY_S: u16 = 1;
const KEY_D: u16 = 2;

use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSWindow, NSWindowStyleMask,
};

use objc2_metal::{
    MTLBuffer, MTLCPUCacheMode, MTLClearColor, MTLCommandBuffer, MTLCommandEncoder,
    MTLCommandQueue, MTLCompareFunction, MTLCreateSystemDefaultDevice, MTLDepthStencilDescriptor,
    MTLDepthStencilState, MTLDevice, MTLHeap, MTLHeapDescriptor, MTLIndexType, MTLLibrary,
    MTLPixelFormat, MTLPrimitiveType, MTLRenderCommandEncoder, MTLRenderPipelineDescriptor,
    MTLRenderPipelineState, MTLResourceOptions, MTLStorageMode, MTLTexture, MTLVertexDescriptor,
    MTLVertexFormat, MTLVertexStepFunction,
};

use objc2_metal_kit::{MTKTextureLoader, MTKView};

#[derive(Copy, Clone)]
#[repr(C)]
struct Uniforms {
    view_proj: Mat4,
    time: f32,
}

struct Mesh {
    vertex_buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
    index_buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
    index_count: usize,
    primitive: MTLPrimitiveType,
}

impl Mesh {
    fn draw(&self, encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>) {
        unsafe {
            encoder.setVertexBuffer_offset_atIndex(Some(&self.vertex_buffer), 0, 1);
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

struct Model {
    mesh: Vec<Mesh>,
    name: String,
}

struct Camera {
    position: Vec3,
    target: Vec3,
    direction: Vec3,
    front: Vec3,
    up: Vec3,
}

const WINDOW_W: f64 = 800.0;
const WINDOW_H: f64 = 600.0;

struct Device {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    texture_loader: Retained<MTKTextureLoader>,
}

pub struct AppState {
    start_date: Retained<NSDate>,
    device: Device,
    pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    depth_stencil_state: Retained<ProtocolObject<dyn MTLDepthStencilState>>, // FIXME: move
    model: Model,
    // RefCell? In frame() an immutable reference to AppState is passed in.
    // But camera state needs to mutate when input is pressed
    // RefCell allows for mutable borrows at runtime, even when the data is immutable
    camera: RefCell<Camera>,
}

pub fn init() -> (AppState, Retained<NSWindow>, Retained<MTKView>) {
    let mtm = MainThreadMarker::new().unwrap();

    let window = {
        let content_rect = NSRect::new(NSPoint::new(0., 0.), NSSize::new(WINDOW_W, WINDOW_H));
        let style =
            NSWindowStyleMask::Closable | NSWindowStyleMask::Resizable | NSWindowStyleMask::Titled;

        unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                content_rect,
                style,
                NSBackingStoreType::Buffered,
                false,
            )
        }
    };

    let device = MTLCreateSystemDefaultDevice().expect("No Metal device");
    let command_queue = device
        .newCommandQueue()
        .expect("Failed to create command queue");

    let view = {
        let frame_rect = window.frame();
        let mtk_view = MTKView::initWithFrame(MTKView::alloc(mtm), frame_rect);
        mtk_view.setDevice(Some(&device));
        mtk_view.setDepthStencilPixelFormat(MTLPixelFormat::Depth32Float);

        mtk_view
    };

    let pipeline_descriptor = MTLRenderPipelineDescriptor::new();
    unsafe {
        pipeline_descriptor
            .colorAttachments()
            .objectAtIndexedSubscript(0)
            .setPixelFormat(view.colorPixelFormat());
    }

    //
    // init Metal Kit Texture Loader
    let mtk_tex_loader = MTKTextureLoader::initWithDevice(MTKTextureLoader::alloc(), &device);
    let tex_to_load = { NSURL::fileURLWithPath(ns_string!("./src/assets/grass.png")) };

    unsafe {
        mtk_tex_loader
            .newTextureWithContentsOfURL_options_error(&tex_to_load, None)
            .inspect(|_| println!("texture loaded"))
            .expect("failed to laod texture");
    }

    // FIXME: absolute path
    let url = { NSURL::fileURLWithPath(ns_string!("./src/cube.metallib")) };
    let library = device
        .newLibraryWithURL_error(&url)
        .expect("Failed to compile shaders");

    let vertex_fn = library.newFunctionWithName(ns_string!("vertex_main"));
    let frag_fn = library.newFunctionWithName(ns_string!("fragment_main"));

    pipeline_descriptor.setVertexFunction(vertex_fn.as_deref());
    pipeline_descriptor.setFragmentFunction(frag_fn.as_deref());

    // Add depth stencil attachment
    pipeline_descriptor.setDepthAttachmentPixelFormat(MTLPixelFormat::Depth32Float);

    // A MTLVertexDescriptor has attributes and layouts
    let vertex_descriptor = MTLVertexDescriptor::new();

    // Attribute 0: position (float3) at offset 0 in buffer(1)
    unsafe {
        let a0 = vertex_descriptor.attributes().objectAtIndexedSubscript(0);
        a0.setFormat(MTLVertexFormat::Float3);
        a0.setOffset(0);
        a0.setBufferIndex(1);
    }

    // layouts describe how to fetch (stride,offset)
    // Layout for buffer(1): stride = 24 bytes
    unsafe {
        let layout = vertex_descriptor.layouts().objectAtIndexedSubscript(1);
        layout.setStride(std::mem::size_of::<[f32; 3]>() as NSUInteger); // 24
        layout.setStepFunction(MTLVertexStepFunction::PerVertex);
        layout.setStepRate(1);
    }

    // Attached vertex spec to pipeline
    pipeline_descriptor.setVertexDescriptor(Some(&vertex_descriptor));

    let pipeline_state = device
        .newRenderPipelineStateWithDescriptor_error(&pipeline_descriptor)
        .expect("Failed to create pipeline state");

    view.setClearColor(MTLClearColor {
        red: 0.0,
        green: 0.5,
        blue: 1.0,
        alpha: 1.0,
    });

    window.setContentView(Some(&view));
    window.center();
    window.setTitle(ns_string!("aa"));
    window.makeKeyAndOrderFront(None);

    // Depth stencil
    let depth_stencil_descriptor = MTLDepthStencilDescriptor::new();
    depth_stencil_descriptor.setDepthCompareFunction(MTLCompareFunction::Less);
    depth_stencil_descriptor.setDepthWriteEnabled(true);
    let depth_stencil_state = device
        .newDepthStencilStateWithDescriptor(&depth_stencil_descriptor)
        .expect("Failed to create depth stencil state");

    // create a heap for long lived data,  everything else goes on the stack
    let heap_descriptor = MTLHeapDescriptor::new();
    heap_descriptor.setSize(64 * 1024 * 1024);
    heap_descriptor.setStorageMode(MTLStorageMode::Shared);
    heap_descriptor.setCpuCacheMode(MTLCPUCacheMode::DefaultCache);
    let heap = {
        device
            .newHeapWithDescriptor(&heap_descriptor)
            .expect("Failed to create heap")
    };

    let (document, buffers, images) =
        gltf::import("./src/assets/FlightHelmet.gltf").expect("could not open gltf");
    assert_eq!(buffers.len(), document.buffers().count());
    assert_eq!(images.len(), document.images().count());

    let mut all_meshes = Vec::new();

    for material in document.materials() {
        //println!("material: {:#?}", material);
    }

    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

            let positions: Vec<[f32; 3]> = reader.read_positions().expect("No positions").collect();

            let indices: Vec<u32> = reader
                .read_indices()
                .expect("No indices")
                .into_u32()
                .collect();

            // allocate buffers
            let vertex_buffer = heap
                .newBufferWithLength_options(
                    (positions.len() * std::mem::size_of::<[f32; 3]>()) as NSUInteger,
                    MTLResourceOptions::StorageModeShared,
                )
                .expect("Failed to create vertex buffer");

            let index_buffer = heap
                .newBufferWithLength_options(
                    (indices.len() * std::mem::size_of::<[i32; 3]>()) as NSUInteger,
                    MTLResourceOptions::StorageModeShared,
                )
                .expect("Failed to create index buffer");

            // fill them
            unsafe {
                let contents = vertex_buffer.contents().as_ptr() as *mut f32;
                std::ptr::copy_nonoverlapping(
                    positions.as_ptr() as *const f32,
                    contents,
                    positions.len() * 3,
                );
            }

            unsafe {
                let contents = index_buffer.contents().as_ptr() as *mut u32;
                std::ptr::copy_nonoverlapping(indices.as_ptr(), contents, indices.len());
            }

            let submesh = Mesh {
                vertex_buffer,
                index_buffer,
                index_count: indices.len(),
                primitive: MTLPrimitiveType::Triangle,
            };

            all_meshes.push(submesh);
        }
    }

    let cam_position = Vec3::new(0.0, 0.5, 3.0);
    let cam_target = Vec3::new(0.0, 0.0, 0.0);
    let camera = Camera {
        position: cam_position,
        target: cam_target,
        direction: Vec3::normalize(cam_position - cam_target),
        front: Vec3::new(0.0, 0.0, -1.0), // Looking at -Z
        up: Vec3::new(0.0, 1.0, 0.0),
    };

    let app_state = AppState {
        start_date: NSDate::now(),
        device: Device {
            device,
            command_queue,
            texture_loader: mtk_tex_loader,
        },
        depth_stencil_state,
        pipeline: pipeline_state,
        model: Model {
            mesh: all_meshes,
            name: "Box".to_string(),
        },
        camera: RefCell::new(camera),
    };
    (app_state, window, view)
}

pub fn frame(view: &MTKView, state: &AppState) {
    let keys = KEYSTATE.lock().unwrap();
    let mut camera = state.camera.borrow_mut();

    let move_speed = 0.05;

    let front = camera.front;
    let right = camera.front.cross(camera.up).normalize();

    if keys.contains(&KEY_W) {
        camera.position += front * move_speed;
    }
    if keys.contains(&KEY_S) {
        camera.position -= front * move_speed;
    }
    if keys.contains(&KEY_A) {
        camera.position -= right * move_speed;
    }
    if keys.contains(&KEY_D) {
        camera.position += right * move_speed;
    }

    drop(keys); // Release the lock

    let Some(drawable) = view.currentDrawable() else {
        return;
    };
    let Some(command_buffer) = state.device.command_queue.commandBuffer() else {
        return;
    };
    let Some(pass_desc) = view.currentRenderPassDescriptor() else {
        return;
    };
    let Some(encoder) = command_buffer.renderCommandEncoderWithDescriptor(&pass_desc) else {
        return;
    };

    // https://learnopengl.com/Getting-started/Camera
    let aspect_ratio = WINDOW_W as f32 / WINDOW_H as f32;
    let projection = glam::Mat4::perspective_rh(
        45.0_f32.to_radians(),
        aspect_ratio,
        0.025, // near plane
        500.0, // far plane
    );

    let view = Mat4::look_at_rh(camera.position, camera.position + camera.front, camera.up);
    drop(camera);

    let view_proj = projection * view;

    let uniforms = Uniforms {
        view_proj,
        time: state.start_date.timeIntervalSinceNow() as f32,
    };
    let uniforms_ptr = NonNull::from(&uniforms);

    unsafe {
        encoder.setVertexBytes_length_atIndex(
            uniforms_ptr.cast(),
            std::mem::size_of_val(&uniforms),
            0,
        );
    }

    encoder.setRenderPipelineState(&state.pipeline);
    encoder.setDepthStencilState(Some(&state.depth_stencil_state));

    //
    // Draw
    for mesh in &state.model.mesh {
        mesh.draw(&encoder);
    }

    encoder.endEncoding();
    command_buffer.presentDrawable(ProtocolObject::from_ref(&*drawable));
    command_buffer.commit();
}

fn main() {
    let mtm = MainThreadMarker::new().unwrap();
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    let delegate: Retained<Delegate> = unsafe {
        let this = Delegate::alloc(mtm).set_ivars(Ivars {
            state: RefCell::new(None),
        });
        msg_send![super(this), init]
    };
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
