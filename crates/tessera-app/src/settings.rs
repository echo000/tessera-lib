use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Deserialize;
use serde::Serialize;

use serde_json::json;

use directories::ProjectDirs;
use directories::UserDirs;

use bitflags::bitflags;

use porter_animation::AnimationFileType;
use porter_audio::AudioFileType;
use porter_model::ModelFileType;
use porter_texture::ImageFileType;
use tessera_viewport::PreviewControlScheme;

use crate::theme::ThemeVariant;

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
struct LoadSettings(u32);

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
struct ModelSettings(u32);

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
struct AnimSettings(u32);

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
struct AudioSettings(u32);

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
struct MapSettings(u32);

bitflags! {
    impl LoadSettings: u32 {
        const LOAD_MODELS = 1 << 0;
        const LOAD_IMAGES = 1 << 1;
        const LOAD_MATERIALS = 1 << 2;
        const LOAD_ANIMATIONS = 1 << 3;
        const LOAD_SOUNDS = 1 << 4;
        const LOAD_MAPS = 1 << 5;
        const LOAD_RAW_FILES = 1 << 6;
    }
}

bitflags! {
    impl ModelSettings: u32 {
        const EXPORT_OBJ = 1 << 0;
        const EXPORT_SMD = 1 << 1;
        const EXPORT_XNA_LARA = 1 << 2;
        const EXPORT_XMODEL_EXPORT = 1 << 3;
        const EXPORT_SEMODEL_REMOVED = 1 << 4;
        const EXPORT_CAST = 1 << 5;
        const EXPORT_MAYA = 1 << 6;
        const EXPORT_FBX = 1 << 7;
    }
}

bitflags! {
    impl AnimSettings: u32 {
        const EXPORT_SEANIM_REMOVED = 1 << 0;
        const EXPORT_CAST = 1 << 1;
    }
}

bitflags! {
    impl AudioSettings: u32 {
        const EXPORT_WAV = 1 << 0;
        const EXPORT_FLAC = 1 << 2;
    }
}

bitflags! {
    impl MapSettings: u32 {
        const EXPORT_CAST = 1 << 0;
    }
}

impl Default for MapSettings {
    fn default() -> Self {
        Self::EXPORT_CAST
    }
}

/// Options for processing normal maps through the converter.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
pub enum ImageNormalMapProcessing {
    None,
    OpenGl,
    DirectX,
}

/// Options for processing model materials.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
pub enum ModelMaterialProcessing {
    Skip,
    InModelFolder,
    InMaterialFolder,
}

/// Options for laying out the folders a sound is exported into.
///
/// Most titles name a sound by the path it had in the source tree, so the name carries a folder
/// structure the export can either keep or discard.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
pub enum SoundPathProcessing {
    /// Every sound lands directly in the sounds folder, named by the last part of its name. Two
    /// sounds whose names differ only by folder overwrite one another.
    FileName,
    /// The sound's name is taken as a path, and the folders it names are created under the sounds
    /// folder.
    FolderTree,
}

/// Serde support for [`ImageFileType`], which porter-texture serializes through miniserde rather
/// than serde. The variant names are written as-is so settings files stay readable across this.
mod image_file_type_serde {
    use super::ImageFileType;

    use serde::Deserialize;
    use serde::Deserializer;
    use serde::Serializer;
    use serde::de::Error;

    pub fn serialize<S: Serializer>(
        file_type: &ImageFileType,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match file_type {
            ImageFileType::Dds => "Dds",
            ImageFileType::Png => "Png",
            ImageFileType::Tiff => "Tiff",
            ImageFileType::Tga => "Tga",
            ImageFileType::Pvr => "Pvr",
        })
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<ImageFileType, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "Dds" => Ok(ImageFileType::Dds),
            "Png" => Ok(ImageFileType::Png),
            "Tiff" => Ok(ImageFileType::Tiff),
            "Tga" => Ok(ImageFileType::Tga),
            "Pvr" => Ok(ImageFileType::Pvr),
            other => Err(D::Error::custom(format!("unknown image file type {other}"))),
        }
    }
}

