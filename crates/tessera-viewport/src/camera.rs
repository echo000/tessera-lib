use wgpu::util::*;
use wgpu::*;

use porter_math::Matrix4x4;
use porter_math::Vector3;

use porter_utils::AsByteSlice;

use crate::gpu::ViewportContext;

/// The near clip plane distance.
const NEAR_CLIP: f32 = 0.1;
/// The minimum orbit distance.
const MIN_RADIUS: f32 = 0.0001;
/// The minimum angle kept between the view direction and the poles while
/// flying, so the view basis never degenerates.
const MIN_PITCH: f32 = 0.01;

/// The camera uniform as uploaded to the gpu. Field order matches the WGSL
/// `Camera` struct in `shaders/viewport.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CameraUniform {
    view_proj: Matrix4x4,
    /// Inverse of projection * rotation-only view: unprojects a pixel to an eye-relative ray.
    grid_inverse: Matrix4x4,
    /// Projection * rotation-only view: projects an eye-relative point to clip space.
    grid_view: Matrix4x4,
    model: Matrix4x4,
    eye: [f32; 4],
    /// x: eye height, y: minor spacing, z: major spacing, w: fade distance.
    params: [f32; 4],
    /// Eye position modulo the minor (xy) and major (zw) spacing, in f64 so the grid stays
    /// aligned far from the origin.
    grid_phase: [f32; 4],
    /// xy: the world origin relative to the eye; zw: whether the x / z axes are drawn.
    grid_axes: [f32; 4],
    flags: [u32; 4],
}

/// An orbit camera around a target point, with an orthographic override used
/// for 2d image previews. Owns the camera uniform buffer and bind group.
pub(crate) struct Camera {
    theta: f32,
    phi: f32,
    radius: f32,
    fov_deg: f32,
    up: f32,
    target: Vector3,
    /// Up-axis correction for the previewed asset.
    model_matrix: Matrix4x4,
    /// Orthographic override as (width, height, scale), for image previews.
    orthographic: Option<(f32, f32, f32)>,
    shaded: bool,
    srgb: bool,
    grayscale: bool,
    uniform_buffer: Buffer,
    uniform_bind_group: BindGroup,
}

