use crate::color::proof::ProofTransform;
use std::sync::Arc;
pub(super) struct GpuProof {
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    pub source: Option<Arc<ProofTransform>>,
    pub warning: bool,
}
impl GpuProof {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rgb-proof-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rgb-proof"),
            size: 112,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rgb-proof"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        queue.write_buffer(&uniform, 0, &[0; 112]);
        Self {
            layout,
            bind_group,
            uniform,
            source: None,
            warning: false,
        }
    }
    pub fn set(&mut self, queue: &wgpu::Queue, source: Option<Arc<ProofTransform>>, warning: bool) {
        if self.warning == warning
            && match (&self.source, &source) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
        {
            return;
        }
        let mut data = [0_f32; 28];
        if let Some(source) = &source {
            for row in 0..3 {
                for c in 0..3 {
                    data[row * 4 + c] = source.forward[row][c];
                    data[12 + row * 4 + c] = source.backward[row][c];
                }
            }
            data[24] = 1.0;
            data[25] = if warning { 1.0 } else { 0.0 };
        }
        let bytes: Vec<u8> = data.into_iter().flat_map(f32::to_ne_bytes).collect();
        queue.write_buffer(&self.uniform, 0, &bytes);
        self.source = source;
        self.warning = warning;
    }
}