/// Global application settings.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Settings {
    version: u32,
    #[serde(default)]
    shared_settings_version: u32,
    load_settings: LoadSettings,
    model_settings: ModelSettings,
    anim_settings: AnimSettings,
    audio_settings: AudioSettings,
    #[serde(with = "image_file_type_serde")]
    image_file_type: ImageFileType,
    image_normal_map_processing: ImageNormalMapProcessing,
    output_directory: Option<PathBuf>,
    preview_controls: PreviewControlScheme,
    preview_overlay: bool,
    far_clip: u32,
    preview_window: bool,
    custom_scale: Option<f32>,
    lod_export: bool,
    export_material_info: bool,
    strip_material_dir: bool,
    log_assets: bool,
    skip_previously_exported: bool,
    build_blend_shapes: bool,
    auto_focus_on_model: bool,
    volume: u32,
    always_on_top: bool,
    // Newly added fields must default so older settings files keep loading.
    #[serde(default)]
    theme: ThemeVariant,
    #[serde(default)]
    map_settings: MapSettings,
    #[serde(default = "default_model_material_processing")]
    model_material_processing: ModelMaterialProcessing,
    #[serde(default = "default_sound_path_processing")]
    sound_path_processing: SoundPathProcessing,
    #[serde(default = "default_auto_scale")]
    auto_scale: bool,
    #[serde(default)]
    last_files: Option<Vec<PathBuf>>,
    #[serde(default)]
    anim_bake: bool,
    #[serde(default)]
    force_raw_files: bool,
}

fn default_model_material_processing() -> ModelMaterialProcessing {
    ModelMaterialProcessing::InMaterialFolder
}

/// Matches how every earlier build exported, so an existing setup keeps the layout it has.
fn default_sound_path_processing() -> SoundPathProcessing {
    SoundPathProcessing::FileName
}

fn default_auto_scale() -> bool {
    true
}

impl Settings {
    /// Loads the settings from the disk at the given path, or returns new ones.
    pub fn load<S: Into<String>>(name: S) -> Settings {
        let Some(project_directory) = ProjectDirs::from("com", "", "Tessera") else {
            return Default::default();
        };

        let path = project_directory
            .config_dir()
            .join(name.into().to_lowercase());
        std::fs::read(path.with_extension("json"))
            .or_else(|_| std::fs::read(path.with_extension("dat")))
            .ok()
            .and_then(|buffer| Self::from_bytes(&buffer).ok())
            .unwrap_or_default()
    }

    fn from_bytes(buffer: &[u8]) -> Result<Self, serde_json::Error> {
        let value: serde_json::Value = serde_json::from_slice(buffer)?;
        let mut settings: Self = serde_json::from_slice(buffer)?;
        if settings.shared_settings_version == 0 && value.get("image_download_cdn").is_some() {
            // The old CoD-derived settings put maps and raw files at bits 10 and 11.
            let flags = settings.load_settings.bits();
            settings.load_settings = LoadSettings::from_bits_retain(
                (flags & 31) | (((flags >> 10) & 1) << 5) | (((flags >> 11) & 1) << 6),
            );
        }
        if value.get("model_material_processing").is_none()
            && let Some(folders) = value
                .get("export_material_folders")
                .and_then(serde_json::Value::as_bool)
        {
            settings.model_material_processing = if folders {
                ModelMaterialProcessing::InMaterialFolder
            } else {
                ModelMaterialProcessing::InModelFolder
            };
        }
        settings.shared_settings_version = 1;
        Ok(settings)
    }

    /// Saves the settings to the disk at the given path.
    pub fn save<S: Into<String>>(&self, name: S) {
        let Some(project_directory) = ProjectDirs::from("com", "", "Tessera") else {
            return;
        };

        let json = json!(self);

        let Ok(result) = serde_json::to_vec_pretty(&json) else {
            return;
        };

        let dirs = std::fs::create_dir_all(project_directory.config_dir());

        debug_assert!(dirs.is_ok());

        let result = std::fs::write(
            project_directory
                .config_dir()
                .join(name.into().to_lowercase())
                .with_extension("json"),
            result,
        );

        debug_assert!(result.is_ok());
    }

    /// Gets the last successfully loaded files.
    pub fn last_files(&self) -> Option<Vec<PathBuf>> {
        self.last_files.clone()
    }

