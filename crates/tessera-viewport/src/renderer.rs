#[cfg(feature = "worlds")]
use indexmap::IndexMap;

use wgpu::PollType;

use porter_math::Angles;
use porter_math::Axis;
use porter_math::Matrix4x4;
use porter_math::Quaternion;
use porter_math::Vector2;
use porter_math::Vector3;

use porter_model::MaterialUsage;
use porter_model::Model;
use porter_model::Skeleton;

#[cfg(feature = "worlds")]
use porter_world::World;

use porter_texture::Image;

use crate::ModelMaterial;
use crate::PreviewControlScheme;
use crate::PreviewError;
use crate::ViewportKeyState;
use crate::camera::Camera;
use crate::gpu::ViewportContext;
use crate::image::ImagePreview;
use crate::model::ModelPreview;
use crate::target::RenderTarget;
use crate::texture::TextureCache;
#[cfg(feature = "worlds")]
use crate::world::WorldPreview;

/// The minimum render target size.
const MIN_SIZE: f32 = 256.0;

/// Ortho zoom bounds, in percent. The floor stops images from being zoomed
/// out into nothing.
const MIN_ORTHO_SCALE: i32 = 5;
const MAX_ORTHO_SCALE: i32 = 400;

/// The fly speed scale used when there are no bounds to size it from, in
/// units per second at a 1x multiplier.
const DEFAULT_FLY_SCALE: f32 = 100.0;
/// Fly speed multiplier bounds, and the step applied per scroll notch.
const MIN_FLY_SPEED: f32 = 0.05;
const MAX_FLY_SPEED: f32 = 32.0;
const FLY_SPEED_STEP: f32 = 1.25;
/// Mouse drag movement, as a fraction of the fly speed per pixel.
const FLY_DRAG_SPEED: f32 = 1.0 / 200.0;

/// The content being previewed.
enum Preview {
    None,
    #[cfg(feature = "worlds")]
    World(WorldPreview),
    Model(ModelPreview),
    Image(ImagePreview),
    Material(ImagePreview),
}

/// Renders previews of models, animations, images and materials to an
/// offscreen buffer.
///
/// All pipelines and static buffers are created up front; animation playback
/// only writes bone matrices and skeleton lines (see
/// [`ViewportRenderer::update_pose`]).
pub struct ViewportRenderer {
    context: ViewportContext,
    camera: Camera,
    target: RenderTarget,
    preview: Preview,
    /// Uploaded textures, kept across previews so rebinding an animation to the model it is
    /// already bound to costs nothing, and so a world uploads each texture once.
    textures: TextureCache,
    name: Option<String>,
    far_clip: f32,
    wireframe: bool,
    show_bones: bool,
    show_grid: bool,
    /// The ortho zoom in percent, for image and material previews.
    scale: i32,
    /// Bounds of the current model or world as (min, max, up axis), for
    /// refocusing the camera.
    bounds: Option<(Vector3, Vector3, Axis)>,
    /// Fly speed in units per second at a 1x multiplier, sized to the bounds
    /// so maps and small props both move at a usable pace.
    fly_scale: f32,
    /// Fly speed multiplier, adjusted by scrolling while flying.
    fly_speed: f32,
}

impl ViewportRenderer {
    /// Constructs a new viewport renderer.
    pub fn new() -> Self {
        let context = ViewportContext::new();
        let camera = Camera::new(&context);
        let target = RenderTarget::new(&context, MIN_SIZE as u32, MIN_SIZE as u32);

        Self {
            context,
            camera,
            target,
            preview: Preview::None,
            textures: TextureCache::new(),
            name: None,
            far_clip: 10000.0,
            wireframe: false,
            show_bones: true,
            show_grid: true,
            scale: 100,
            bounds: None,
            fly_scale: DEFAULT_FLY_SCALE,
            fly_speed: 1.0,
        }
    }

    /// Sets the image asset to preview.
    pub fn set_preview_image(&mut self, name: String, image: Image) -> Result<(), PreviewError> {
        self.release_preview();

        let preview = ImagePreview::from_image(&self.context, &image)?;

        self.preview = Preview::Image(preview);
        self.name = Some(name);

        self.fit_orthographic();
        self.reclaim();

        Ok(())
    }

