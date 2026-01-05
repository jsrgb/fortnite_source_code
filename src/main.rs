#![deny(unsafe_op_in_unsafe_fn)]

mod input;
mod platform;
mod render;
mod resource;

// TODO: What?
use objc2::AnyThread;

use crate::input::Key;
use crate::platform::{Delegate, Ivars};
use crate::render::{RenderPass, SinglePass, Uniforms};
use crate::resource::{Asset, Buffer, BufferKind, Device, Mesh, ShaderLibrary};

use objc2::MainThreadOnly;

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{MainThreadMarker, msg_send};

use glam::{Mat4, Vec3};

use objc2_foundation::{NSDate, NSPoint, NSRect, NSSize, NSString, NSUInteger, NSURL, ns_string};

// TODO: Move and improve
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSWindow, NSWindowStyleMask,
};

use objc2_metal::*;

use objc2_metal_kit::{MTKTextureLoader, MTKView};

// TODO: camera.rs?
struct Camera {
    position: Vec3,
    target: Vec3,
    direction: Vec3,
    front: Vec3,
    up: Vec3,
    yaw: f32,
    pitch: f32,
}

impl Camera {
    fn new(
        position: Vec3,
        target: Vec3,
        direction: Vec3,
        front: Vec3,
        up: Vec3,
        yaw: f32,
        pitch: f32,
    ) -> Self {
        Self {
            position,
            target,
            direction,
            front,
            up,
            yaw,
            pitch,
        }
    }
}

const WINDOW_W: f64 = 800.0;
const WINDOW_H: f64 = 600.0;

pub struct AppState {
    start_date: Retained<NSDate>,
    pub device: Device,
    model: Asset,
    // RefCell? In frame() an immutable reference to AppState is passed in.
    // But camera state needs to mutate when input is pressed
    // RefCell allows for mutable borrows at runtime, even when the data is immutable
    // Maybe move out of app state
    camera: RefCell<Camera>,
    pass: SinglePass,
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

    let shader_lib = ShaderLibrary::new(
        String::from("Single pass shader library"),
        String::from("./src/shaders/pos_uv.metallib"),
        &device,
    );
    pipeline_descriptor.setVertexFunction(Some(shader_lib.vertex.as_ref()));
    pipeline_descriptor.setFragmentFunction(Some(shader_lib.fragment.as_ref()));
    // Add depth stencil attachment
    pipeline_descriptor.setDepthAttachmentPixelFormat(MTLPixelFormat::Depth32Float);

    view.setClearColor(MTLClearColor {
        red: 0.2,
        green: 0.2,
        blue: 0.8,
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

    let (document, buffers, images) =
        gltf::import("./src/assets/Sponza.gltf").expect("could not open gltf");
    assert_eq!(buffers.len(), document.buffers().count());
    assert_eq!(images.len(), document.images().count());

    let mut all_meshes = Vec::new();

    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

            let positions: Vec<[f32; 3]> = reader.read_positions().expect("No positions").collect();

            let tex_coords: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .expect("no texture coordinates")
                .into_f32()
                .collect();

            let indices: Vec<u32> = reader
                .read_indices()
                .expect("No indices")
                .into_u32()
                .collect();

            // allocate buffers
            let position_buffer = Buffer::new(
                &device,
                positions.len(),
                std::mem::size_of::<[f32; 3]>(),
                MTLResourceOptions::StorageModeShared,
                BufferKind::POSITIONS,
            );

            let uv_buffer = Buffer::new(
                &device,
                tex_coords.len(),
                std::mem::size_of::<[f32; 2]>(),
                MTLResourceOptions::StorageModeShared,
                BufferKind::UV,
            );

            // TODO: more generic buffer create?
            let index_buffer = device
                .newBufferWithLength_options(
                    (indices.len() * std::mem::size_of::<[i32; 3]>()) as NSUInteger,
                    MTLResourceOptions::StorageModeShared,
                )
                .expect("Failed to create index buffer");

            // fill them
            unsafe {
                let contents = position_buffer.buffer.contents().as_ptr() as *mut f32;
                std::ptr::copy_nonoverlapping(
                    positions.as_ptr() as *const f32,
                    contents,
                    positions.len() * 3,
                );
            }

            unsafe {
                let contents = uv_buffer.buffer.contents().as_ptr() as *mut f32;
                std::ptr::copy_nonoverlapping(
                    tex_coords.as_ptr() as *const f32,
                    contents,
                    tex_coords.len() * 2,
                );
            }

            unsafe {
                let contents = index_buffer.contents().as_ptr() as *mut u32;
                std::ptr::copy_nonoverlapping(indices.as_ptr(), contents, indices.len());
            }

            let material = primitive.material();

            let texture = if let Some(tex) = material.pbr_metallic_roughness().base_color_texture()
            {
                let image = tex.texture().source();

                match image.source() {
                    gltf::image::Source::Uri { uri, .. } => {
                        let full_path = format!("./src/assets/{}", uri);
                        let path_to_tex = NSURL::fileURLWithPath(&NSString::from_str(&full_path));

                        Some(unsafe {
                            mtk_tex_loader
                                .newTextureWithContentsOfURL_options_error(&path_to_tex, None)
                                .expect("Failed to load texture from file")
                        })
                    }
                    gltf::image::Source::View { .. } => None,
                }
            } else {
                None
            };

            let mut all_buffers = Vec::new();
            all_buffers.push(position_buffer);
            all_buffers.push(uv_buffer);

            let submesh = Mesh::new(
                all_buffers,
                index_buffer,
                texture, // TODO: List of materials
                indices.len(),
                MTLPrimitiveType::Triangle,
            );

            all_meshes.push(submesh);
        }
    }