    /// Whether successfully loaded files have been remembered.
    pub fn has_last_files(&self) -> bool {
        self.last_files.is_some()
    }

    /// Remembers the last successfully loaded files.
    pub fn set_last_files(&mut self, files: Option<Vec<PathBuf>>) {
        self.last_files = files;
    }

    /// Whether the backend should bake exported animations.
    pub fn anim_bake(&self) -> bool {
        self.anim_bake
    }

    /// Sets whether the backend should bake exported animations.
    pub fn set_anim_bake(&mut self, value: bool) {
        self.anim_bake = value;
    }

    /// Whether the backend should treat all assets as raw files.
    pub fn force_raw_files(&self) -> bool {
        self.force_raw_files
    }

    /// Sets whether the backend should treat all assets as raw files.
    pub fn set_force_raw_files(&mut self, value: bool) {
        self.force_raw_files = value;
    }

    /// Checks whether or not the new settings requires a reload.
    pub fn reload_required(&self, new_settings: &Self) -> bool {
        if self.load_models() != new_settings.load_models()
            || self.load_animations() != new_settings.load_animations()
            || self.load_images() != new_settings.load_images()
            || self.load_materials() != new_settings.load_materials()
            || self.load_sounds() != new_settings.load_sounds()
            || self.load_maps() != new_settings.load_maps()
            || self.load_raw_files() != new_settings.load_raw_files()
            || self.force_raw_files() != new_settings.force_raw_files()
        {
            return true;
        }

        false
    }

    /// Whether or not to load models.
    pub fn load_models(&self) -> bool {
        self.load_settings.contains(LoadSettings::LOAD_MODELS)
    }

    /// Sets whether or not to load models.
    pub fn set_load_models(&mut self, value: bool) {
        self.load_settings.set(LoadSettings::LOAD_MODELS, value);
    }

    /// Whether or not to load images.
    pub fn load_images(&self) -> bool {
        self.load_settings.contains(LoadSettings::LOAD_IMAGES)
    }

    /// Sets whether or not to load images.
    pub fn set_load_images(&mut self, value: bool) {
        self.load_settings.set(LoadSettings::LOAD_IMAGES, value)
    }

    /// Whether or not to load materials.
    pub fn load_materials(&self) -> bool {
        self.load_settings.contains(LoadSettings::LOAD_MATERIALS)
    }

    /// Sets whether or not to load materials.
    pub fn set_load_materials(&mut self, value: bool) {
        self.load_settings.set(LoadSettings::LOAD_MATERIALS, value)
    }

    /// Whether or not to load animations.
    pub fn load_animations(&self) -> bool {
        self.load_settings.contains(LoadSettings::LOAD_ANIMATIONS)
    }

    /// Sets whether or not to load animations.
    pub fn set_load_animations(&mut self, value: bool) {
        self.load_settings.set(LoadSettings::LOAD_ANIMATIONS, value)
    }

    /// Whether or not to load sounds.
    pub fn load_sounds(&self) -> bool {
        self.load_settings.contains(LoadSettings::LOAD_SOUNDS)
    }

    /// Sets whether or not to load sounds.
    pub fn set_load_sounds(&mut self, value: bool) {
        self.load_settings.set(LoadSettings::LOAD_SOUNDS, value)
    }

    /// Whether or not to load maps.
    pub fn load_maps(&self) -> bool {
        self.load_settings.contains(LoadSettings::LOAD_MAPS)
    }

    /// Sets whether or not to load maps.
    pub fn set_load_maps(&mut self, value: bool) {
        self.load_settings.set(LoadSettings::LOAD_MAPS, value)
    }

    /// Whether or not to export maps as cast worlds.
    pub fn map_export_cast(&self) -> bool {
        self.map_settings.contains(MapSettings::EXPORT_CAST)
    }

    /// Sets whether or not to export maps as cast worlds.
    pub fn set_map_export_cast(&mut self, value: bool) {
        self.map_settings.set(MapSettings::EXPORT_CAST, value)
    }

    /// Whether or not to load raw files.
    pub fn load_raw_files(&self) -> bool {
        self.load_settings.contains(LoadSettings::LOAD_RAW_FILES)
            || (cfg!(feature = "raw-files-forcible") && self.force_raw_files())
    }

