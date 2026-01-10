#![deny(unsafe_op_in_unsafe_fn)]

mod camera;
mod input;
mod platform;
mod render;
mod resource;

// TODO: What?
use objc2::AnyThread;
use objc2::runtime::AnyObject;

use crate::camera::Camera;
use crate::input::Key;
use crate::platform::{Delegate, Ivars};
use crate::render::{Asset, Mesh, RenderPass, SinglePass, Uniforms};
use crate::resource::{Buffer, BufferKind, Device, ShaderLibrary};

use objc2::MainThreadOnly;

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{MainThreadMarker, msg_send};

use glam::{Mat4, Vec3};

use objc2_foundation::{
    NSDate, NSDictionary, NSNumber, NSPoint, NSRect, NSSize, NSString, NSUInteger, NSURL, ns_string,
};

use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSWindow, NSWindowStyleMask,
};

use objc2_metal::*;

use objc2_metal_kit::{MTKTextureLoader, MTKTextureLoaderOptionAllocateMipmaps, MTKView};

const WINDOW_W: f64 = 800.0;
const WINDOW_H: f64 = 600.0;

const GLTF_NAME: &str = "Sponza";

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

    // TODO: move to resource.rs
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
        String::from("./src/shaders/normals.metallib"),
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
    window.setTitle(ns_string!("fortnite_source_code_leaked"));
    window.makeKeyAndOrderFront(None);

    // Depth stencil
    let depth_stencil_descriptor = MTLDepthStencilDescriptor::new();
    depth_stencil_descriptor.setDepthCompareFunction(MTLCompareFunction::Less);
    depth_stencil_descriptor.setDepthWriteEnabled(true);
    let depth_stencil_state = device
        .newDepthStencilStateWithDescriptor(&depth_stencil_descriptor)
        .expect("Failed to create depth stencil state");

    let gltf_path = format!("./assets/{}/glTF/{}.gltf", GLTF_NAME, GLTF_NAME);
    let (document, buffers, images) = gltf::import(gltf_path).expect("could not import glTF");
    assert_eq!(buffers.len(), document.buffers().count());
    assert_eq!(images.len(), document.images().count());

    let mut all_meshes = Vec::new();

    let key = unsafe { MTKTextureLoaderOptionAllocateMipmaps };
    let value = NSNumber::numberWithBool(true);
    let options = NSDictionary::from_slices(&[key], &[&*value as &AnyObject]);

    let mipmap_command_buffer = command_queue
        .commandBuffer()
        .expect("Failed to create mipmap command buffer");
    let mipmap_blit_encoder = mipmap_command_buffer
        .blitCommandEncoder()
        .expect("Failed to create mipmap blit encoder");

    // FIXME: This is kind of horible
    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

            let positions: Vec<[f32; 3]> = reader.read_positions().expect("No positions").collect();

            let normals: Vec<[f32; 3]> = reader.read_normals().expect("No normals").collect();

            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .expect("no texture coordinates")
                .into_f32()
                .collect();

            let indices: Vec<u32> = reader
                .read_indices()
                .expect("No indices")
                .into_u32()
                .collect();

            let num_vertices = positions.len();
            let stride = std::mem::size_of::<[f32; 8]>();

            // allocate buffers
            // interleave all attributes into a single buffer
            let buffer = Buffer::new(
                &device,
                num_vertices,
                stride,
                MTLResourceOptions::StorageModeShared,
                BufferKind::POSITIONS,
            );

            // fill the buffer with data
            unsafe {
                let contents = buffer.buffer.contents().as_ptr() as *mut u8;
                for i in 0..num_vertices {
                    let offset = i * stride;

                    std::ptr::copy_nonoverlapping(
                        positions[i].as_ptr() as *const u8,
                        contents.add(offset + 0),
                        12,
                    );

                    std::ptr::copy_nonoverlapping(
                        normals[i].as_ptr() as *const u8,
                        contents.add(offset + 12),
                        12,
                    );

                    std::ptr::copy_nonoverlapping(
                        uvs[i].as_ptr() as *const u8,
                        contents.add(offset + 24),
                        8,
                    );
                }
            }

            // TODO: more generic buffer create?
            let index_buffer = device
                .newBufferWithLength_options(
                    (indices.len() * std::mem::size_of::<[i32; 3]>()) as NSUInteger,
                    MTLResourceOptions::StorageModeShared,
                )
                .expect("Failed to create index buffer");

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
                        let full_path = format!("./assets/{}/glTF/{}", GLTF_NAME, uri);
                        let path_to_tex = NSURL::fileURLWithPath(&NSString::from_str(&full_path));

                        let texture = unsafe {
                            mtk_tex_loader
                                .newTextureWithContentsOfURL_options_error(
                                    &path_to_tex,
                                    Some(&options),
                                )
                                .expect("Failed to load texture from file")
                        };

                        mipmap_blit_encoder.generateMipmapsForTexture(&texture);

                        Some(texture)
                    }
                    gltf::image::Source::View { .. } => None,
                }
            } else {
                None
            };

            let mut all_buffers = Vec::new();
            all_buffers.push(buffer);

            let model = Mat4::from_rotation_x(f32::to_radians(-15.0));

            let mut materials = Vec::new();
            materials.push(texture);

            let submesh = Mesh::new(
                all_buffers,
                index_buffer,
                materials,
                indices.len(),
                MTLPrimitiveType::Triangle,
                model,
            );

            all_meshes.push(submesh);
        }
    }

    mipmap_blit_encoder.endEncoding();
    mipmap_command_buffer.commit();

    // TODO: Move to resource module
    // A MTLVertexDescriptor has attributes and layouts
    let vertex_descriptor = MTLVertexDescriptor::new();

    // Attribute 0: position (float3) at offset 0 in buffer(1)
    unsafe {
        let pos_attr = vertex_descriptor.attributes().objectAtIndexedSubscript(0);
        pos_attr.setFormat(MTLVertexFormat::Float3);
        pos_attr.setOffset(0);
        pos_attr.setBufferIndex(1);

        let norm_attr = vertex_descriptor.attributes().objectAtIndexedSubscript(1);
        norm_attr.setFormat(MTLVertexFormat::Float3);
        norm_attr.setOffset(12);
        norm_attr.setBufferIndex(1);

        let uv_attr = vertex_descriptor.attributes().objectAtIndexedSubscript(2);
        uv_attr.setFormat(MTLVertexFormat::Float2);
        uv_attr.setOffset(24);
        uv_attr.setBufferIndex(1);
    }

    unsafe {
        let layout = vertex_descriptor.layouts().objectAtIndexedSubscript(1);
        layout.setStride(std::mem::size_of::<[f32; 8]>() as NSUInteger);
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

    let model = Mat4::ZERO;
    let uniforms = Uniforms {
        view_proj,
        time,
        model,
    };

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
