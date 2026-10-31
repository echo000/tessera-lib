use wgpu::util::*;
use wgpu::*;

use porter_model::MaterialUsage;

use porter_texture::Image;

use porter_utils::SliceExt;

use crate::PreviewError;
use crate::gpu::ImageVertex;
use crate::gpu::ViewportContext;
use crate::texture::PreviewTexture;

/// One image of an image/material preview: the uploaded texture and its quad.
struct ImagePlane {
    texture: PreviewTexture,
    vertex_buffer: Buffer,
    usage: MaterialUsage,
}

/// A 2d image preview; materials are a cyclable list of images.
pub(crate) struct ImagePreview {
    images: Vec<Result<ImagePlane, PreviewError>>,
    index: usize,
}

/// Builds the quad for an image, in pixel coordinates.
fn build_quad(context: &ViewportContext, width: f32, height: f32) -> Buffer {
    let vertex = |x: f32, y: f32, u: f32, v: f32| ImageVertex {
        position: [x, y, 0.0],
        uv: [u, v],
    };

    let vertices = [
        vertex(0.0, 0.0, 0.0, 0.0),
        vertex(width, 0.0, 1.0, 0.0),
        vertex(width, height, 1.0, 1.0),
        vertex(0.0, 0.0, 0.0, 0.0),
        vertex(width, height, 1.0, 1.0),
        vertex(0.0, height, 0.0, 1.0),
    ];

    context.device().create_buffer_init(&BufferInitDescriptor {
        label: None,
        contents: vertices.as_slice().as_this_slice(),
        usage: BufferUsages::VERTEX,
    })
}

impl ImagePreview {
    /// Creates a preview for a single image.
    pub fn from_image(context: &ViewportContext, image: &Image) -> Result<Self, PreviewError> {
        let texture = PreviewTexture::from_image(context, image, false)?;
        let vertex_buffer = build_quad(context, texture.width as f32, texture.height as f32);

        Ok(Self {
            images: vec![Ok(ImagePlane {
                texture,
                vertex_buffer,
                usage: MaterialUsage::Unknown,
            })],
            index: 0,
        })
    }

    /// Creates a preview for a material's images. Images that fail to upload
    /// keep their slot so the count and cycling stay consistent.
    pub fn from_material(
        context: &ViewportContext,
        material: &[(MaterialUsage, Image)],
    ) -> Result<Self, PreviewError> {
        if material.is_empty() {
            return Err(PreviewError::InvalidAsset);
        }

        let mut images: Vec<(MaterialUsage, Result<ImagePlane, PreviewError>)> = material
            .iter()
            .map(|(usage, image)| {
                let plane = PreviewTexture::from_image(context, image, false).map(|texture| {
                    let vertex_buffer =
                        build_quad(context, texture.width as f32, texture.height as f32);

                    ImagePlane {
                        texture,
                        vertex_buffer,
                        usage: *usage,
                    }
                });

                (*usage, plane)
            })
            .collect();

        // Show the most important usage first (Albedo orders lowest).
        images.sort_by_key(|(usage, _)| *usage);

        Ok(Self {
            images: images.into_iter().map(|(_, plane)| plane).collect(),
            index: 0,
        })
    }

    /// Cycles to the next image in the material.
    pub fn next(&mut self) {
        if !self.images.is_empty() {
            self.index = (self.index + 1) % self.images.len();
        }
    }

    /// The current image index.
    pub fn index(&self) -> usize {
        self.index
    }

    /// The number of images.
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// Whether the current image failed to upload.
    pub fn is_error(&self) -> bool {
        self.images
            .get(self.index)
            .map(|plane| plane.is_err())
            .unwrap_or(true)
    }

    /// The current image's (width, height), or None on error.
    pub fn size(&self) -> Option<(u32, u32)> {
        self.plane()
            .map(|plane| (plane.texture.width, plane.texture.height))
    }

    /// Whether the current image is in sRGB colorspace.
    pub fn srgb(&self) -> bool {
        self.plane()
            .map(|plane| plane.texture.srgb)
            .unwrap_or(false)
    }

    /// Whether the current image is grayscale.
    pub fn grayscale(&self) -> bool {
        self.plane()
            .map(|plane| plane.texture.grayscale)
            .unwrap_or(false)
    }

    /// The current image's usage label.
    pub fn usage(&self) -> String {
        self.plane()
            .map(|plane| plane.usage.to_string())
            .unwrap_or_default()
    }

    /// The current image plane, if it uploaded successfully.
    fn plane(&self) -> Option<&ImagePlane> {
        self.images
            .get(self.index)
            .and_then(|plane| plane.as_ref().ok())
    }

    /// Draws the current image.
    pub fn draw(&self, context: &ViewportContext, render_pass: &mut RenderPass<'_>) {
        let Some(plane) = self.plane() else {
            return;
        };

        render_pass.set_pipeline(&context.image_pipeline);
        render_pass.set_bind_group(1, plane.texture.bind_group(), &[]);
        render_pass.set_vertex_buffer(0, plane.vertex_buffer.slice(..));
        render_pass.draw(0..6, 0..1);
    }
}
