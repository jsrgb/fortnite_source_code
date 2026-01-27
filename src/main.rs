#![deny(unsafe_op_in_unsafe_fn)]

mod camera;
mod input;
mod platform;
mod render;
mod resource;
mod world;

// TODO: What?
use objc2::AnyThread;
use objc2::runtime::AnyObject;

use crate::camera::Camera;
use crate::input::Key;
use crate::platform::{Delegate, Ivars};
use crate::render::{Asset, Mesh, RenderPass, SinglePass, SkyboxPass, Uniforms, create_cube_mesh};
use crate::resource::{
    Buffer, BufferKind, Device, ShaderLibrary, VertexAttribute, VertexDescriptor,
};
use crate::world::{Skybox, World};

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
    // RefCell? In frame() an immutable reference to AppState is passed in.
    // But camera state needs to mutate when input is pressed
    // RefCell allows for mutable borrows at runtime, even when the data is immutable
    // Maybe move out of app state
    world: Box<World>,
    camera: RefCell<Camera>,
    passes: Vec<Box<dyn RenderPass>>,
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

            let model = Mat4::IDENTITY;

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
    let vertex_descriptor = VertexDescriptor::new(vec![
        VertexAttribute {
            format: MTLVertexFormat::Float3,
            offset: 0,
            index: 0,
            buffer_id: 1,
        },
        VertexAttribute {
            format: MTLVertexFormat::Float3,
            offset: 12,
            index: 1,
            buffer_id: 1,
        },
        VertexAttribute {
            format: MTLVertexFormat::Float2,
            offset: 24,
            index: 2,
            buffer_id: 1,
        },
    ]);

    // Attached vertex spec to pipeline
    pipeline_descriptor.setVertexDescriptor(Some(&vertex_descriptor));

    let pipeline_state = device
        .newRenderPipelineStateWithDescriptor_error(&pipeline_descriptor)
        .expect("Failed to create pipeline state");

    let cam_position = Vec3::new(0.0, 10.0, 0.0);
    let camera = Camera::new(
        cam_position,
        Vec3::new(0.0, 0.0, -1.0), // front, Looking at -Z
        Vec3::new(0.0, 1.0, 0.0),  // up
        -90.0,                     // yaw
        0.0,                       // pitch
    );

    let pass = SinglePass {
        pipeline: pipeline_state,
        depth_stencil_state,
    };

    // ===== Skybox initialization =====

    // Load skybox shader using existing helper
    let skybox_shader_lib = ShaderLibrary::new(
        String::from("Skybox shader library"),
        String::from("./src/shaders/skybox.metallib"),
        &device,
    );

    // Create skybox vertex descriptor (only position attribute)
    let skybox_vertex_descriptor = MTLVertexDescriptor::new();
    unsafe {
        let attr = skybox_vertex_descriptor
            .attributes()
            .objectAtIndexedSubscript(0);
        attr.setFormat(MTLVertexFormat::Float3);
        attr.setOffset(0);
        attr.setBufferIndex(1);

        let layout = skybox_vertex_descriptor
            .layouts()
            .objectAtIndexedSubscript(1);
        layout.setStride(12); // 3 floats * 4 bytes
        layout.setStepFunction(MTLVertexStepFunction::PerVertex);
        layout.setStepRate(1);
    }

    // Create skybox pipeline
    let skybox_pipeline_descriptor = MTLRenderPipelineDescriptor::new();
    unsafe {
        skybox_pipeline_descriptor
            .colorAttachments()
            .objectAtIndexedSubscript(0)
            .setPixelFormat(view.colorPixelFormat());
    }
    skybox_pipeline_descriptor.setVertexFunction(Some(&skybox_shader_lib.vertex));
    skybox_pipeline_descriptor.setFragmentFunction(Some(&skybox_shader_lib.fragment));
    skybox_pipeline_descriptor.setVertexDescriptor(Some(&skybox_vertex_descriptor));
    skybox_pipeline_descriptor.setDepthAttachmentPixelFormat(MTLPixelFormat::Depth32Float);

    let skybox_pipeline_state = device
        .newRenderPipelineStateWithDescriptor_error(&skybox_pipeline_descriptor)
        .expect("Failed to create skybox pipeline state");

    // Create skybox depth stencil state (depth write disabled)
    let skybox_depth_descriptor = MTLDepthStencilDescriptor::new();
    skybox_depth_descriptor.setDepthCompareFunction(MTLCompareFunction::LessEqual);
    skybox_depth_descriptor.setDepthWriteEnabled(false); // Don't write depth for skybox
    let skybox_depth_state = device
        .newDepthStencilStateWithDescriptor(&skybox_depth_descriptor)
        .expect("Failed to create skybox depth stencil state");

    // Load cube map textures
    let cube_texture = {
        let texture_descriptor = unsafe {
            MTLTextureDescriptor::textureCubeDescriptorWithPixelFormat_size_mipmapped(
                MTLPixelFormat::BGRA8Unorm,
                2048,
                false,
            )
        };
        texture_descriptor.setUsage(MTLTextureUsage::ShaderRead);
        texture_descriptor.setStorageMode(MTLStorageMode::Private);

        let cube_tex = device
            .newTextureWithDescriptor(&texture_descriptor)
            .expect("Failed to create cube texture");

        // Load each face
        let face_names = ["posx", "negx", "posy", "negy", "posz", "negz"];
        for (slice, name) in face_names.iter().enumerate() {
            let texture_path = format!("./assets/skybox/Maskonaive/{}.jpg", name);
            let path = NSString::from_str(&texture_path);
            let url = NSURL::fileURLWithPath(&path);

            let temp_texture = unsafe {
                mtk_tex_loader
                    .newTextureWithContentsOfURL_options_error(&url, None)
                    .expect(&format!("Failed to load skybox texture: {}", name))
            };

            // Copy to cube texture slice
            let blit_command_buffer = command_queue
                .commandBuffer()
                .expect("Failed to create blit command buffer");
            let blit_encoder = blit_command_buffer
                .blitCommandEncoder()
                .expect("Failed to create blit encoder");

            unsafe {
                blit_encoder.copyFromTexture_sourceSlice_sourceLevel_toTexture_destinationSlice_destinationLevel_sliceCount_levelCount(
                    &temp_texture,
                    0,
                    0,
                    &cube_tex,
                    slice,
                    0,
                    1,
                    1,
                );
            }

            blit_encoder.endEncoding();
            blit_command_buffer.commit();
            blit_command_buffer.waitUntilCompleted();
        }

        cube_tex
    };

    // Create cube mesh
    let cube_mesh = create_cube_mesh(&device);

    // Create skybox pass
    // let skybox = SkyboxPass {
    //     pipeline: skybox_pipeline_state,
    //     depth_stencil_state: skybox_depth_state,
    //     cube_mesh,
    //     cube_texture,
    // };

    // let skyb = Skybox {
    //     mesh: cube_mesh,
    //     texture: cube_texture,
    // };

    let world = World { meshes: all_meshes };

    // create
    let app_state = AppState {
        start_date: NSDate::now(),
        device: Device {
            device,
            command_queue,
        },
        world: Box::new(world),
        camera: RefCell::new(camera),
        passes: vec![Box::new(pass)],
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

    // Render skybox first (before scene, with depth write disabled)
    // Remove translation from view matrix so skybox stays centered on camera
    let (_, rotation, _) = view.to_scale_rotation_translation();
    let view_no_translation = Mat4::from_quat(rotation);
    let skybox_view_proj = projection * view_no_translation;
    //state.skybox.render(&encoder, skybox_view_proj);

    for pass in &state.passes {
        pass.render(&state.world, &camera, &encoder);
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