    /// Sets the material asset to preview.
    pub fn set_preview_material(
        &mut self,
        name: String,
        material: Vec<(MaterialUsage, Image)>,
    ) -> Result<(), PreviewError> {
        self.release_preview();

        let preview = ImagePreview::from_material(&self.context, &material)?;

        self.preview = Preview::Material(preview);
        self.name = Some(name);

        self.fit_orthographic();
        self.reclaim();

        Ok(())
    }

    /// Sets the model asset to preview, with one image per material slot.
    /// `materials` is borrowed rather than owned because an animation preview rebinds its model
    /// whenever a new animation is selected, and the textures are kept alive across those rebinds.
    pub fn set_preview_model(
        &mut self,
        name: String,
        model: Model,
        materials: &[ModelMaterial],
        srgb: bool,
        focus: bool,
    ) -> Result<(), PreviewError> {
        self.release_preview();

        let preview =
            ModelPreview::from_model(&self.context, &mut self.textures, &model, materials, srgb)?;

        self.camera.set_orthographic(None);
        self.camera.set_model_matrix(up_axis_matrix(model.up_axis));

        self.set_bounds(Self::model_bounds(&model));

        if focus {
            self.focus();
        }

        self.preview = Preview::Model(preview);
        self.name = Some(name);

        self.reclaim();

        Ok(())
    }

    /// Sets the world asset to preview. `models` is keyed by the
    /// [`porter_world::Instance::reference`] that places it, with one image
    /// per material slot of that model. Only placed models are drawn, so world
    /// geometry (like a map's bsp) needs its own identity instance.
    #[cfg(feature = "worlds")]
    pub fn set_preview_world(
        &mut self,
        name: String,
        world: World,
        models: IndexMap<String, (Model, Vec<ModelMaterial>)>,
        srgb: bool,
        focus: bool,
    ) -> Result<(), PreviewError> {
        self.release_preview();

        let preview =
            WorldPreview::from_world(&self.context, &mut self.textures, &world, &models, srgb)?;

        self.finish_world_preview(name, preview, world.up_axis, focus);

        Ok(())
    }

    /// Previews native affine placements without converting them through quaternion/scale pairs.
    #[cfg(feature = "worlds")]
    pub fn set_preview_scene(
        &mut self,
        name: String,
        scene: crate::Scene,
        srgb: bool,
        focus: bool,
    ) -> Result<(), PreviewError> {
        self.release_preview();

        let preview = WorldPreview::from_scene(&self.context, &mut self.textures, &scene, srgb)?;

        self.finish_world_preview(name, preview, scene.up_axis, focus);

        Ok(())
    }

    #[cfg(feature = "worlds")]
    fn finish_world_preview(
        &mut self,
        name: String,
        preview: WorldPreview,
        up_axis: Axis,
        focus: bool,
    ) {
        self.camera.set_orthographic(None);
        self.camera.set_model_matrix(up_axis_matrix(up_axis));

        self.set_bounds(preview.bounds().map(|(min, max)| (min, max, up_axis)));

        if focus {
            self.focus();
        }
        self.preview = Preview::World(preview);
        self.name = Some(name);

        self.reclaim();
    }

    /// Applies a posed skeleton to the current model preview: bone matrices
    /// and skeleton lines only, nothing else is touched. The pose must match
    /// the bone order of the model the preview was created with.
    pub fn update_pose(&mut self, pose: &Skeleton) {
        if let Preview::Model(model) = &mut self.preview {
            model.update_pose(&self.context, pose);
        }
    }

    /// Clears the asset being previewed.
    pub fn clear_preview(&mut self) {
        self.preview = Preview::None;
        self.name = None;

        self.camera.set_orthographic(None);
        self.set_bounds(None);

        self.textures.clear();
        self.reclaim();
    }

    /// Drops the geometry of the outgoing preview before the incoming one is built.
    ///
    /// A preview is the largest thing the viewport holds, and building the replacement while the
    /// old one is still resident doubles the peak for no reason. Its textures survive in the
    /// cache, so the rebuild re-uses them rather than uploading again.
    fn release_preview(&mut self) {
        self.preview = Preview::None;
        self.set_bounds(None);
    }

