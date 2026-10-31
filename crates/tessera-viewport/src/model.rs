use std::sync::Arc;

use wgpu::util::*;
use wgpu::*;

use porter_math::Matrix4x4;

use porter_model::Mesh;
use porter_model::Model;
use porter_model::Skeleton;

use crate::ModelMaterial;

use porter_utils::SliceExt;

use crate::PreviewError;
use crate::gpu::MeshVertex;
use crate::gpu::ViewportContext;
use crate::lines::DynamicJoints;
use crate::lines::DynamicLines;
use crate::texture::PreviewTexture;
use crate::texture::TextureCache;

#[derive(Default)]
pub(crate) struct DrawState {
    pipeline: Option<usize>,
    texture: usize,
    bones: usize,
}

/// A mesh uploaded to the gpu: static vertex/index buffers plus its material
/// texture. Nothing here changes during animation playback.
struct GpuMesh {
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    index_count: u32,
    vertex_count: usize,
    face_count: usize,
    texture: Arc<PreviewTexture>,
    pipeline_index: usize,
    translucent: bool,
}

/// A model preview: static gpu meshes skinned on the gpu against a bone
/// matrix buffer, plus the skeleton line view. Animation updates only write
/// the bone matrices and skeleton lines.
pub(crate) struct ModelPreview {
    meshes: Vec<GpuMesh>,
    /// world * inverse bind pose per bone, uploaded each pose update.
    bones_buffer: Buffer,
    bones_bind_group: BindGroup,
    bone_capacity: usize,
    /// Inverse bind pose matrices from the rest skeleton.
    inverse_bind_poses: Vec<Matrix4x4>,
    skeleton_lines: Option<DynamicLines>,
    joint_markers: Option<DynamicJoints>,
    bone_count: usize,
    pub srgb: bool,
}

/// Converts a mesh into gpu vertices, taking the eight strongest weights.
fn build_vertices(mesh: &Mesh) -> Vec<MeshVertex> {
    let uv_layers = mesh.vertices.uv_layers();
    let influence = mesh.vertices.maximum_influence();

    let mut vertices = Vec::with_capacity(mesh.vertices.len());

    for v in 0..mesh.vertices.len() {
        let vertex = mesh.vertices.vertex(v);

        let position = vertex.position();
        let normal = vertex.normal();

        let uv = if uv_layers > 0 {
            vertex.uv(0)
        } else {
            porter_math::Vector2::zero()
        };

        let mut bone_ids = [0u16; 8];
        let mut bone_weights = [0f32; 8];

        if influence > 0 {
            // Keep the eight strongest influences.
            let mut weights: Vec<(u16, f32)> = (0..influence)
                .map(|w| {
                    let weight = vertex.weight(w);

                    ({ weight.bone }, { weight.value })
                })
                .filter(|(_, value)| *value > 0.0)
                .collect();

            weights.sort_by(|a, b| b.1.total_cmp(&a.1));

            for (slot, (bone, value)) in weights.into_iter().take(8).enumerate() {
                bone_ids[slot] = bone;
                bone_weights[slot] = value;
            }
        }

        vertices.push(MeshVertex {
            position: [position.x, position.y, position.z],
            normal: [normal.x, normal.y, normal.z],
            uv: [uv.x, uv.y],
            bone_ids: std::array::from_fn(|index| bone_ids[index]),
            bone_weights: std::array::from_fn(|index| bone_weights[index]),
            bone_ids_2: std::array::from_fn(|index| bone_ids[index + 4]),
            bone_weights_2: std::array::from_fn(|index| bone_weights[index + 4]),
            color: if mesh.vertices.colors() > 0 {
                let color = vertex.color(0);
                [color.r, color.g, color.b, color.a]
            } else {
                [255; 4]
            },
        });
    }

    vertices
}