    /// Sets whether or not to load raw files.
    pub fn set_load_raw_files(&mut self, value: bool) {
        self.load_settings.set(LoadSettings::LOAD_RAW_FILES, value)
    }

    /// The model file types to export to.
    pub fn model_file_types(&self) -> Vec<ModelFileType> {
        let mut result = Vec::with_capacity(8);

        if self.model_settings.contains(ModelSettings::EXPORT_OBJ) {
            result.push(ModelFileType::Obj);
        }

        if self.model_settings.contains(ModelSettings::EXPORT_SMD) {
            result.push(ModelFileType::Smd);
        }

        if self.model_settings.contains(ModelSettings::EXPORT_XNA_LARA) {
            result.push(ModelFileType::XnaLara);
        }

        if self
            .model_settings
            .contains(ModelSettings::EXPORT_XMODEL_EXPORT)
        {
            result.push(ModelFileType::XModelExport);
        }

        if self.model_settings.contains(ModelSettings::EXPORT_CAST) {
            result.push(ModelFileType::Cast);
        }

        if self.model_settings.contains(ModelSettings::EXPORT_MAYA) {
            result.push(ModelFileType::Maya);
        }

        if self.model_settings.contains(ModelSettings::EXPORT_FBX) {
            result.push(ModelFileType::Fbx);
        }

        result
    }

    /// Sets whether or not a model file type is in use.
    pub fn set_model_file_type(&mut self, file_type: ModelFileType, value: bool) {
        let flag = match file_type {
            ModelFileType::Obj => ModelSettings::EXPORT_OBJ,
            ModelFileType::Smd => ModelSettings::EXPORT_SMD,
            ModelFileType::XnaLara => ModelSettings::EXPORT_XNA_LARA,
            ModelFileType::XModelExport => ModelSettings::EXPORT_XMODEL_EXPORT,
            ModelFileType::Cast => ModelSettings::EXPORT_CAST,
            ModelFileType::Maya => ModelSettings::EXPORT_MAYA,
            ModelFileType::Fbx => ModelSettings::EXPORT_FBX,
        };

        self.model_settings.set(flag, value);
    }

    /// The animation file types to export to.
    pub fn anim_file_types(&self) -> Vec<AnimationFileType> {
        let mut result = Vec::with_capacity(1);

        if self.anim_settings.contains(AnimSettings::EXPORT_CAST) {
            result.push(AnimationFileType::Cast);
        }

        result
    }

    /// Sets whether or not an anim file type is in use.
    pub fn set_anim_file_type(&mut self, file_type: AnimationFileType, value: bool) {
        let flag = match file_type {
            AnimationFileType::Cast => AnimSettings::EXPORT_CAST,
        };

        self.anim_settings.set(flag, value);
    }

    /// The audio file types to export to.
    pub fn audio_file_types(&self) -> Vec<AudioFileType> {
        let mut result = Vec::with_capacity(3);

        if self.audio_settings.contains(AudioSettings::EXPORT_WAV) {
            result.push(AudioFileType::Wav);
        }

        if self.audio_settings.contains(AudioSettings::EXPORT_FLAC) {
            result.push(AudioFileType::Flac);
        }

        result
    }

    /// Sets whether or not an audio file type is in use.
    pub fn set_audio_file_type(&mut self, file_type: AudioFileType, value: bool) {
        let flag = match file_type {
            AudioFileType::Wav => AudioSettings::EXPORT_WAV,
            AudioFileType::Flac => AudioSettings::EXPORT_FLAC,
            AudioFileType::Ogg => {
                // We don't support writing these formats.
                return;
            }
        };

        self.audio_settings.set(flag, value);
    }

    /// The image file type to export to.
    pub fn image_file_type(&self) -> ImageFileType {
        self.image_file_type
    }

    /// Sets the image file type to export to.
    pub fn set_image_file_type(&mut self, file_type: ImageFileType) {
        self.image_file_type = file_type;
    }

    /// The image normal map processing technique.
    pub fn image_normal_map_processing(&self) -> ImageNormalMapProcessing {
        self.image_normal_map_processing
    }