    /// Returns the gpu memory the last swap freed.
    ///
    /// Dropping a wgpu resource only queues it for destruction; the allocation comes back when the
    /// device next polls. Previews are large and are often replaced several times between frames,
    /// so waiting for the next render to poll is what makes the viewport look like it leaks.
    fn reclaim(&mut self) {
        self.textures.prune();

        let _ = self.context.device().poll(PollType::Wait {
            submission_index: None,
            timeout: None,
        });
    }

    /// Returns true if the preview is empty.
    pub fn is_empty_preview(&self) -> bool {
        matches!(self.preview, Preview::None)
    }

    /// Sets the bounds of the current content, and sizes the fly speed to it.
    fn set_bounds(&mut self, bounds: Option<(Vector3, Vector3, Axis)>) {
        self.bounds = bounds;

        let radius = bounds.map(|(min, max, _)| ((max - min) / 2.0).length());

        self.fly_scale = match radius {
            Some(radius) if radius.is_finite() && radius > 0.0 => radius.max(1.0),
            _ => DEFAULT_FLY_SCALE,
        };
    }

    /// Points the camera at the current content, or the default orbit when
    /// there is nothing to frame.
    pub fn focus(&mut self) {
        match self.bounds {
            Some((min, max, up_axis)) => self.focus_on_bounds(min, max, up_axis),
            None if self.camera.is_orthographic() => self.fit_orthographic(),
            None => self.camera.reset(),
        }
    }

    /// Points the camera at the bounding sphere of the given box.
    pub fn focus_on_model(&mut self, model: &Model) {
        self.set_bounds(Self::model_bounds(model));
        self.focus();
    }

    fn model_bounds(model: &Model) -> Option<(Vector3, Vector3, Axis)> {
        if model.meshes.iter().any(|mesh| !mesh.vertices.is_empty()) {
            let bounds = model.bounding_box();
            return Some((bounds.min, bounds.max, model.up_axis));
        }
        let first = model.skeleton.bones.first()?.world_position;
        let mut min = first;
        let mut max = first;
        for bone in &model.skeleton.bones {
            let position = bone.world_position;
            min = Vector3::new(
                min.x.min(position.x),
                min.y.min(position.y),
                min.z.min(position.z),
            );
            max = Vector3::new(
                max.x.max(position.x),
                max.y.max(position.y),
                max.z.max(position.z),
            );
        }

        Some((min, max, model.up_axis))
    }

    /// Frames the posed geometry when a clip places the actor away from its bind pose.
    pub fn focus_on_pose(&mut self, model: &Model, pose: &Skeleton) {
        if model.meshes.is_empty() || model.skeleton.bones.len() != pose.bones.len() {
            return;
        }

        let inverse_bind = model
            .skeleton
            .bones
            .iter()
            .enumerate()
            .map(|(index, bone)| (index as u16, bone.world_matrix().inverse()))
            .collect();
        let mut posed = model.clone();
        posed.skeleton = pose.clone();
        posed.apply_bind_pose(&inverse_bind);
        self.focus_on_model(&posed);
    }

    /// Points the camera at the bounding sphere of the given box.
    fn focus_on_bounds(&mut self, min: Vector3, max: Vector3, up_axis: Axis) {
        // The geometry is rotated into the y-up world by the up-axis matrix
        // (see the shader); apply the same rotation to the centroid so the
        // camera looks at where the model actually renders.
        let center = ((min + max) / 2.0).transform(&up_axis_matrix(up_axis));

        let radius = ((max - min) / 2.0).length();

        if !radius.is_finite() || radius <= 0.0 {
            self.camera.reset();
            return;
        }

        let aspect = self.target.width as f32 / self.target.height as f32;
        let fov = self.camera.fov_deg().to_radians();

        let vertical_half_angle = fov / 2.0;
        let horizontal_half_angle = (vertical_half_angle.tan() * aspect).atan();
        let half_angle = vertical_half_angle.min(horizontal_half_angle);

        // Fit the sphere's tangent, including its depth, with a small framing margin.
        let distance = radius / half_angle.sin() * 1.05;
        self.camera.focus_on(center, distance);
        self.far_clip = self.far_clip.max(distance + radius * 2.0);
    }

