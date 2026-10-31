use std::collections::HashMap;
use std::sync::Arc;

use porter_utils::HashExt;

use wgpu::util::*;
use wgpu::*;

use porter_texture::Image;
use porter_texture::ImageFormat;

use porter_utils::SliceExt;

use crate::MaterialSettings;
use crate::PreviewError;
use crate::gpu::ViewportContext;
use crate::{NormalMap, NormalMapEncoding};

/// An image uploaded to the gpu with its texture bind group.
pub(crate) struct PreviewTexture {
    bind_group: BindGroup,
    pub width: u32,
    pub height: u32,
    pub srgb: bool,
    pub grayscale: bool,
}

impl PreviewTexture {
    /// Uploads the given image, using repeat addressing for mesh materials
    /// and clamped addressing for 2d previews.
    pub fn from_image(
        context: &ViewportContext,
        image: &Image,
        repeat: bool,
    ) -> Result<Self, PreviewError> {
        Self::with_settings(context, image, repeat, MaterialSettings::default())
    }

    pub fn with_settings(
        context: &ViewportContext,
        image: &Image,
        repeat: bool,
        settings: MaterialSettings,
    ) -> Result<Self, PreviewError> {
        Self::with_normal_map(context, image, None, repeat, settings)
    }

    fn with_normal_map(
        context: &ViewportContext,
        image: &Image,
        normal_map: Option<&NormalMap>,
        repeat: bool,
        settings: MaterialSettings,
    ) -> Result<Self, PreviewError> {
        let format = image.format();
        if normal_map.is_some_and(|map| !map.strength.is_finite()) {
            return Err(PreviewError::InvalidAsset);
        }
        let (view, wgpu_format) = upload_image(context, image)?;
        let normal_view = normal_map
            .map(|map| upload_image(context, &map.image).map(|(view, _)| view))
            .transpose()?;

        let address_mode = if repeat {
            AddressMode::Repeat
        } else {
            AddressMode::ClampToEdge
        };

        let sampler = context.device().create_sampler(&SamplerDescriptor {
            address_mode_u: address_mode,
            address_mode_v: address_mode,
            address_mode_w: address_mode,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..Default::default()
        });

        let values = settings.tint;
        let flags = [
            u32::from(settings.unlit),
            u32::from(settings.alpha_test),
            u32::from(settings.vertex_color),
            settings.alpha_cutoff.to_bits(),
        ];
        let mut uniform = Vec::from(values.as_slice().as_this_slice::<u8>());
        uniform.extend_from_slice(flags.as_slice().as_this_slice());
        let normal_values = normal_map
            .map(|map| {
                [
                    map.strength.to_bits(),
                    map.encoding as u32,
                    u32::from(map.invert_y),
                    1,
                ]
            })
            .unwrap_or([1.0f32.to_bits(), 0, 0, 0]);
        uniform.extend_from_slice(normal_values.as_slice().as_this_slice());
        let buffer = context.device().create_buffer_init(&BufferInitDescriptor {
            label: Some("preview material"),
            contents: &uniform,
            usage: BufferUsages::UNIFORM,
        });

        let bind_group = context.device().create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &context.texture_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&sampler),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: BindingResource::TextureView(normal_view.as_ref().unwrap_or(&view)),
                },
            ],
        });

        Ok(Self {
            bind_group,
            width: image.width(),
            height: image.height(),
            srgb: format.is_srgb(),
            grayscale: wgpu_format.components() == 1,
        })
    }

    /// Creates the fallback 4x4 gray texture used when a material has no
    /// usable image.
    pub fn fallback(context: &ViewportContext) -> Result<Self, PreviewError> {
        Self::fallback_with_settings(context, MaterialSettings::default())
    }

    pub fn fallback_with_settings(
        context: &ViewportContext,
        settings: MaterialSettings,
    ) -> Result<Self, PreviewError> {
        Self::fallback_with_normal_map(context, None, settings)
    }

    pub fn fallback_with_normal_map(
        context: &ViewportContext,
        normal_map: Option<&NormalMap>,
        settings: MaterialSettings,
    ) -> Result<Self, PreviewError> {
        let mut image =
            Image::new(4, 4, ImageFormat::R8G8B8A8Unorm).map_err(|_| PreviewError::InvalidAsset)?;

        image
            .create_frame()
            .map_err(|_| PreviewError::OutOfMemory)?
            .buffer_mut()
            .copy_from_slice(
                [if settings.unlit {
                    0xFFFFFFFFu32
                } else {
                    0xFFA1A1A1u32
                }; 4 * 4]
                    .as_slice()
                    .as_this_slice(),
            );

        Self::with_normal_map(context, &image, normal_map, true, settings)
    }

    /// The texture bind group.
    pub fn bind_group(&self) -> &BindGroup {
        &self.bind_group
    }
}

