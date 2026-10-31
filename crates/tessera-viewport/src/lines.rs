use wgpu::util::*;
use wgpu::*;

use porter_math::Vector3;

use porter_model::Skeleton;

use porter_utils::SliceExt;

use crate::gpu::JointInstance;
use crate::gpu::LineVertex;
use crate::gpu::ViewportContext;

const BONE_LINK_COLOR: [f32; 4] = [0.85, 0.75, 0.3, 1.0];

/// Latitude/longitude subdivisions of a joint marker sphere.
const JOINT_STACKS: usize = 8;
const JOINT_SECTORS: usize = 12;

/// A line list re-written every animation frame (the skeleton).
pub(crate) struct DynamicLines {
    buffer: Buffer,
    /// The size the buffer was allocated with, in vertices.
    capacity: usize,
    vertex_count: u32,
}

/// A per-bone instance buffer re-written every animation frame. The shared
/// unit sphere (in [`ViewportContext`]) is drawn once per instance.
pub(crate) struct DynamicJoints {
    buffer: Buffer,
    /// The size the buffer was allocated with, in instances.
    capacity: usize,
    instance_count: u32,
}

/// Builds the parent-link lines connecting each bone to its parent.
fn build_skeleton(skeleton: &Skeleton) -> Vec<LineVertex> {
    let mut vertices = Vec::with_capacity(skeleton.bones.len() * 2);

    for bone in &skeleton.bones {
        if bone.parent > -1 {
            let from = bone.world_position;
            let to = skeleton.bones[bone.parent as usize].world_position;

            vertices.push(LineVertex {
                position: [from.x, from.y, from.z],
                color: BONE_LINK_COLOR,
            });
            vertices.push(LineVertex {
                position: [to.x, to.y, to.z],
                color: BONE_LINK_COLOR,
            });
        }
    }

    vertices
}

/// Builds the shared unit sphere as a triangle list, wound counter-clockwise
/// for outward-facing front faces. Each position doubles as the surface normal.
pub(crate) fn unit_sphere() -> Vec<Vector3> {
    let mut dirs = Vec::with_capacity(JOINT_STACKS * JOINT_SECTORS * 6);

    let point = |stack: usize, sector: usize| {
        let phi = std::f32::consts::PI * stack as f32 / JOINT_STACKS as f32;
        let theta = std::f32::consts::TAU * sector as f32 / JOINT_SECTORS as f32;
        let (sin_phi, cos_phi) = phi.sin_cos();
        let (sin_theta, cos_theta) = theta.sin_cos();

        Vector3::new(sin_phi * cos_theta, cos_phi, sin_phi * sin_theta)
    };

    for stack in 0..JOINT_STACKS {
        for sector in 0..JOINT_SECTORS {
            let a = point(stack, sector);
            let b = point(stack + 1, sector);
            let c = point(stack + 1, sector + 1);
            let d = point(stack, sector + 1);

            dirs.extend([a, c, b, a, d, c]);
        }
    }

    dirs
}

/// Builds one joint marker instance per bone: the world-space position the
/// shared sphere is drawn at. The shader scales each sphere by camera distance
/// for a constant screen size.
fn build_joint_instances(skeleton: &Skeleton) -> Vec<JointInstance> {
    skeleton
        .bones
        .iter()
        .map(|bone| JointInstance {
            center: [
                bone.world_position.x,
                bone.world_position.y,
                bone.world_position.z,
            ],
        })
        .collect()
}

impl DynamicLines {
    /// Creates a skeleton line buffer for the given skeleton.
    pub fn skeleton(context: &ViewportContext, skeleton: &Skeleton) -> Self {
        let vertices = build_skeleton(skeleton);

        let buffer = context.device().create_buffer_init(&BufferInitDescriptor {
            label: Some("tessera-viewport skeleton"),
            contents: vertices.as_slice().as_this_slice(),
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        });

        Self {
            buffer,
            capacity: vertices.len(),
            vertex_count: vertices.len() as u32,
        }
    }

    /// Rewrites the skeleton lines in place, reallocating only if the posed
    /// skeleton produces more vertices than the buffer holds.
    pub fn update_skeleton(&mut self, context: &ViewportContext, skeleton: &Skeleton) {
        let vertices = build_skeleton(skeleton);

        if vertices.len() > self.capacity {
            *self = Self::skeleton(context, skeleton);
            return;
        }

        context
            .queue()
            .write_buffer(&self.buffer, 0, vertices.as_slice().as_this_slice());

        self.vertex_count = vertices.len() as u32;
    }

    /// Draws the lines; the pipeline must already be set.
    pub fn draw(&self, render_pass: &mut RenderPass<'_>) {
        if self.vertex_count == 0 {
            return;
        }

        let bytes = self.vertex_count as u64 * size_of::<LineVertex>() as u64;

        render_pass.set_vertex_buffer(0, self.buffer.slice(..bytes));
        render_pass.draw(0..self.vertex_count, 0..1);
    }
}

impl DynamicJoints {
    /// Creates a joint instance buffer for the given skeleton.
    pub fn skeleton(context: &ViewportContext, skeleton: &Skeleton) -> Self {
        let instances = build_joint_instances(skeleton);

        let buffer = context.device().create_buffer_init(&BufferInitDescriptor {
            label: Some("tessera-viewport joints"),
            contents: instances.as_slice().as_this_slice(),
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        });

        Self {
            buffer,
            capacity: instances.len(),
            instance_count: instances.len() as u32,
        }
    }

    /// Rewrites the joint instances in place, reallocating only if the posed
    /// skeleton has more bones than the buffer holds.
    pub fn update_skeleton(&mut self, context: &ViewportContext, skeleton: &Skeleton) {
        let instances = build_joint_instances(skeleton);

        if instances.len() > self.capacity {
            *self = Self::skeleton(context, skeleton);
            return;
        }

        context
            .queue()
            .write_buffer(&self.buffer, 0, instances.as_slice().as_this_slice());

        self.instance_count = instances.len() as u32;
    }

    /// Draws the joint markers by instancing the shared unit sphere; the
    /// pipeline must already be set.
    pub fn draw(&self, context: &ViewportContext, render_pass: &mut RenderPass<'_>) {
        if self.instance_count == 0 {
            return;
        }

        let bytes = self.instance_count as u64 * size_of::<JointInstance>() as u64;

        render_pass.set_vertex_buffer(0, context.joint_sphere.slice(..));
        render_pass.set_vertex_buffer(1, self.buffer.slice(..bytes));
        render_pass.draw(0..context.joint_sphere_vertices, 0..self.instance_count);
    }
}
