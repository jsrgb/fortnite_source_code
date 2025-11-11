#![allow(irrefutable_let_patterns)]

use blade_graphics as gpu;

#[derive(blade_macros::ShaderData)]
struct DrawData {}

struct Example {
    command_encoder: gpu::CommandEncoder,
    prev_sync_point: Option<gpu::SyncPoint>,
    context: gpu::Context,
    surface: gpu::Surface,
    pipeline: gpu::RenderPipeline,
    window_size: winit::dpi::PhysicalSize<u32>,
}

impl Example {
    fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        self.window_size = size;
        let config = Self::make_surface_config(size);
        self.context.reconfigure_surface(&mut self.surface, config);
    }

    fn make_surface_config(size: winit::dpi::PhysicalSize<u32>) -> gpu::SurfaceConfig {
        gpu::SurfaceConfig {
            size: gpu::Extent {
                width: size.width,
                height: size.height,
                depth: 1,
            },
            usage: gpu::TextureUsage::TARGET,
            display_sync: gpu::DisplaySync::Block,
            ..Default::default()
        }
    }

    fn new(window: &winit::window::Window) -> Self {
        let window_size = window.inner_size();
        let context = unsafe {
            gpu::Context::init(gpu::ContextDesc {
                presentation: true,
                validation: cfg!(debug_assertions),
                timing: true,
                capture: false,

                ..Default::default()
            })
            .unwrap()
        };
        let surface = context
            .create_surface_configured(window, Self::make_surface_config(window_size))
            .unwrap();

        let mut command_encoder = context.create_command_encoder(gpu::CommandEncoderDesc {
            name: "main",
            buffer_count: 2,
        });
        command_encoder.start();
        let sync_point = context.submit(&mut command_encoder);

        let source = std::fs::read_to_string("src/shaders/triangle.wgsl").unwrap();
        let shader = context.create_shader(gpu::ShaderDesc { source: &source });
        let draw_layout = <DrawData as gpu::ShaderData>::layout();

        let pipeline = context.create_render_pipeline(gpu::RenderPipelineDesc {
            name: "main",
            data_layouts: &[&draw_layout],
            primitive: gpu::PrimitiveState {
                topology: gpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            vertex: shader.at("vs_main"),
            vertex_fetches: &[],
            fragment: Some(shader.at("fs_main")),
            color_targets: &[surface.info().format.into()],
            depth_stencil: None,
            multisample_state: Default::default(),
        });

        Self {
            command_encoder,
            prev_sync_point: Some(sync_point),
            context,
            surface,
            pipeline,
            window_size,
        }
    }

    fn destroy(&mut self) {
        if let Some(sp) = self.prev_sync_point.take() {
            self.context.wait_for(&sp, !0);
        }
        self.context
            .destroy_command_encoder(&mut self.command_encoder);
        self.context.destroy_surface(&mut self.surface);
    }

    fn render(&mut self) {
        self.command_encoder.start();

        let frame = self.surface.acquire_frame();

        self.command_encoder.init_texture(frame.texture());

        if let mut pass = self.command_encoder.render(
            "triangle",
            gpu::RenderTargetSet {
                colors: &[gpu::RenderTarget {
                    view: frame.texture_view(),
                    init_op: gpu::InitOp::Clear(gpu::TextureColor::OpaqueBlack),
                    finish_op: gpu::FinishOp::Store, // TODO: What
                }],
                depth_stencil: None,
            },
        ) {
            if let mut pc = pass.with(&self.pipeline) {
                // first vertex, vertex count, first index, index count
                pc.draw(0, 3, 0, 1);
            }
        }
        self.command_encoder.present(frame);

        // TODO: what happens if i remove this
        let sync_point = self.context.submit(&mut self.command_encoder);

        if let Some(prev) = self.prev_sync_point.take() {
            self.context.wait_for(&prev, !0);
        }
        self.prev_sync_point = Some(sync_point);
    }
}

fn main() {
    env_logger::init();

    let event_loop = winit::event_loop::EventLoop::new().unwrap();
    let window_attributes =
        winit::window::Window::default_attributes().with_title("blade triangle");

    let window = event_loop.create_window(window_attributes).unwrap();

    let mut example = Example::new(&window);

    event_loop
        .run(|event, target| {
            target.set_control_flow(winit::event_loop::ControlFlow::Poll);

            match event {
                winit::event::Event::AboutToWait => {
                    window.request_redraw();
                }

                winit::event::Event::WindowEvent { event, .. } => match event {
                    winit::event::WindowEvent::Resized(size) => {
                        example.resize(size);
                    }
                    winit::event::WindowEvent::KeyboardInput {
                        event:
                            winit::event::KeyEvent {
                                physical_key: winit::keyboard::PhysicalKey::Code(key_code),
                                state: winit::event::ElementState::Pressed,
                                ..
                            },
                        ..
                    } => match key_code {
                        winit::keyboard::KeyCode::Escape => {
                            target.exit();
                        }
                        _ => {}
                    },

                    winit::event::WindowEvent::CloseRequested => {
                        target.exit();
                    }

                    winit::event::WindowEvent::RedrawRequested => {
                        example.render();
                    }

                    _ => {}
                },

                _ => {}
            }
        })
        .unwrap();

    example.destroy();
}
