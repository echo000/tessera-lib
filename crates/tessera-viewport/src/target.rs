use porter_texture::GpuExt;
use wgpu::*;

use crate::gpu::COLOR_FORMAT;
use crate::gpu::DEPTH_FORMAT;
use crate::gpu::SAMPLE_COUNT;
use crate::gpu::ViewportContext;

/// The offscreen render target: MSAA color, resolve texture, depth, and the
/// cpu readback buffer.
pub(crate) struct RenderTarget {
    pub width: u32,
    pub height: u32,
    msaa_view: TextureView,
    resolve_texture: Texture,
    resolve_view: TextureView,
    depth_view: TextureView,
    readback_buffer: Buffer,
}

impl RenderTarget {
    /// Creates a render target of the given pixel size.
    pub fn new(context: &ViewportContext, width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);

        let device = context.device();

        let msaa_texture = device.create_texture(&TextureDescriptor {
            label: Some("tessera-viewport msaa"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let resolve_texture = device.create_texture(&TextureDescriptor {
            label: Some("tessera-viewport resolve"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: TextureUsages::COPY_SRC | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let depth_texture = device.create_texture(&TextureDescriptor {
            label: Some("tessera-viewport depth"),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: SAMPLE_COUNT,
            dimension: TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let readback_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("tessera-viewport readback"),
            size: COLOR_FORMAT.buffer_size_aligned(width, height),
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            width,
            height,
            msaa_view: msaa_texture.create_view(&Default::default()),
            resolve_view: resolve_texture.create_view(&Default::default()),
            resolve_texture,
            depth_view: depth_texture.create_view(&Default::default()),
            readback_buffer,
        }
    }

    /// Begins the render pass for a frame.
    pub fn begin_pass<'a>(&'a self, encoder: &'a mut CommandEncoder) -> RenderPass<'a> {
        encoder.begin_render_pass(&RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &self.msaa_view,
                depth_slice: None,
                resolve_target: Some(&self.resolve_view),
                ops: Operations {
                    load: LoadOp::Clear(Color {
                        r: 0.066,
                        g: 0.066,
                        b: 0.066,
                        a: 1.0,
                    }),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(1.0),
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        })
    }

    /// Copies the resolved frame to the readback buffer and reads it back as
    /// tightly packed rgba pixels.
    pub fn read_back(&self, context: &ViewportContext, mut encoder: CommandEncoder) -> Vec<u8> {
        let bytes_per_row = COLOR_FORMAT.bytes_per_row(self.width);
        let aligned_bytes_per_row =
            bytes_per_row.next_multiple_of(COPY_BYTES_PER_ROW_ALIGNMENT) as usize;

        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &self.resolve_texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &self.readback_buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(
                        bytes_per_row.next_multiple_of(COPY_BYTES_PER_ROW_ALIGNMENT),
                    ),
                    rows_per_image: None,
                },
            },
            Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        let submission = context.queue().submit(Some(encoder.finish()));

        let slice = self.readback_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);

        slice.map_async(MapMode::Read, move |result| {
            let _ = tx.send(result);
        });

        let _ = context.device().poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        });

        if !matches!(rx.recv(), Ok(Ok(()))) {
            return Vec::new();
        }

        let Ok(buffer) = slice.get_mapped_range() else {
            self.readback_buffer.unmap();
            return Vec::new();
        };

        let tight_size = bytes_per_row as usize * self.height as usize;

        let pixels = if buffer.len() == tight_size {
            buffer.to_vec()
        } else {
            // Strip the row alignment padding.
            let mut result = vec![0; tight_size];

            for (i, row) in result.chunks_exact_mut(bytes_per_row as usize).enumerate() {
                let source = i * aligned_bytes_per_row;

                row.copy_from_slice(&buffer[source..source + bytes_per_row as usize]);
            }

            result
        };

        drop(buffer);

        self.readback_buffer.unmap();

        pixels
    }
}