    /// Sets the ortho projection to fit the current image in the viewport.
    fn fit_orthographic(&mut self) {
        let preview = match &self.preview {
            Preview::Image(preview) | Preview::Material(preview) => preview,
            _ => return,
        };

        let Some((width, height)) = preview.size() else {
            // Keep cycling working for materials whose current image failed.
            self.camera.set_orthographic(Some((1.0, 1.0, 1.0)));
            self.scale = 100;
            return;
        };

        let fit = (self.target.width as f32 / width as f32)
            .min(self.target.height as f32 / height as f32);

        self.scale = ((fit * 100.0) as i32).clamp(MIN_ORTHO_SCALE, 100);

        self.camera.set_orthographic(Some((
            width as f32,
            height as f32,
            self.scale as f32 / 100.0,
        )));
    }

    /// Resizes the renderer output, in physical pixels.
    pub fn resize(&mut self, width: f32, height: f32, far_clip: f32) {
        let width = width.max(1.0) as u32;
        let height = height.max(1.0) as u32;

        self.far_clip = far_clip;

        if self.target.width == width && self.target.height == height {
            return;
        }

        self.target = RenderTarget::new(&self.context, width, height);
    }

    /// Cycles to the next image in a material preview.
    pub fn cycle_material(&mut self) {
        if let Preview::Material(material) = &mut self.preview {
            material.next();
            self.fit_orthographic();
        }
    }

    /// Toggles the wireframe view.
    pub fn orbit(&mut self, theta: f32, phi: f32) {
        self.camera.rotate(theta, phi);
    }

    /// Toggles the wireframe view.
    pub fn toggle_wireframe(&mut self) {
        self.wireframe = !self.wireframe;
    }

    /// Toggles the bone view.
    pub fn toggle_bones(&mut self) {
        self.show_bones = !self.show_bones;
    }

    /// Toggles the grid view.
    pub fn toggle_grid(&mut self) {
        self.show_grid = !self.show_grid;
    }

    /// Toggles the shaded view.
    pub fn toggle_shaded(&mut self) {
        self.camera.set_shaded(!self.camera.shaded());
    }

    /// Resets the camera view.
    pub fn reset_view(&mut self) {
        if self.camera.is_orthographic() {
            self.fit_orthographic();
        } else {
            self.camera.reset();
            self.focus();
        }
    }

    /// Performs a scrolling operation.
    pub fn scroll_delta(&mut self, delta: f32, key_state: ViewportKeyState) {
        if self.camera.is_orthographic() {
            let step = if delta > 0.0 { 3 } else { -3 };

            self.scale = (self.scale + step).clamp(MIN_ORTHO_SCALE, MAX_ORTHO_SCALE);

            self.camera
                .set_orthographic_scale(self.scale as f32 / 100.0);
        } else if matches!(key_state.control_scheme, PreviewControlScheme::Unreal) {
            if key_state.right {
                // Scrolling while flying changes the fly speed, like Unreal.
                let step = if delta > 0.0 {
                    FLY_SPEED_STEP
                } else {
                    1.0 / FLY_SPEED_STEP
                };

                self.fly_speed = (self.fly_speed * step).clamp(MIN_FLY_SPEED, MAX_FLY_SPEED);
            } else {
                // Unreal dollies the camera itself rather than zooming on a
                // pivot, so it can scroll through the scene.
                self.camera.fly(delta * self.fly_units() * 0.1, 0.0, 0.0);
            }
        } else {
            self.camera.zoom_smooth(delta * 1.1);
        }
    }

    /// The current fly speed, in units per second.
    fn fly_units(&self) -> f32 {
        self.fly_scale * self.fly_speed
    }

    /// Returns true when previewing 2d content (images and materials).
    pub fn is_orthographic(&self) -> bool {
        self.camera.is_orthographic()
    }

    /// Returns the fly speed multiplier.
    pub fn fly_speed(&self) -> f32 {
        self.fly_speed
    }

