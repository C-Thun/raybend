//! Realtime global adjustment resources: update a 192 KiB table, retain the source.
use crate::develop::working::{GPU_TONE_ROWS, GPU_TONE_SAMPLES, GpuToneDescription};
use std::sync::Arc;

pub(super) struct GpuTone {
    pub layout: wgpu::BindGroupLayout,
    pub resources: super::color_transform::TransformResources,
    pub source: Option<Arc<GpuToneDescription>>,
}

impl GpuTone {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let layout = super::color_transform::transform_layout(device, "raybend-tone-layout");
        let resources = build(device, queue, &layout, None);
        Self {
            layout,
            resources,
            source: None,
        }
    }
    pub fn set(
        &mut self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: Option<Arc<GpuToneDescription>>,
    ) {
        if match (&self.source, &source) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        } {
            return;
        }
        queue.write_buffer(&self.resources.uniform, 0, &config(source.as_deref()));
        if let Some(source) = &source {
            let pixels: Vec<u8> = source
                .tables
                .iter()
                .flatten()
                .flat_map(|v| half::f16::from_f32(*v).to_bits().to_le_bytes())
                .collect();
            let size = wgpu::Extent3d {
                width: GPU_TONE_SAMPLES as u32,
                height: GPU_TONE_ROWS as u32,
                depth_or_array_layers: 1,
            };
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.resources.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(size.width * 8),
                    rows_per_image: Some(GPU_TONE_ROWS as u32),
                },
                size,
            );
        }
        self.source = source;
    }
}

fn config(source: Option<&GpuToneDescription>) -> [u8; 64] {
    let mut config = [0_u8; 64];
    if let Some(source) = source {
        let vectors = [
            source.gains,
            [1.0, source.chroma.0, source.chroma.1],
            source.low_offsets,
            source.high_offsets,
        ];
        for (row, values) in vectors.iter().enumerate() {
            for (col, v) in values.iter().enumerate() {
                config[(row * 4 + col) * 4..(row * 4 + col + 1) * 4]
                    .copy_from_slice(&v.to_ne_bytes());
            }
        }
        config[28..32]
            .copy_from_slice(&(if source.linear_output { 1.0_f32 } else { 0.0 }).to_ne_bytes());
    }
    config
}

fn build(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    source: Option<&GpuToneDescription>,
) -> super::color_transform::TransformResources {
    let identity = vec![[0.0_f32, 0.0, 0.0, 1.0]; GPU_TONE_SAMPLES * GPU_TONE_ROWS];
    let samples = source.map_or(identity.as_slice(), |source| source.tables.as_slice());
    super::color_transform::transform_resources(
        device,
        queue,
        layout,
        &config(source),
        samples,
        (GPU_TONE_SAMPLES as u32, GPU_TONE_ROWS as u32),
        "raybend-tone",
    )
}