    /// Sets the image normal map processing.
    pub fn set_image_normal_map_processing(&mut self, processing: ImageNormalMapProcessing) {
        self.image_normal_map_processing = processing;
    }

    /// An output directory used to save assets.
    pub fn output_directory(&self) -> PathBuf {
        if let Some(output_directory) = self.output_directory.clone() {
            return output_directory;
        }

        // We need to make sure that we combine with the exe path.
        // Otherwise we'll have such a bad time, because people are dumb.
        if cfg!(target_os = "windows") {
            if cfg!(debug_assertions) {
                PathBuf::from("./exported_files")
            } else {
                static CURRENT_PATH: OnceLock<PathBuf> = OnceLock::new();

                CURRENT_PATH
                    .get_or_init(|| std::env::current_exe().unwrap_or_default())
                    .parent()
                    .unwrap_or(Path::new("./"))
                    .join("exported_files")
            }
        } else if let Some(user_dirs) = UserDirs::new() {
            match user_dirs.document_dir() {
                Some(path) => path.join("exported_files"),
                None => PathBuf::from("~/Documents/exported_files"),
            }
        } else {
            PathBuf::from("~/Documents/exported_files")
        }
    }

    /// Sets a new output directory.
    pub fn set_output_directory(&mut self, path: PathBuf) {
        self.output_directory = Some(path);
    }

    /// Gets the preview control scheme.
    pub fn preview_controls(&self) -> PreviewControlScheme {
        self.preview_controls
    }

    /// Sets the preview control scheme.
    pub fn set_preview_controls(&mut self, controls: PreviewControlScheme) {
        self.preview_controls = controls;
    }

    /// Whether or not to show the preview overlay hints.
    pub fn preview_overlay(&self) -> bool {
        self.preview_overlay
    }

    /// Sets whether or not to show the preview overlay.
    pub fn set_preview_overlay(&mut self, value: bool) {
        self.preview_overlay = value;
    }

    /// Whether or not LODs are exported.
    pub fn export_lods(&self) -> bool {
        self.lod_export
    }

    /// Sets whether or not to export available LODs.
    pub fn set_export_lods(&mut self, value: bool) {
        self.lod_export = value;
    }

    /// Whether or not to skip existing items.
    pub fn skip_previously_exported(&self) -> bool {
        self.skip_previously_exported
    }

    /// Sets whether or not to skip existing items.
    pub fn set_skip_previously_exported(&mut self, value: bool) {
        self.skip_previously_exported = value;
    }

    /// Whether or not to export material info.
    pub fn export_material_info(&self) -> bool {
        self.export_material_info
    }

    /// Sets whether or not to export material info.
    pub fn set_export_material_info(&mut self, value: bool) {
        self.export_material_info = value;
    }

    /// The model material processing.
    pub fn model_material_processing(&self) -> ModelMaterialProcessing {
        self.model_material_processing
    }

    /// Sets the model material processing.
    pub fn set_model_material_processing(&mut self, processing: ModelMaterialProcessing) {
        self.model_material_processing = processing;
    }

    /// How the folders an asset is exported into are laid out.
    pub fn sound_path_processing(&self) -> SoundPathProcessing {
        self.sound_path_processing
    }

    /// Sets how the folders an asset is exported into are laid out.
    pub fn set_sound_path_processing(&mut self, processing: SoundPathProcessing) {
        self.sound_path_processing = processing;
    }

    /// Whether or not to strip directory from material path.
    pub fn strip_material_dir(&self) -> bool {
        self.strip_material_dir
    }

    /// Sets whether or not to strip directory from material path.
    pub fn set_strip_material_dir(&mut self, value: bool) {
        self.strip_material_dir = value;
    }

    /// Whether or not to log assets.
    pub fn log_assets(&self) -> bool {
        self.log_assets
    }

    /// Sets whether or not to log assets.
    pub fn set_log_assets(&mut self, value: bool) {
        self.log_assets = value;
    }

    /// Gets the far clip distance for preview.
    pub fn far_clip(&self) -> u32 {
        self.far_clip.clamp(10000, 1000000)
    }