    // TODO: Move to resource module
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
    // POS
    unsafe {
        let layout = vertex_descriptor.layouts().objectAtIndexedSubscript(1);
        layout.setStride(std::mem::size_of::<[f32; 3]>() as NSUInteger); // 12
        layout.setStepFunction(MTLVertexStepFunction::PerVertex);
        layout.setStepRate(1);
    }

    // UV
    unsafe {
        let a1 = vertex_descriptor.attributes().objectAtIndexedSubscript(1);
        a1.setFormat(MTLVertexFormat::Float2);
        a1.setOffset(0);
        a1.setBufferIndex(2);
    }

    unsafe {
        let layout = vertex_descriptor.layouts().objectAtIndexedSubscript(2);
        layout.setStride(std::mem::size_of::<[f32; 2]>() as NSUInteger);
        layout.setStepFunction(MTLVertexStepFunction::PerVertex);
        layout.setStepRate(1);
    }

    // Attached vertex spec to pipeline
    pipeline_descriptor.setVertexDescriptor(Some(&vertex_descriptor));

    let pipeline_state = device
        .newRenderPipelineStateWithDescriptor_error(&pipeline_descriptor)
        .expect("Failed to create pipeline state");

    let cam_position = Vec3::new(0.0, 10.0, 0.0);
    let cam_target = Vec3::new(0.0, 0.0, 0.0);
    let camera = Camera::new(
        cam_position,
        cam_target,
        Vec3::normalize(cam_position - cam_target), // direction
        Vec3::new(0.0, 0.0, -1.0),                  // front, Looking at -Z
        Vec3::new(0.0, 1.0, 0.0),                   // up
        -90.0,                                      // yaw
        0.0,                                        // pitch
    );

    let pass = SinglePass::new(pipeline_state, depth_stencil_state);

    let app_state = AppState {
        start_date: NSDate::now(),
        device: Device {
            device,
            command_queue,
        },
        model: Asset {
            meshes: all_meshes,
            name: "Box".to_string(),
        },
        camera: RefCell::new(camera),
        pass,
    };
    (app_state, window, view)
}

pub fn frame(view: &MTKView, state: &AppState) {
    let mut camera = state.camera.borrow_mut();

    let move_speed = 4.0;

    let direction = Vec3::new(
        f32::cos(f32::to_radians(camera.yaw)) * f32::cos(f32::to_radians(camera.pitch)),
        f32::sin(f32::to_radians(camera.pitch)),
        f32::sin(f32::to_radians(camera.yaw)) * f32::cos(f32::to_radians(camera.pitch)),
    );
    let front = direction.normalize();
    camera.front = front;
    let right = front.cross(camera.up).normalize();
    let up = camera.up;

    // TODO: add a tiny event queue? :)
    //
    if Key::W.is_pressed() {
        camera.position += front * move_speed;
    }
    if Key::S.is_pressed() {
        camera.position -= front * move_speed;
    }
    if Key::A.is_pressed() {
        camera.position -= right * move_speed;
    }
    if Key::D.is_pressed() {
        camera.position += right * move_speed;
    }
    if Key::SPC.is_pressed() {
        camera.position += up * move_speed;
    }
    if Key::C.is_pressed() {
        camera.position -= up * move_speed;
    }

    let yaw_sens: f32 = 7.0;
    let pitch_sens: f32 = 7.0;
    if Key::Q.is_pressed() {
        camera.yaw -= yaw_sens;
    }
    if Key::E.is_pressed() {
        camera.yaw += yaw_sens;
    }

    if Key::F.is_pressed() {
        camera.pitch -= pitch_sens;
    }
    if Key::R.is_pressed() {
        camera.pitch += pitch_sens;
    }

    if camera.pitch > 89.0 {
        camera.pitch = 89.0;
    }
    if camera.pitch < -89.0 {
        camera.pitch = -89.0;
    }

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
        f32::to_radians(60.0),
        aspect_ratio,
        0.025,  // near plane
        8000.0, // far plane
    );

    // Update camera uniform
    let view = Mat4::look_at_rh(camera.position, camera.position + camera.front, camera.up);
    let view_proj = projection * view;
    let time = state.start_date.timeIntervalSinceNow() as f32;

    let uniforms = Uniforms { view_proj, time };

    state.pass.render(&encoder, &uniforms, &state.model, time);

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
