#![deny(unsafe_op_in_unsafe_fn)]

use objc2::MainThreadOnly;
use std::ffi::c_void;
use std::ptr::NonNull;

use objc2::DefinedClass;

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, MainThreadMarker};

use glam::{Mat4, Vec2, Vec3, Vec4};

use objc2_foundation::{
    ns_string, NSDate, NSInteger, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect,
    NSSize, NSUInteger, NSURL,
};

use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
    NSWindow, NSWindowStyleMask,
};

use objc2_metal::{
    MTLBuffer, MTLCPUCacheMode, MTLClearColor, MTLCommandBuffer, MTLCommandEncoder,
    MTLCommandQueue, MTLCompareFunction, MTLCreateSystemDefaultDevice, MTLDepthStencilDescriptor,
    MTLDepthStencilState, MTLDevice, MTLHeap, MTLHeapDescriptor, MTLIndexType, MTLLibrary,
    MTLPackedFloat3, MTLPixelFormat, MTLPrimitiveType, MTLRenderCommandEncoder,
    MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLResourceOptions, MTLStorageMode,
    MTLVertexDescriptor, MTLVertexFormat, MTLVertexStepFunction,
};

use objc2_metal_kit::{MTKView, MTKViewDelegate};

#[derive(Copy, Clone)]
#[repr(C)]
struct Uniforms {
    view_proj: Mat4,
    time: f32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct CubeVertex {
    position: MTLPackedFloat3,
    color: MTLPackedFloat3,
}

const WINDOW_W: f64 = 800.0;
const WINDOW_H: f64 = 600.0;

// FIXME: remove
const CUBE_VERTS: [CubeVertex; 8] = [
    // Front face
    CubeVertex {
        position: MTLPackedFloat3 {
            x: -0.5,
            y: -0.5,
            z: 0.5,
        },
        color: MTLPackedFloat3 {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        },
    },
    CubeVertex {
        position: MTLPackedFloat3 {
            x: 0.5,
            y: -0.5,
            z: 0.5,
        },
        color: MTLPackedFloat3 {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        },
    },
    CubeVertex {
        position: MTLPackedFloat3 {
            x: 0.5,
            y: 0.5,
            z: 0.5,
        },
        color: MTLPackedFloat3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        },
    },
    CubeVertex {
        position: MTLPackedFloat3 {
            x: -0.5,
            y: 0.5,
            z: 0.5,
        },
        color: MTLPackedFloat3 {
            x: 1.0,
            y: 1.0,
            z: 0.0,
        },
    },
    // Back face
    CubeVertex {
        position: MTLPackedFloat3 {
            x: -0.5,
            y: -0.5,
            z: -0.5,
        },
        color: MTLPackedFloat3 {
            x: 1.0,
            y: 0.0,
            z: 1.0,
        },
    },
    CubeVertex {
        position: MTLPackedFloat3 {
            x: 0.5,
            y: -0.5,
            z: -0.5,
        },
        color: MTLPackedFloat3 {
            x: 0.0,
            y: 1.0,
            z: 1.0,
        },
    },
    CubeVertex {
        position: MTLPackedFloat3 {
            x: 0.5,
            y: 0.5,
            z: -0.5,
        },
        color: MTLPackedFloat3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
    },
    CubeVertex {
        position: MTLPackedFloat3 {
            x: -0.5,
            y: 0.5,
            z: -0.5,
        },
        color: MTLPackedFloat3 {
            x: 0.2,
            y: 0.2,
            z: 0.2,
        },
    },
];

const CUBE_INDICES: [u16; 36] = [
    // Front
    0, 1, 2, 2, 3, 0, // Right
    1, 5, 6, 6, 2, 1, // Back
    5, 4, 7, 7, 6, 5, // Left
    4, 0, 3, 3, 7, 4, // Top
    3, 2, 6, 6, 7, 3, // Bottom
    4, 5, 1, 1, 0, 4,
];

struct GpuDevice {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
}

struct AppState {
    start_date: Retained<NSDate>,
    device: GpuDevice,
    pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    depth_stencil_state: Retained<ProtocolObject<dyn MTLDepthStencilState>>, // FIXME: move
    vbuf: Retained<ProtocolObject<dyn MTLBuffer>>,
    ibuf: Retained<ProtocolObject<dyn MTLBuffer>>,
}

