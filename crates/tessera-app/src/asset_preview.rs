use std::sync::Arc;

use porter_animation::Animation;

use porter_model::MaterialUsage;
use porter_model::Model;

use porter_texture::Image;

use porter_audio::Audio;
use tessera_viewport::ModelMaterial;

/// A decoded model selected by its stable asset id for animation playback.
#[derive(Debug, Clone)]
pub struct AnimationModel {
    pub id: u64,
    pub name: String,
    pub model: Model,
    pub materials: Vec<ModelMaterial>,
}

/// The result of an assets data to be previewed.
#[derive(Debug, Clone)]
pub enum AssetPreview {
    #[cfg(feature = "worlds")]
    Scene(String, tessera_viewport::Scene, bool),
    #[cfg(feature = "worlds")]
    World(
        String,
        porter_world::World,
        indexmap::IndexMap<String, (Model, Vec<ModelMaterial>)>,
        bool,
    ),
    /// This asset type doesn't support preview.
    NotSupported,
    /// An error occured while previewing this asset.
    PreviewError,
    /// A raw file asset for preview.
    RawFile(String, Vec<u8>),
    /// An image asset for preview.
    Image(String, Image),
    /// A model asset for preview.
    Model(String, Model, Vec<Option<Image>>),
    /// A model with material settings and an explicit camera focus request.
    ModelWithMaterials(String, Model, Vec<ModelMaterial>, bool),
    /// A material asset for preview.
    Material(String, Vec<(MaterialUsage, Image)>),
    // An audio asset for preview.
    Audio(String, Audio),
    /// An animation asset for preview.
    Animation(String, Animation),
    /// Source curves (when available) that require backend rig binding after model selection.
    AnimationUnbound(String, Option<Animation>),
    /// An animation decoded for an explicitly selected model and its skeleton.
    AnimationWithModel(String, Animation, Arc<AnimationModel>),
}
