use porter_math::Matrix4x4;
use wgpu::util::*;
use wgpu::*;

use porter_gpu::GPUInstance;
use porter_gpu::gpu_instance;

use porter_utils::SliceExt;

/// The offscreen color format frames are rendered into.
pub(crate) const COLOR_FORMAT: TextureFormat = TextureFormat::Rgba8Unorm;
/// The depth buffer format.
pub(crate) const DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;
/// The MSAA sample count.
pub(crate) const SAMPLE_COUNT: u32 = 4;

/// A mesh vertex as uploaded to the gpu.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub bone_ids: [u16; 4],
    pub bone_weights: [f32; 4],
    pub color: [u8; 4],
    pub bone_ids_2: [u16; 4],
    pub bone_weights_2: [f32; 4],
}

/// A line vertex as uploaded to the gpu.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct LineVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

/// A vertex of the shared unit sphere used for joint markers. The position is
/// also the surface normal (unit sphere), and the per-instance center/scale is
/// applied in the shader.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct JointVertex {
    pub position: [f32; 3],
}

/// A joint marker instance: the world-space bone position the sphere sits on.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct JointInstance {
    pub center: [f32; 3],
}

/// An image quad vertex as uploaded to the gpu.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct ImageVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

/// Shared gpu state for the viewport: shader, bind group layouts, and the
/// render pipelines. Created once per renderer; pipelines are never rebuilt.
pub(crate) struct ViewportContext {
    pub instance: &'static GPUInstance,
    pub camera_layout: BindGroupLayout,
    pub texture_layout: BindGroupLayout,
    pub bones_layout: BindGroupLayout,
    pub identity_instance: Buffer,
    pub mesh_pipelines: [RenderPipeline; 12],
    pub mesh_pipeline_wireframe: RenderPipeline,
    pub grid_pipeline: RenderPipeline,
    pub bone_pipeline: RenderPipeline,
    pub joint_pipeline: RenderPipeline,
    /// Shared unit sphere drawn once per bone via instancing.
    pub joint_sphere: Buffer,
    pub joint_sphere_vertices: u32,
    pub image_pipeline: RenderPipeline,
}