struct Ivars {
    state: RefCell<Option<AppState>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = objc2::MainThreadOnly]
    #[ivars = Ivars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        // https://developer.apple.com/documentation/appkit/nsapplicationdelegate/applicationdidfinishlaunching(_:)?language=objc
        unsafe fn init(&self, _notification: &NSNotification) {
            // FIXME: innacurate fn name i think
            let mtm = MainThreadMarker::new().unwrap();

            let window = {
                let content_rect =
                    NSRect::new(NSPoint::new(0., 0.), NSSize::new(WINDOW_W, WINDOW_H));
                let style = NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Resizable
                    | NSWindowStyleMask::Titled;

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

            // Attribute 1: color (float3) at offset 12 in buffer(1)
            unsafe {
                let a1 = vertex_descriptor.attributes().objectAtIndexedSubscript(1);
                a1.setFormat(MTLVertexFormat::Float3);
                a1.setOffset(12);
                a1.setBufferIndex(1);
            }

            // layouts describe how to fetch (stride,offset)
            // Layout for buffer(1): stride = 24 bytes
            unsafe {
                let layout = vertex_descriptor.layouts().objectAtIndexedSubscript(1);
                layout.setStride(std::mem::size_of::<CubeVertex>() as NSUInteger); // 24
                layout.setStepFunction(MTLVertexStepFunction::PerVertex);
                layout.setStepRate(1);
            }

            // Attached vertex spec to pipeline
            pipeline_descriptor.setVertexDescriptor(Some(&vertex_descriptor));

            let pipeline_state = device
                .newRenderPipelineStateWithDescriptor_error(&pipeline_descriptor)
                .expect("Failed to create pipeline state");

            view.setDelegate(Some(ProtocolObject::from_ref(self)));
            view.setClearColor(MTLClearColor {
                red: 0.0,
                green: 0.5,
                blue: 1.0,
                alpha: 1.0,
            });

            window.setContentView(Some(&view));
            window.center();
            window.setTitle(ns_string!("triangle"));
            window.makeKeyAndOrderFront(None);

            // Depth stencil
            let depth_stencil_descriptor = MTLDepthStencilDescriptor::new();
            depth_stencil_descriptor.setDepthCompareFunction(MTLCompareFunction::Less);
            depth_stencil_descriptor.setDepthWriteEnabled(true);
            let depth_stencil_state = device
                .newDepthStencilStateWithDescriptor(&depth_stencil_descriptor)
                .expect("Failed to create depth stencil state");

            // FIXME:
            let vbytes = (std::mem::size_of::<CubeVertex>() * CUBE_VERTS.len()) as NSUInteger;
            let ibytes = (std::mem::size_of::<u16>() * CUBE_INDICES.len()) as NSUInteger;
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

            let vbuf = heap
                .newBufferWithLength_options(vbytes, MTLResourceOptions::StorageModeShared)
                .expect("Failed to create  buffer");
            let ibuf = heap
                .newBufferWithLength_options(ibytes, MTLResourceOptions::StorageModeShared)
                .expect("Failed to create buffer");

            unsafe {
                std::ptr::copy_nonoverlapping(
                    CUBE_VERTS.as_ptr() as *const u8,
                    vbuf.contents().as_ptr() as *mut u8,
                    vbytes as usize,
                );
            }

            unsafe {
                std::ptr::copy_nonoverlapping(
                    CUBE_INDICES.as_ptr() as *const u8,
                    ibuf.contents().as_ptr() as *mut u8,
                    ibytes as usize,
                );
            }

            *self.ivars().state.borrow_mut() = Some(AppState {
                start_date: NSDate::now(),
                device: GpuDevice {
                    device,
                    command_queue,
                },
                vbuf,
                ibuf,
                depth_stencil_state,
                pipeline: pipeline_state,
            });
        }

        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        // https://developer.apple.com/documentation/appkit/nsapplicationdelegate/applicationshouldterminateafterlastwindowclosed(_:)?language=objc
        fn on_window_close(&self, _: &NSApplication) -> bool {
            true
        }
    }

    unsafe impl MTKViewDelegate for Delegate {
        #[unsafe(method(drawInMTKView:))]
        // https://developer.apple.com/documentation/metalkit/mtkview/draw()?language=objc
        unsafe fn draw(&self, mtk_view: &MTKView) {
            let borrow = self.ivars().state.borrow();
            let Some(state) = borrow.as_ref() else {
                return;
            };

            let Some(drawable) = mtk_view.currentDrawable() else {
                return;
            };
            let Some(command_buffer) = state.device.command_queue.commandBuffer() else {
                return;
            };
            let Some(pass_desc) = mtk_view.currentRenderPassDescriptor() else {
                return;
            };
            let Some(encoder) = command_buffer.renderCommandEncoderWithDescriptor(&pass_desc)
            else {
                return;
            };

            let aspect_ratio = WINDOW_W as f32 / WINDOW_H as f32;
            let projection = glam::Mat4::perspective_rh(
                45.0_f32.to_radians(),
                aspect_ratio,
                0.025, // near plane
                500.0, // far plane
            );
            let view = Mat4::look_at_rh(
                Vec3::new(1.0, 2.0, -1.0), // camera pos
                Vec3::new(0.0, 0.0, 0.0),  // center
                Vec3::new(0.0, 1.0, 0.0),  // up vector
            );

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

            unsafe {
                encoder.setVertexBuffer_offset_atIndex(Some(&state.vbuf), 0, 1);
            }

            encoder.setRenderPipelineState(&state.pipeline);
            encoder.setDepthStencilState(Some(&state.depth_stencil_state));
            unsafe {
                encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                    MTLPrimitiveType::Triangle,
                    CUBE_INDICES.len() as NSUInteger,
                    MTLIndexType::UInt16,
                    &state.ibuf,
                    0,
                );
            }
            encoder.endEncoding();
            command_buffer.presentDrawable(ProtocolObject::from_ref(&*drawable));
            command_buffer.commit();
        }

        #[unsafe(method(mtkView:drawableSizeWillChange:))]
        //https://developer.apple.com/documentation/metalkit/mtkviewdelegate/mtkview(_:drawablesizewillchange:)?language=objc
        unsafe fn update_view_on_resize(&self, _view: &MTKView, _size: NSSize) {}
    }
);

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