impl Camera {
    /// Constructs a new camera with the default orbit position.
    pub fn new(context: &ViewportContext) -> Self {
        let uniforms = CameraUniform {
            view_proj: Matrix4x4::new(),
            grid_inverse: Matrix4x4::new(),
            grid_view: Matrix4x4::new(),
            model: Matrix4x4::new(),
            eye: [0.0; 4],
            params: [0.0; 4],
            grid_phase: [0.0; 4],
            grid_axes: [0.0; 4],
            flags: [0; 4],
        };

        let uniform_buffer = context.device().create_buffer_init(&BufferInitDescriptor {
            label: Some("tessera-viewport camera"),
            contents: uniforms.as_byte_slice(),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        let uniform_bind_group = context.device().create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &context.camera_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let mut camera = Self {
            theta: 0.0,
            phi: 0.0,
            radius: 0.0,
            fov_deg: 65.0,
            up: 1.0,
            target: Vector3::zero(),
            model_matrix: Matrix4x4::new(),
            orthographic: None,
            shaded: false,
            srgb: false,
            grayscale: false,
            uniform_buffer,
            uniform_bind_group,
        };

        camera.reset();

        camera
    }

    /// Returns the camera bind group.
    pub fn bind_group(&self) -> &BindGroup {
        &self.uniform_bind_group
    }

    /// Returns the FOV in degrees.
    pub fn fov_deg(&self) -> f32 {
        self.fov_deg
    }

    /// Returns true if in orthographic (image preview) mode.
    pub fn is_orthographic(&self) -> bool {
        self.orthographic.is_some()
    }

    /// Sets or clears the orthographic override as (width, height, scale).
    pub fn set_orthographic(&mut self, orthographic: Option<(f32, f32, f32)>) {
        self.orthographic = orthographic;
    }

    /// Sets the orthographic scale value.
    pub fn set_orthographic_scale(&mut self, scale: f32) {
        if let Some((_, _, o_scale)) = &mut self.orthographic {
            *o_scale = scale;
        }
    }

    /// Sets the up-axis correction matrix for the previewed asset.
    pub fn set_model_matrix(&mut self, matrix: Matrix4x4) {
        self.model_matrix = matrix;
    }

    /// Sets whether meshes render with the flat shaded material.
    pub fn set_shaded(&mut self, shaded: bool) {
        self.shaded = shaded;
    }

    /// Returns whether meshes render with the flat shaded material.
    pub fn shaded(&self) -> bool {
        self.shaded
    }

    /// Sets whether the previewed content is in sRGB colorspace.
    pub fn set_srgb(&mut self, srgb: bool) {
        self.srgb = srgb;
    }

    /// Sets whether the previewed image is grayscale.
    pub fn set_grayscale(&mut self, grayscale: bool) {
        self.grayscale = grayscale;
    }

    /// Resets the camera to the default orbit position.
    pub fn reset(&mut self) {
        self.theta = 0.5 * std::f32::consts::PI;
        self.phi = 0.45 * std::f32::consts::PI;
        self.radius = 100.0;
        self.up = 1.0;
        self.target = Vector3::zero();
    }

    /// Rotates the camera by theta/phi.
    pub fn rotate(&mut self, theta: f32, phi: f32) {
        if self.up > 0.0 {
            self.theta += theta;
        } else {
            self.theta -= theta;
        }

        self.phi += phi;

        if self.phi > std::f32::consts::PI {
            self.phi -= std::f32::consts::TAU;
        } else if self.phi < -std::f32::consts::TAU {
            self.phi += std::f32::consts::TAU;
        }

        // Flip the up direction when the camera crosses the poles.
        if (self.phi > 0.0 && self.phi < std::f32::consts::PI)
            || (self.phi < -std::f32::consts::PI && self.phi > -std::f32::consts::TAU)
        {
            self.up = 1.0;
        } else {
            self.up = -1.0;
        }
    }

    /// Zooms the camera by the given distance.
    pub fn zoom(&mut self, distance: f32) {
        self.zoom_smooth(distance * 0.1);
    }

    /// Zooms the camera proportionally to the current distance.
    pub fn zoom_smooth(&mut self, delta: f32) {
        self.radius = (self.radius * (-delta.clamp(-4.0, 4.0) * 0.1).exp()).max(MIN_RADIUS);
    }

    /// Pans the camera across the view plane.
    pub fn pan(&mut self, x: f32, y: f32) {
        let look = (self.target - self.position()).normalized();
        let world_up = Vector3::new(0.0, self.up, 0.0);

        let right = look.cross(world_up);
        let up = look.cross(right);

        self.target += ((right * x) + (up * y)) * (self.radius * 0.1);
    }

    /// Rebuilds theta/phi from the current view direction, upright and with
    /// the pitch kept off the poles, so first person movement has a stable
    /// basis. The eye position is preserved.
    fn level(&mut self) {
        let offset = self.position() - self.target;
        let length = offset.length();

        if !length.is_finite() || length <= f32::EPSILON {
            return;
        }

        let eye = self.target + offset;

        self.phi = (offset.y / length)
            .clamp(-1.0, 1.0)
            .acos()
            .clamp(MIN_PITCH, std::f32::consts::PI - MIN_PITCH);
        self.theta = offset.x.atan2(offset.z);
        self.up = 1.0;

        self.target = eye - self.orbit_offset();
    }

    /// Returns the (forward, right) view basis, levelling the camera first.
    fn fly_basis(&mut self) -> (Vector3, Vector3) {
        self.level();

        let forward = (self.target - self.position()).normalized();
        let right = forward.cross(Vector3::new(0.0, 1.0, 0.0)).normalized();

        (forward, right)
    }

    /// Rotates the view in place around the eye, first person style.
    /// Positive yaw turns right, positive pitch looks up.
    pub fn look(&mut self, yaw: f32, pitch: f32) {
        self.level();

        let eye = self.position();

        self.theta -= yaw;
        self.phi = (self.phi + pitch).clamp(MIN_PITCH, std::f32::consts::PI - MIN_PITCH);

        self.target = eye - self.orbit_offset();
    }

    /// Moves the camera along its view basis: `forward` along the view
    /// direction, `right` across it and `up` along the world up axis.
    pub fn fly(&mut self, forward: f32, right: f32, up: f32) {
        let (forward_dir, right_dir) = self.fly_basis();

        self.target +=
            (forward_dir * forward) + (right_dir * right) + (Vector3::new(0.0, 1.0, 0.0) * up);
    }

    /// Moves the camera along the view direction flattened onto the ground
    /// plane, so driving forward doesn't change height.
    pub fn drive(&mut self, forward: f32) {
        let (forward_dir, _) = self.fly_basis();

        let flat = Vector3::new(forward_dir.x, 0.0, forward_dir.z);

        if flat.length() <= f32::EPSILON {
            return;
        }

        self.target += flat.normalized() * forward;
    }

    /// Focuses the camera on the given point at the given distance.
    pub fn focus_on(&mut self, position: Vector3, distance: f32) {
        self.target = position;
        self.radius = distance.max(MIN_RADIUS);
    }

    /// Returns the camera position in world space.
    pub fn position(&self) -> Vector3 {
        self.target + self.orbit_offset()
    }

    /// The eye offset from the target for the current theta/phi/radius.
    fn orbit_offset(&self) -> Vector3 {
        Vector3::new(
            self.radius * self.phi.sin() * self.theta.sin(),
            self.radius * self.phi.cos(),
            self.radius * self.phi.sin() * self.theta.cos(),
        )
    }

    /// Uploads the current camera state to the gpu.
    pub fn upload(&self, context: &ViewportContext, width: f32, height: f32, far_clip: f32) {
        // Projection * rotation-only view, so grid rays and hit points are eye relative and keep
        // their precision however far the camera is from the origin.
        let mut grid_view = Matrix4x4::new();
        let view_proj = if let Some((o_width, o_height, o_scale)) = self.orthographic {
            let projection = Matrix4x4::orthographic(0.0, width, height, 0.0, -1.0, 1.0);

            // Center the scaled image in the viewport.
            let center_x = (width - (o_width * o_scale)) / 2.0;
            let center_y = (height - (o_height * o_scale)) / 2.0;

            let view = Matrix4x4::create_position(Vector3::new(center_x, center_y, 0.0))
                * Matrix4x4::create_scale(Vector3::new(o_scale, o_scale, 1.0));

            projection * view
        } else {
            // Keep native metre-scale assets frameable without clipping their front surfaces.
            let near_clip = (self.radius * 0.001).clamp(0.000001, NEAR_CLIP);
            let projection =
                Matrix4x4::perspective_fov(self.fov_deg, width / height, near_clip, far_clip);

            let view = Matrix4x4::look_at(
                self.position(),
                self.target,
                Vector3::new(0.0, self.up, 0.0),
            );

            grid_view = projection * rotation_only(&view);

            projection * view
        };

        let eye = self.position();

        let grid = GridParameters::new(eye, self.radius, far_clip);

        let uniforms = CameraUniform {
            view_proj,
            grid_inverse: grid_view.inverse(),
            grid_view,
            model: self.model_matrix,
            eye: [eye.x, eye.y, eye.z, 0.0],
            params: grid.options,
            grid_phase: grid.phase,
            grid_axes: grid.axes,
            flags: [
                self.shaded as u32,
                self.srgb as u32,
                self.grayscale as u32,
                0,
            ],
        };

        context
            .queue()
            .write_buffer(&self.uniform_buffer, 0, uniforms.as_byte_slice());
    }
}

/// The view matrix with its translation removed: the camera sits at the origin.
///
/// porter-math stores row-vector matrices, so the translation is the last row.
fn rotation_only(view: &Matrix4x4) -> Matrix4x4 {
    let mut data = [0.0_f32; 16];

    for (index, value) in data.iter_mut().enumerate() {
        *value = view[index];
    }

    data[12] = 0.0;
    data[13] = 0.0;
    data[14] = 0.0;

    Matrix4x4::from(data)
}

/// The grid uniforms, derived the way echo_engine's editor grid derives them.
struct GridParameters {
    /// x: eye height, y: minor spacing, z: major spacing, w: fade distance.
    options: [f32; 4],
    /// Eye xz modulo minor (xy) and major (zw) spacing.
    phase: [f32; 4],
    /// xy: origin relative to the eye; zw: axis visibility flags.
    axes: [f32; 4],
}

impl GridParameters {
    fn new(eye: Vector3, radius: f32, far_clip: f32) -> Self {
        // Camera-height decades, so distant lines gain coarser layers.
        let spacing = 10.0_f32
            .powf(eye.y.abs().max(1.0).log10().floor() - 1.0)
            .max(0.1);
        let major = spacing * 10.0;

        // Echo fades by camera height alone; an orbit camera also needs the orbit radius,
        // or zooming out at ground level fades the grid away under the model.
        let fade = far_clip.min((eye.y.abs() * 100.0).max(radius * 10.0).max(100.0));

        // f64 keeps the fractional part when the eye is millions of units out.
        let phase = |value: f32, step: f32| f64::from(value).rem_euclid(f64::from(step)) as f32;

        Self {
            options: [eye.y, spacing, major, fade],
            phase: [
                phase(eye.x, spacing),
                phase(eye.z, spacing),
                phase(eye.x, major),
                phase(eye.z, major),
            ],
            axes: [
                -eye.x,
                -eye.z,
                f32::from(eye.x.abs() < fade),
                f32::from(eye.z.abs() < fade),
            ],
        }
    }
}