impl ViewportContext {
    /// Creates the shared gpu state.
    pub fn new() -> Self {
        let instance = gpu_instance();
        let device = instance.device();

        let shader = device.create_shader_module(include_wgsl!("shaders/viewport.wgsl"));

        let camera_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("tessera-viewport camera"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let texture_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("tessera-viewport texture"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let bones_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("tessera-viewport bones"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let mesh_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[
                Some(&camera_layout),
                Some(&texture_layout),
                Some(&bones_layout),
            ],
            immediate_size: 0,
        });

        let line_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&camera_layout)],
            immediate_size: 0,
        });

        let image_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&camera_layout), Some(&texture_layout)],
            immediate_size: 0,
        });

        let mesh_vertex_layout = VertexBufferLayout {
            array_stride: size_of::<MeshVertex>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: VertexFormat::Float32x2,
                },
                VertexAttribute {
                    offset: 32,
                    shader_location: 3,
                    format: VertexFormat::Uint16x4,
                },
                VertexAttribute {
                    offset: 40,
                    shader_location: 4,
                    format: VertexFormat::Float32x4,
                },
                VertexAttribute {
                    offset: 56,
                    shader_location: 5,
                    format: VertexFormat::Unorm8x4,
                },
                VertexAttribute {
                    offset: std::mem::offset_of!(MeshVertex, bone_ids_2) as BufferAddress,
                    shader_location: 10,
                    format: VertexFormat::Uint16x4,
                },
                VertexAttribute {
                    offset: std::mem::offset_of!(MeshVertex, bone_weights_2) as BufferAddress,
                    shader_location: 11,
                    format: VertexFormat::Float32x4,
                },
            ],
        };

        // Per-instance placement matrix, one column per attribute.
        let mesh_instance_layout = VertexBufferLayout {
            array_stride: size_of::<Matrix4x4>() as BufferAddress,
            step_mode: VertexStepMode::Instance,
            attributes: &[
                VertexAttribute {
                    offset: 0,
                    shader_location: 6,
                    format: VertexFormat::Float32x4,
                },
                VertexAttribute {
                    offset: 16,
                    shader_location: 7,
                    format: VertexFormat::Float32x4,
                },
                VertexAttribute {
                    offset: 32,
                    shader_location: 8,
                    format: VertexFormat::Float32x4,
                },
                VertexAttribute {
                    offset: 48,
                    shader_location: 9,
                    format: VertexFormat::Float32x4,
                },
            ],
        };

        let mesh_vertex_buffers = [Some(mesh_vertex_layout), Some(mesh_instance_layout)];

        let opaque_targets = [Some(ColorTargetState {
            format: COLOR_FORMAT,
            blend: Some(BlendState::REPLACE),
            write_mask: ColorWrites::ALL,
        })];
        let translucent_targets = [Some(ColorTargetState {
            format: COLOR_FORMAT,
            blend: Some(BlendState::ALPHA_BLENDING),
            write_mask: ColorWrites::ALL,
        })];
        let additive_targets = [Some(ColorTargetState {
            format: COLOR_FORMAT,
            blend: Some(BlendState {
                color: BlendComponent {
                    src_factor: BlendFactor::One,
                    dst_factor: BlendFactor::One,
                    operation: BlendOperation::Add,
                },
                alpha: BlendComponent {
                    src_factor: BlendFactor::One,
                    dst_factor: BlendFactor::One,
                    operation: BlendOperation::Add,
                },
            }),
            write_mask: ColorWrites::ALL,
        })];
        let mesh_pipeline_desc = |fragment: &'static str,
                                  cull: Option<Face>,
                                  polygon_mode: PolygonMode,
                                  blend: usize,
                                  clockwise: bool| {
            RenderPipelineDescriptor {
                label: None,
                layout: Some(&mesh_layout),
                vertex: VertexState {
                    module: &shader,
                    entry_point: Some("vs_mesh"),
                    buffers: &mesh_vertex_buffers,
                    compilation_options: Default::default(),
                },
                primitive: PrimitiveState {
                    topology: PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: if clockwise {
                        FrontFace::Cw
                    } else {
                        FrontFace::Ccw
                    },
                    cull_mode: cull,
                    unclipped_depth: false,
                    polygon_mode,
                    conservative: false,
                },
                depth_stencil: Some(DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(blend == 0),
                    depth_compare: Some(CompareFunction::Less),
                    stencil: StencilState::default(),
                    bias: DepthBiasState::default(),
                }),
                multisample: MultisampleState {
                    count: SAMPLE_COUNT,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                fragment: Some(FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    targets: match blend {
                        1 => &translucent_targets,
                        2 => &additive_targets,
                        _ => &opaque_targets,
                    },
                    compilation_options: Default::default(),
                }),
                multiview_mask: None,
                cache: None,
            }
        };

        let mesh_pipelines = std::array::from_fn(|index| {
            let two_sided = (index / 3) % 2 != 0;
            device.create_render_pipeline(&mesh_pipeline_desc(
                if two_sided {
                    "fs_mesh_two_sided"
                } else {
                    "fs_mesh"
                },
                if two_sided { None } else { Some(Face::Back) },
                PolygonMode::Fill,
                index % 3,
                index < 6,
            ))
        });
        let mesh_pipeline_wireframe = device.create_render_pipeline(&mesh_pipeline_desc(
            "fs_mesh_two_sided",
            None,
            PolygonMode::Line,
            0,
            true,
        ));

        let line_vertex_layout = VertexBufferLayout {
            array_stride: size_of::<LineVertex>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: VertexFormat::Float32x4,
                },
            ],
        };

        let line_vertex_buffers = [Some(line_vertex_layout)];
        let line_pipeline_desc =
            |vertex: &'static str,
             fragment: &'static str,
             depth_compare: CompareFunction,
             depth_write_enabled: bool| RenderPipelineDescriptor {
                label: None,
                layout: Some(&line_layout),
                vertex: VertexState {
                    module: &shader,
                    entry_point: Some(vertex),
                    buffers: &line_vertex_buffers,
                    compilation_options: Default::default(),
                },
                primitive: PrimitiveState {
                    topology: PrimitiveTopology::LineList,
                    strip_index_format: None,
                    front_face: FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: PolygonMode::Line,
                    conservative: false,
                },
                depth_stencil: Some(DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write_enabled),
                    depth_compare: Some(depth_compare),
                    stencil: StencilState::default(),
                    bias: DepthBiasState::default(),
                }),
                multisample: MultisampleState {
                    count: SAMPLE_COUNT,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                fragment: Some(FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    targets: &[Some(ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: Some(BlendState::ALPHA_BLENDING),
                        write_mask: ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                multiview_mask: None,
                cache: None,
            };

        // The infinite grid is a fullscreen triangle (no vertex buffers) that
        // ray-casts the ground plane in the fragment shader.
        let grid_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: None,
            layout: Some(&line_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_grid"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: DEPTH_FORMAT,
                // The reference grid must not hide asset geometry crossing its plane.
                depth_write_enabled: Some(false),
                depth_compare: Some(CompareFunction::Less),
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState {
                count: SAMPLE_COUNT,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_grid"),
                targets: &[Some(ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

        // Bones draw over the mesh (depth test always passes).
        let bone_pipeline = device.create_render_pipeline(&line_pipeline_desc(
            "vs_line_model",
            "fs_line",
            CompareFunction::Always,
            false,
        ));

        // Joint markers: solid spheres drawn over the mesh. Back-face culling
        // leaves only the front hemisphere, so there is no self-overlap and
        // depth-always keeps them visible through the model.
        let joint_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: None,
            layout: Some(&line_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_joint"),
                buffers: &[
                    // Per-vertex: the shared unit sphere.
                    Some(VertexBufferLayout {
                        array_stride: size_of::<JointVertex>() as BufferAddress,
                        step_mode: VertexStepMode::Vertex,
                        attributes: &[VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: VertexFormat::Float32x3,
                        }],
                    }),
                    // Per-instance: the bone position the sphere sits on.
                    Some(VertexBufferLayout {
                        array_stride: size_of::<JointInstance>() as BufferAddress,
                        step_mode: VertexStepMode::Instance,
                        attributes: &[VertexAttribute {
                            offset: 0,
                            shader_location: 1,
                            format: VertexFormat::Float32x3,
                        }],
                    }),
                ],
                compilation_options: Default::default(),
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: Some(Face::Back),
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(CompareFunction::Always),
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState {
                count: SAMPLE_COUNT,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_joint"),
                targets: &[Some(ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

        let image_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: None,
            layout: Some(&image_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_image"),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: size_of::<ImageVertex>() as BufferAddress,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: VertexFormat::Float32x3,
                        },
                        VertexAttribute {
                            offset: 12,
                            shader_location: 1,
                            format: VertexFormat::Float32x2,
                        },
                    ],
                })],
                compilation_options: Default::default(),
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(CompareFunction::Always),
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState {
                count: SAMPLE_COUNT,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_image"),
                targets: &[Some(ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

        let identity_instance = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("tessera-viewport identity instance"),
            contents: [Matrix4x4::new()].as_slice().as_this_slice(),
            usage: BufferUsages::VERTEX,
        });

        let sphere = crate::lines::unit_sphere();

        let joint_sphere = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("tessera-viewport joint sphere"),
            contents: sphere.as_slice().as_this_slice(),
            usage: BufferUsages::VERTEX,
        });

        Self {
            instance,
            camera_layout,
            texture_layout,
            bones_layout,
            identity_instance,
            mesh_pipelines,
            mesh_pipeline_wireframe,
            grid_pipeline,
            bone_pipeline,
            joint_pipeline,
            joint_sphere,
            joint_sphere_vertices: sphere.len() as u32,
            image_pipeline,
        }
    }

    /// Returns the gpu device.
    pub fn device(&self) -> &Device {
        self.instance.device()
    }

    /// Returns the gpu queue.
    pub fn queue(&self) -> &Queue {
        self.instance.queue()
    }
}