    /// Moves the camera first person for `delta` seconds. Each axis is a
    /// direction in -1..=1: forward along the view, right across it and up
    /// along the world up axis.
    pub fn fly(&mut self, forward: f32, right: f32, up: f32, delta: f32) {
        if self.camera.is_orthographic() {
            return;
        }

        let distance = self.fly_units() * delta;

        self.camera
            .fly(forward * distance, right * distance, up * distance);
    }

    /// Updates the far clip distance.
    pub fn far_clip(&mut self, far_clip: f32) {
        self.far_clip = far_clip;
    }

    /// Performs a mouse move operation.
    pub fn mouse_move<D: Into<Vector2>>(&mut self, delta: D, key_state: ViewportKeyState) {
        let delta = delta.into();

        const ROTATE_SPEED: f32 = 1.0 / 200.0;
        const ZOOM_SPEED: f32 = 0.3;
        const PAN_SPEED: f32 = 1.0 / 80.0;

        let rotate = |camera: &mut Camera| {
            camera.rotate(delta.x * ROTATE_SPEED, delta.y * ROTATE_SPEED);
        };

        let zoom = |camera: &mut Camera| {
            camera.zoom(-((delta.x + delta.y) * ZOOM_SPEED));
        };

        let pan = |camera: &mut Camera| {
            camera.pan(delta.x * PAN_SPEED, delta.y * PAN_SPEED);
        };

        match key_state.control_scheme {
            PreviewControlScheme::Simple | PreviewControlScheme::Maya => {
                if matches!(key_state.control_scheme, PreviewControlScheme::Maya) && !key_state.alt
                {
                    return;
                }

                if key_state.left {
                    rotate(&mut self.camera);
                } else if key_state.right {
                    zoom(&mut self.camera);
                } else if key_state.middle {
                    pan(&mut self.camera);
                }
            }
            PreviewControlScheme::Unreal => {
                if self.camera.is_orthographic() {
                    return;
                }

                // Mouse deltas are `last - current`: positive x is a drag
                // left, positive y a drag up.
                let units = self.fly_units() * FLY_DRAG_SPEED;

                if key_state.alt {
                    // Alt gives the Maya style orbit controls, as in Unreal.
                    if key_state.left {
                        rotate(&mut self.camera);
                    } else if key_state.right {
                        zoom(&mut self.camera);
                    } else if key_state.middle {
                        pan(&mut self.camera);
                    }
                } else if key_state.middle || (key_state.left && key_state.right) {
                    self.camera.fly(0.0, -delta.x * units, delta.y * units);
                } else if key_state.right {
                    self.camera
                        .look(-delta.x * ROTATE_SPEED, delta.y * ROTATE_SPEED);
                } else if key_state.left {
                    self.camera.look(-delta.x * ROTATE_SPEED, 0.0);
                    self.camera.drive(delta.y * units);
                }
            }
            PreviewControlScheme::Blender => {
                if key_state.middle && key_state.shift {
                    pan(&mut self.camera);
                } else if key_state.middle && key_state.alt {
                    zoom(&mut self.camera);
                } else if key_state.middle {
                    rotate(&mut self.camera);
                }
            }
        }
    }

