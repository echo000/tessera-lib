use indexmap::IndexMap;

use wgpu::util::*;
use wgpu::*;

use porter_math::Axis;
use porter_math::Matrix4x4;
use porter_math::Vector3;

use porter_model::Model;

use crate::ModelMaterial;

use porter_utils::SliceExt;

use porter_world::Instance;
use porter_world::World;

use crate::PreviewError;
use crate::gpu::ViewportContext;
use crate::model::DrawState;
use crate::model::ModelPreview;
use crate::texture::TextureCache;

/// A placement that retains its complete affine transform, including shear and reflection.
#[derive(Debug, Clone)]
pub struct SceneInstance {
    pub reference: String,
    pub transform: Matrix4x4,
}

/// Models and their native affine placements, uploaded through the world instancing path.
#[derive(Debug, Clone)]
pub struct Scene {
    pub up_axis: Axis,
    pub instances: Vec<SceneInstance>,
    pub models: IndexMap<String, (Model, Vec<ModelMaterial>)>,
}

/// A model referenced by the world: uploaded once, drawn at every placement.
struct WorldModel {
    preview: ModelPreview,
    /// One placement matrix per instance of this model.
    instances: Buffer,
    instance_count: u32,
}

/// A world preview: every model referenced by the world's instances is
/// uploaded once and drawn with gpu instancing, one draw call per mesh.
pub(crate) struct WorldPreview {
    models: Vec<WorldModel>,
    /// The bounds of every placed model, before up-axis correction.
    bounds: Option<(Vector3, Vector3)>,
    pub srgb: bool,
}

/// The placement matrix for an instance (T * R * S).
fn placement_matrix(instance: &Instance) -> Matrix4x4 {
    Matrix4x4::create_position(instance.position)
        * Matrix4x4::create_rotation(instance.rotation)
        * Matrix4x4::create_scale(instance.scale)
}

impl WorldPreview {
    /// Uploads the models referenced by the world's instances, keyed by
    /// [`Instance::reference`]. Models that nothing references aren't uploaded,
    /// and instances referencing a missing model are skipped.
    pub fn from_world(
        context: &ViewportContext,
        textures: &mut TextureCache,
        world: &World,
        models: &IndexMap<String, (Model, Vec<ModelMaterial>)>,
        srgb: bool,
    ) -> Result<Self, PreviewError> {
        let instances = world
            .instances
            .iter()
            .map(|instance| (instance.reference.as_str(), placement_matrix(instance)));

        Self::from_instances(context, textures, instances, models, srgb)
    }

    pub fn from_scene(
        context: &ViewportContext,
        textures: &mut TextureCache,
        scene: &Scene,
        srgb: bool,
    ) -> Result<Self, PreviewError> {
        if scene.instances.iter().any(|instance| {
            !scene.models.contains_key(&instance.reference)
                || (0..16).any(|index| !instance.transform[index].is_finite())
                || instance.transform[3] != 0.0
                || instance.transform[7] != 0.0
                || instance.transform[11] != 0.0
                || instance.transform[15] != 1.0
        }) {
            return Err(PreviewError::InvalidAsset);
        }

        Self::from_instances(
            context,
            textures,
            scene
                .instances
                .iter()
                .map(|instance| (instance.reference.as_str(), instance.transform)),
            &scene.models,
            srgb,
        )
    }

    fn from_instances<'a>(
        context: &ViewportContext,
        textures: &mut TextureCache,
        instances: impl IntoIterator<Item = (&'a str, Matrix4x4)>,
        models: &IndexMap<String, (Model, Vec<ModelMaterial>)>,
        srgb: bool,
    ) -> Result<Self, PreviewError> {
        let mut placements: Vec<Vec<Matrix4x4>> = vec![Vec::new(); models.len()];

        for (reference, transform) in instances {
            if let Some(index) = models.get_index_of(reference) {
                placements[index].push(transform);
            }
        }

        let mut preview = Self {
            models: Vec::new(),
            bounds: None,
            srgb,
        };

        for ((model, materials), placements) in models.values().zip(placements) {
            if placements.is_empty() {
                continue;
            }

            preview.extend_bounds(model, &placements);

            let instances = context.device().create_buffer_init(&BufferInitDescriptor {
                label: Some("tessera-viewport world instances"),
                contents: placements.as_slice().as_this_slice(),
                usage: BufferUsages::VERTEX,
            });

            preview.models.push(WorldModel {
                preview: ModelPreview::from_model(context, textures, model, materials, srgb)?,
                instances,
                instance_count: placements.len() as u32,
            });
        }

        Ok(preview)
    }

    /// Grows the bounds by the model's box at each placement.
    fn extend_bounds(&mut self, model: &Model, placements: &[Matrix4x4]) {
        let aabb = model.bounding_box();

        // A model without vertices has an inverted box.
        if aabb.min.x > aabb.max.x {
            return;
        }

        let (mut min, mut max) = self.bounds.unwrap_or((
            Vector3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
            Vector3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
        ));

        for placement in placements {
            for corner in 0..8 {
                let point = Vector3::new(
                    if corner & 1 == 0 {
                        aabb.min.x
                    } else {
                        aabb.max.x
                    },
                    if corner & 2 == 0 {
                        aabb.min.y
                    } else {
                        aabb.max.y
                    },
                    if corner & 4 == 0 {
                        aabb.min.z
                    } else {
                        aabb.max.z
                    },
                )
                .transform(placement);

                min = Vector3::new(min.x.min(point.x), min.y.min(point.y), min.z.min(point.z));
                max = Vector3::new(max.x.max(point.x), max.y.max(point.y), max.z.max(point.z));
            }
        }

        self.bounds = Some((min, max));
    }

    /// Returns the (min, max) bounds of every placed model, before up-axis
    /// correction, or `None` if nothing is placed.
    pub fn bounds(&self) -> Option<(Vector3, Vector3)> {
        self.bounds
    }

    /// Returns (models, instances, vertices, faces) for the statistics bar.
    /// Vertices and faces count every placement, as drawn.
    pub fn statistics(&self) -> (usize, usize, usize, usize) {
        let mut result = (self.models.len(), 0, 0, 0);

        for model in &self.models {
            let (_, vertices, faces, _) = model.preview.statistics();
            let count = model.instance_count as usize;

            result.1 += count;
            result.2 += vertices * count;
            result.3 += faces * count;
        }

        result
    }

    /// Draws every model at each of its placements.
    pub fn draw(
        &self,
        context: &ViewportContext,
        render_pass: &mut RenderPass<'_>,
        wireframe: bool,
    ) {
        // One state for the whole world: the models share pipelines and textures, and carrying
        // it across them is where most of the redundant binds go.
        let mut state = DrawState::default();

        for model in &self.models {
            model.preview.draw_instanced(
                context,
                render_pass,
                &mut state,
                &model.instances,
                model.instance_count,
                wireframe,
            );
        }
    }
}