fn upload_image(
    context: &ViewportContext,
    image: &Image,
) -> Result<(TextureView, TextureFormat), PreviewError> {
    if image.format().is_int() {
        return Err(PreviewError::Unsupported);
    }
    let format = image
        .format()
        .to_wgpu()
        .map_err(|_| PreviewError::Unsupported)?;
    let frame = image.frames().first().ok_or(PreviewError::InvalidAsset)?;
    let texture = context.device().create_texture_with_data(
        context.queue(),
        &TextureDescriptor {
            label: None,
            size: Extent3d {
                width: image.width(),
                height: image.height(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        },
        TextureDataOrder::LayerMajor,
        frame.buffer(),
    );
    Ok((texture.create_view(&Default::default()), format))
}

#[derive(Debug, Hash, PartialEq, Eq)]
struct NormalMapKey {
    pixels: u64,
    width: u32,
    height: u32,
    format: u32,
    strength: u32,
    encoding: NormalMapEncoding,
    invert_y: bool,
}

impl NormalMapKey {
    fn new(map: &NormalMap) -> Self {
        Self {
            pixels: map
                .image
                .frames()
                .first()
                .map(|frame| frame.buffer().hash_xxh364())
                .unwrap_or_default(),
            width: map.image.width(),
            height: map.image.height(),
            format: map.image.format() as u32,
            strength: map.strength.to_bits(),
            encoding: map.encoding,
            invert_y: map.invert_y,
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
struct TextureKey {
    pixels: u64,
    width: u32,
    height: u32,
    format: u32,
    repeat: bool,
    flags: [bool; 3],
    tint: [u32; 4],
    alpha_cutoff: u32,
    normal_map: Option<NormalMapKey>,
}

impl TextureKey {
    fn new(
        image: &Image,
        normal_map: Option<&NormalMap>,
        repeat: bool,
        settings: MaterialSettings,
    ) -> Self {
        Self {
            pixels: image
                .frames()
                .first()
                .map(|frame| frame.buffer().hash_xxh364())
                .unwrap_or_default(),
            width: image.width(),
            height: image.height(),
            format: image.format() as u32,
            repeat,
            flags: [settings.unlit, settings.alpha_test, settings.vertex_color],
            tint: settings.tint.map(f32::to_bits),
            alpha_cutoff: settings.alpha_cutoff.to_bits(),
            normal_map: normal_map.map(NormalMapKey::new),
        }
    }
}

pub(crate) struct TextureCache {
    entries: HashMap<TextureKey, Arc<PreviewTexture>>,
}

impl TextureCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn get_or_upload(
        &mut self,
        context: &ViewportContext,
        image: &Image,
        normal_map: Option<&NormalMap>,
        repeat: bool,
        settings: MaterialSettings,
    ) -> Result<Arc<PreviewTexture>, PreviewError> {
        let key = TextureKey::new(image, normal_map, repeat, settings);
        if let Some(texture) = self.entries.get(&key) {
            return Ok(texture.clone());
        }
        let texture = Arc::new(PreviewTexture::with_normal_map(
            context, image, normal_map, repeat, settings,
        )?);
        self.entries.insert(key, texture.clone());
        Ok(texture)
    }

    pub fn prune(&mut self) {
        self.entries
            .retain(|_, texture| Arc::strong_count(texture) > 1);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