    /// Sets the far clip distance for preview.
    pub fn set_far_clip(&mut self, far_clip: u32) {
        self.far_clip = far_clip;
    }

    /// Whether or not preview should open in a new window by default.
    pub fn preview_window(&self) -> bool {
        self.preview_window
    }

    /// Sets whether or not preview should open in a new window by default.
    pub fn set_preview_window(&mut self, value: bool) {
        self.preview_window = value;
    }

    /// Gets the custom auto scale value to use.
    pub fn custom_scale(&self) -> Option<f32> {
        self.custom_scale
    }

    /// Sets the custom auto scale value to use.
    pub fn set_custom_scale(&mut self, scale: Option<f32>) {
        self.custom_scale = scale;
    }

    /// Whether or not assets are scaled on export.
    pub fn auto_scale(&self) -> bool {
        self.auto_scale
    }

    /// Sets whether or not assets are scaled on export.
    pub fn set_auto_scale(&mut self, value: bool) {
        self.auto_scale = value;
    }

    /// Gets the scale factor to apply, falling back to the given default.
    ///
    /// A custom scale replaces the default rather than compounding with it. Zero or negative
    /// custom scales are ignored, so a stray value cannot flatten an export.
    pub fn auto_scale_factor(&self, default: f32) -> Option<f32> {
        self.auto_scale().then(|| {
            self.custom_scale()
                .filter(|scale| *scale > 0.0)
                .unwrap_or(default)
        })
    }

    /// Update settings and returns a copy.
    pub fn update<F: FnOnce(&mut Self)>(&self, callback: F) -> Self {
        let mut settings = self.clone();

        callback(&mut settings);

        settings
    }

    /// Whether or not to build blend shapes for supported games.
    pub fn build_blend_shapes(&self) -> bool {
        self.build_blend_shapes
    }

    /// Sets whether or not to build blend shapes for supported games.
    pub fn set_build_blend_shapes(&mut self, value: bool) {
        self.build_blend_shapes = value;
    }

    /// Whether or not to focus on model in previewer automatically.
    pub fn auto_focus_on_model(&self) -> bool {
        self.auto_focus_on_model
    }

    /// Sets whether or not to focus on model in previewer automatically.
    pub fn set_auto_focus_on_model(&mut self, value: bool) {
        self.auto_focus_on_model = value;
    }

    /// Gets the volume level for audio preview.
    pub fn volume(&self) -> u32 {
        self.volume
    }

    /// Sets the volume level for audio preview.
    pub fn set_volume(&mut self, volume: u32) {
        self.volume = volume.clamp(0, 50);
    }

    /// Whether or not to load sounds.
    pub fn always_on_top(&self) -> bool {
        self.always_on_top
    }

    /// Sets whether or not to load sounds.
    pub fn set_always_on_top(&mut self, value: bool) {
        self.always_on_top = value
    }

    /// The theme variant to render the app with.
    pub fn theme(&self) -> ThemeVariant {
        self.theme
    }

    /// Sets the theme variant to render the app with.
    pub fn set_theme(&mut self, theme: ThemeVariant) {
        self.theme = theme;
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            shared_settings_version: 1,
            load_settings: LoadSettings::LOAD_MODELS,
            model_settings: ModelSettings::EXPORT_CAST,
            anim_settings: AnimSettings::EXPORT_CAST,
            audio_settings: AudioSettings::EXPORT_WAV,
            image_file_type: ImageFileType::Png,
            image_normal_map_processing: ImageNormalMapProcessing::None,
            output_directory: None,
            preview_controls: PreviewControlScheme::Simple,
            preview_overlay: true,
            far_clip: 10000,
            preview_window: false,
            custom_scale: None,
            lod_export: false,
            export_material_info: true,
            strip_material_dir: false,
            log_assets: false,
            skip_previously_exported: true,
            build_blend_shapes: true,
            auto_focus_on_model: true,
            volume: 30,
            always_on_top: false,
            theme: ThemeVariant::default(),
            map_settings: MapSettings::default(),
            model_material_processing: default_model_material_processing(),
            sound_path_processing: default_sound_path_processing(),
            auto_scale: default_auto_scale(),
            last_files: None,
            anim_bake: false,
            force_raw_files: false,
        }
    }
}