    /// Returns the statistics for the current preview.
    pub fn statistics(&self) -> Vec<(String, String)> {
        let name = || {
            (
                String::from("Name"),
                self.name.clone().unwrap_or_else(|| String::from("N/A")),
            )
        };

        match &self.preview {
            #[cfg(feature = "worlds")]
            Preview::World(world) => {
                let (models, instances, vertices, faces) = world.statistics();

                vec![
                    name(),
                    (String::from("Models"), models.to_string()),
                    (String::from("Instances"), instances.to_string()),
                    (String::from("Verts"), vertices.to_string()),
                    (String::from("Tris"), faces.to_string()),
                ]
            }
            Preview::Model(model) => {
                let (meshes, vertices, faces, bones) = model.statistics();

                vec![
                    name(),
                    (String::from("Meshes"), meshes.to_string()),
                    (String::from("Verts"), vertices.to_string()),
                    (String::from("Tris"), faces.to_string()),
                    (String::from("Bones"), bones.to_string()),
                ]
            }
            Preview::Image(image) => {
                let (width, height) = image.size().unwrap_or_default();

                vec![
                    name(),
                    (String::from("Width"), width.to_string()),
                    (String::from("Height"), height.to_string()),
                    (String::from("Scale"), format!("{}%", self.scale)),
                ]
            }
            Preview::Material(material) => {
                let mut result = vec![
                    name(),
                    (
                        String::from("Image"),
                        format!("{} of {}", material.index() + 1, material.len()),
                    ),
                ];

                if material.is_error() {
                    result.push((
                        String::from("Status"),
                        String::from("Unable to preview this image"),
                    ));
                } else {
                    let (width, height) = material.size().unwrap_or_default();

                    result.extend([
                        (String::from("Usage"), material.usage()),
                        (String::from("Width"), width.to_string()),
                        (String::from("Height"), height.to_string()),
                        (String::from("Scale"), format!("{}%", self.scale)),
                    ]);
                }

                result
            }
            Preview::None => vec![name()],
        }
    }

    /// Renders a frame in GPUI's premultiplied BGRA format.
    pub fn render_bgra(&mut self) -> (u32, u32, Vec<u8>) {
        let (width, height, mut pixels) = self.render();
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            let alpha = u16::from(pixel[3]);
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
            }
        }
        (width, height, pixels)
    }

    /// Renders a frame and returns (width, height, rgba pixels).
    pub fn render(&mut self) -> (u32, u32, Vec<u8>) {
        // Sync per-preview color state to the camera uniform.
        match &self.preview {
            #[cfg(feature = "worlds")]
            Preview::World(world) => {
                self.camera.set_srgb(world.srgb);
                self.camera.set_grayscale(false);
            }
            Preview::Model(model) => {
                self.camera.set_srgb(model.srgb);
                self.camera.set_grayscale(false);
            }
            Preview::Image(image) | Preview::Material(image) => {
                self.camera.set_srgb(image.srgb());
                self.camera.set_grayscale(image.grayscale());
            }
            Preview::None => {
                self.camera.set_srgb(false);
                self.camera.set_grayscale(false);
            }
        }

        self.camera.upload(
            &self.context,
            self.target.width as f32,
            self.target.height as f32,
            self.far_clip,
        );

        let mut encoder = self
            .context
            .device()
            .create_command_encoder(&Default::default());

        {
            let mut render_pass = self.target.begin_pass(&mut encoder);

            render_pass.set_bind_group(0, self.camera.bind_group(), &[]);

            // Before the geometry, so the mesh paints over the plane wherever they overlap:
            // assets previewed at their authored world placement can straddle y = 0, and a
            // viewer wants the model whole, not sliced by grid lines (echo draws its editor
            // grid after geometry for the opposite reason).
            if !self.camera.is_orthographic() && self.show_grid {
                render_pass.set_pipeline(&self.context.grid_pipeline);
                render_pass.draw(0..3, 0..1);
            }

            match &self.preview {
                #[cfg(feature = "worlds")]
                Preview::World(world) => {
                    world.draw(&self.context, &mut render_pass, self.wireframe);
                }
                Preview::Model(model) => {
                    model.draw(
                        &self.context,
                        &mut render_pass,
                        self.show_bones,
                        self.wireframe,
                    );
                }
                Preview::Image(image) | Preview::Material(image) => {
                    image.draw(&self.context, &mut render_pass);
                }
                Preview::None => {}
            }
        }

        let pixels = self.target.read_back(&self.context, encoder);

        (self.target.width, self.target.height, pixels)
    }
}

impl Default for ViewportRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// The up-axis correction matrix for previewed assets (viewport is y-up).
fn up_axis_matrix(up_axis: Axis) -> Matrix4x4 {
    match up_axis {
        Axis::X => Quaternion::from_euler(Vector3::new(0.0, 0.0, -90.0), Angles::Degrees).to_4x4(),
        Axis::Y => Matrix4x4::new(),
        Axis::Z => Quaternion::from_euler(Vector3::new(-90.0, 0.0, 0.0), Angles::Degrees).to_4x4(),
    }
}