impl ModelPreview {
    /// Uploads the given model, with one image per material slot.
    pub fn from_model(
        context: &ViewportContext,
        cache: &mut TextureCache,
        model: &Model,
        materials: &[ModelMaterial],
        srgb: bool,
    ) -> Result<Self, PreviewError> {
        let fallback = Arc::new(PreviewTexture::fallback(context)?);

        let textures: Vec<Arc<PreviewTexture>> = materials
            .iter()
            .map(|material| {
                #[cfg(feature = "normal-maps")]
                let normal_map = material.normal_map.as_ref();
                #[cfg(not(feature = "normal-maps"))]
                let normal_map = None;
                let texture = if let Some(image) = &material.image {
                    cache.get_or_upload(context, image, normal_map, true, material.settings)
                } else {
                    PreviewTexture::fallback_with_normal_map(context, normal_map, material.settings)
                        .map(Arc::new)
                }?;
                Ok(texture)
            })
            .collect::<Result<_, PreviewError>>()?;

        let build_mesh = |mesh: &Mesh, two_sided: bool| -> GpuMesh {
            let vertices = build_vertices(mesh);

            let vertex_buffer = context.device().create_buffer_init(&BufferInitDescriptor {
                label: None,
                contents: vertices.as_slice().as_this_slice(),
                usage: BufferUsages::VERTEX,
            });

            let index_buffer = context.device().create_buffer_init(&BufferInitDescriptor {
                label: None,
                contents: mesh.faces.as_slice().as_this_slice(),
                usage: BufferUsages::INDEX,
            });

            let texture = mesh
                .material
                .and_then(|index| textures.get(index).cloned())
                .unwrap_or_else(|| fallback.clone());
            let mut settings = mesh
                .material
                .and_then(|index| materials.get(index))
                .map(|material| material.settings)
                .unwrap_or_default();
            settings.two_sided |= two_sided;

            GpuMesh {
                vertex_buffer,
                index_buffer,
                index_count: mesh.faces.len() as u32 * 3,
                vertex_count: mesh.vertices.len(),
                face_count: mesh.faces.len(),
                texture,
                pipeline_index: settings.pipeline_index(),
                translucent: settings.blend != crate::MaterialBlend::Opaque,
            }
        };

        let mut meshes: Vec<_> = model
            .meshes
            .iter()
            .map(|mesh| build_mesh(mesh, false))
            .chain(
                model
                    .hairs
                    .iter()
                    .map(|hair| build_mesh(&hair.to_mesh(), true)),
            )
            .collect();
        meshes.sort_by_key(|mesh| mesh.translucent);

        let inverse_bind_poses: Vec<Matrix4x4> = model
            .skeleton
            .bones
            .iter()
            .map(|bone| bone.world_matrix().inverse())
            .collect();

        let bone_count = model.skeleton.bones.len();

        // The rest pose skins to identity; the buffer must hold at least one
        // matrix for the shader binding to be valid.
        let identity = vec![Matrix4x4::new(); bone_count.max(1)];

        let bones_buffer = context.device().create_buffer_init(&BufferInitDescriptor {
            label: Some("tessera-viewport bone matrices"),
            contents: identity.as_slice().as_this_slice(),
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        });

        let bones_bind_group = context.device().create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &context.bones_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: bones_buffer.as_entire_binding(),
            }],
        });

        let (skeleton_lines, joint_markers) = if bone_count > 0 {
            (
                Some(DynamicLines::skeleton(context, &model.skeleton)),
                Some(DynamicJoints::skeleton(context, &model.skeleton)),
            )
        } else {
            (None, None)
        };

        Ok(Self {
            meshes,
            bones_buffer,
            bones_bind_group,
            bone_capacity: bone_count.max(1),
            inverse_bind_poses,
            skeleton_lines,
            joint_markers,
            bone_count,
            srgb,
        })
    }

    /// Applies a posed skeleton: uploads the skinning matrices and rewrites
    /// the skeleton lines. The pose must match the rest skeleton's bone order.
    pub fn update_pose(&mut self, context: &ViewportContext, pose: &Skeleton) {
        let matrices: Vec<Matrix4x4> = pose
            .bones
            .iter()
            .enumerate()
            .map(|(index, bone)| {
                match self.inverse_bind_poses.get(index) {
                    Some(inverse_bind) => bone.world_matrix() * *inverse_bind,
                    // Bones-only previews have no bind pose to invert.
                    None => Matrix4x4::new(),
                }
            })
            .collect();

        if !matrices.is_empty() {
            if matrices.len() > self.bone_capacity {
                self.bones_buffer = context.device().create_buffer_init(&BufferInitDescriptor {
                    label: Some("tessera-viewport bone matrices"),
                    contents: matrices.as_slice().as_this_slice(),
                    usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                });

                self.bones_bind_group = context.device().create_bind_group(&BindGroupDescriptor {
                    label: None,
                    layout: &context.bones_layout,
                    entries: &[BindGroupEntry {
                        binding: 0,
                        resource: self.bones_buffer.as_entire_binding(),
                    }],
                });

                self.bone_capacity = matrices.len();
            } else {
                context.queue().write_buffer(
                    &self.bones_buffer,
                    0,
                    matrices.as_slice().as_this_slice(),
                );
            }
        }

        match &mut self.skeleton_lines {
            Some(lines) => lines.update_skeleton(context, pose),
            None => {
                if !pose.bones.is_empty() {
                    self.skeleton_lines = Some(DynamicLines::skeleton(context, pose));
                }
            }
        }

        match &mut self.joint_markers {
            Some(joints) => joints.update_skeleton(context, pose),
            None => {
                if !pose.bones.is_empty() {
                    self.joint_markers = Some(DynamicJoints::skeleton(context, pose));
                }
            }
        }

        self.bone_count = pose.bones.len();
    }

    /// Returns (meshes, vertices, faces, bones) for the statistics bar.
    pub fn statistics(&self) -> (usize, usize, usize, usize) {
        (
            self.meshes.len(),
            self.meshes.iter().map(|mesh| mesh.vertex_count).sum(),
            self.meshes.iter().map(|mesh| mesh.face_count).sum(),
            self.bone_count,
        )
    }

    /// Draws the meshes and, optionally, the skeleton lines.
    pub fn draw(
        &self,
        context: &ViewportContext,
        render_pass: &mut RenderPass<'_>,
        show_bones: bool,
        wireframe: bool,
    ) {
        self.draw_instanced(
            context,
            render_pass,
            &mut DrawState::default(),
            &context.identity_instance,
            1,
            wireframe,
        );

        if show_bones {
            if let Some(lines) = &self.skeleton_lines {
                render_pass.set_pipeline(&context.bone_pipeline);
                lines.draw(render_pass);
            }

            if let Some(joints) = &self.joint_markers {
                render_pass.set_pipeline(&context.joint_pipeline);
                joints.draw(context, render_pass);
            }
        }
    }

    pub fn draw_instanced(
        &self,
        context: &ViewportContext,
        render_pass: &mut RenderPass<'_>,
        state: &mut DrawState,
        instances: &Buffer,
        instance_count: u32,
        wireframe: bool,
    ) {
        if instance_count == 0 {
            return;
        }

        let bones = std::ptr::from_ref(&self.bones_bind_group) as usize;

        if state.bones != bones {
            render_pass.set_bind_group(2, &self.bones_bind_group, &[]);
            state.bones = bones;
        }

        render_pass.set_vertex_buffer(1, instances.slice(..));

        for mesh in &self.meshes {
            if mesh.index_count == 0 {
                continue;
            }

            let wanted = if wireframe { 12 } else { mesh.pipeline_index };
            if state.pipeline != Some(wanted) {
                render_pass.set_pipeline(if wireframe {
                    &context.mesh_pipeline_wireframe
                } else {
                    &context.mesh_pipelines[wanted]
                });
                state.pipeline = Some(wanted);
            }

            // Compared by address rather than by value: the texture cache hands the same `Arc`
            // to every mesh that shares an image, so this collapses most of a map's binds.
            let texture = Arc::as_ptr(&mesh.texture) as usize;

            if state.texture != texture {
                render_pass.set_bind_group(1, mesh.texture.bind_group(), &[]);
                state.texture = texture;
            }

            render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            render_pass.set_index_buffer(mesh.index_buffer.slice(..), IndexFormat::Uint32);
            render_pass.draw_indexed(0..mesh.index_count, 0, 0..instance_count);
        }
    }
}
