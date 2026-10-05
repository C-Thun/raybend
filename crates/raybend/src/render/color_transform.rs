//! Device resources for the prepared display tail. Source texture is never
//! touched when the monitor changes. Layout/resources are rebuilt on recovery.
use crate::color::display::ScreenTransform;
use std::sync::Arc;
use wgpu::util::DeviceExt;

pub(super) struct GpuScreenTransform {
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub source: Option<Arc<ScreenTransform>>,
    pub output_scale: Option<f32>,
}

impl GpuScreenTransform {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let layout = layout(device, "raybend-display-layout", true);
        let bind_group = build(device, queue, &layout, None, None);
        Self {
            layout,
            bind_group,
            source: None,
            output_scale: None,
        }
    }

    pub fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: Option<Arc<ScreenTransform>>,
    ) {
        if match (&self.source, &source) {
            (None, None) => true,
            (Some(old), Some(new)) => Arc::ptr_eq(old, new),
            _ => false,
        } {
            return;
        }
        self.bind_group = build(
            device,
            queue,
            &self.layout,
            source.as_deref(),
            self.output_scale,
        );
        self.source = source;
    }
    pub fn set_output_scale(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scale: Option<f32>,
    ) {
        if self.output_scale == scale {
            return;
        }
        self.output_scale = scale;
        self.bind_group = build(device, queue, &self.layout, self.source.as_deref(), scale);
    }
}

fn build(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    source: Option<&ScreenTransform>,
    output_scale: Option<f32>,
) -> wgpu::BindGroup {
    let mut config = [0_u8; 64];
    if let Some(source) = source {
        for row in 0..3 {
            for col in 0..3 {
                config[(row * 4 + col) * 4..(row * 4 + col + 1) * 4]
                    .copy_from_slice(&source.matrix[row][col].to_ne_bytes());
            }
        }
        config[48..52].copy_from_slice(&1.0_f32.to_ne_bytes());
        if let Some(clut) = &source.clut {
            config[12..16].copy_from_slice(&clut.lab_scale.to_ne_bytes());
            config[48..52].copy_from_slice(&(if clut.lab { 3.0_f32 } else { 2.0 }).to_ne_bytes());
        }
        if let Some(native)=&source.native {config[48..52].copy_from_slice(&(if native.lab {5.0_f32}else{4.0}).to_ne_bytes());}
        if source.identity_compensation {
            config[60..64].copy_from_slice(&1.0_f32.to_ne_bytes());
        }
    }
    if let Some(scale) = output_scale {
        config[52..56].copy_from_slice(&1.0_f32.to_ne_bytes());
        config[56..60].copy_from_slice(&scale.to_ne_bytes());
    }
    let identity = [[0.0_f32, 0.0, 0.0, 1.0]];
    let samples = source.filter(|s| s.clut.is_none() && s.native.is_none()).map_or(identity.as_slice(), |source| source.compensation.as_slice());
    resources(
        device,
        queue,
        layout,
        &config,
        samples,
        (samples.len() as u32, 1),
        "raybend-display",
        Some(source.and_then(|s| s.clut.as_ref())),
        source.and_then(|s|s.native.as_ref()),
    )
    .bind_group
}

pub(super) struct TransformResources {
    pub uniform: wgpu::Buffer,
    pub texture: wgpu::Texture,
    pub bind_group: wgpu::BindGroup,
}

pub(super) fn transform_resources(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    config: &[u8],
    samples: &[[f32; 4]],
    dimensions: (u32, u32),
    label: &str,
) -> TransformResources {
    resources(device, queue, layout, config, samples, dimensions, label, None, None)
}

#[allow(clippy::too_many_arguments)]
fn resources(
    device: &wgpu::Device, queue: &wgpu::Queue, layout: &wgpu::BindGroupLayout,
    config: &[u8], samples: &[[f32;4]], dimensions: (u32,u32), label: &str,
    clut: Option<Option<&crate::color::display_clut::DisplayClut>>,
    native: Option<&crate::color::display_native::NativeDisplay>,
) -> TransformResources {
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: config,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let pixels: Vec<u8> = samples
        .iter()
        .flatten()
        .flat_map(|v| half::f16::from_f32(*v).to_bits().to_le_bytes())
        .collect();
    let size = wgpu::Extent3d {
        width: dimensions.0,
        height: dimensions.1,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size.width * 8),
            rows_per_image: Some(size.height),
        },
        size,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let clut_view = clut.map(|source| {
        let edge = source.map_or(1, |s| s.edge);
        let cube = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("raybend-display-clut"), size: wgpu::Extent3d { width: edge, height: edge, depth_or_array_layers: edge },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
        });
        let dummy = [[0.0,0.0,0.0,1.0]];
        let samples = source.map_or(dummy.as_slice(), |s| s.samples.as_slice());
        // A slice at a time bounds CPU packing to 129² × 8 bytes.
        for (z, slice) in samples.chunks((edge*edge) as usize).enumerate() {
            let pixels: Vec<u8> = slice.iter().flatten().flat_map(|v| half::f16::from_f32(*v).to_bits().to_le_bytes()).collect();
            queue.write_texture(wgpu::TexelCopyTextureInfo { texture: &cube, mip_level: 0, origin: wgpu::Origin3d { x:0,y:0,z:z as u32 }, aspect: wgpu::TextureAspect::All },
                &pixels, wgpu::TexelCopyBufferLayout { offset:0,bytes_per_row:Some(edge*8),rows_per_image:Some(edge) },
                wgpu::Extent3d { width:edge,height:edge,depth_or_array_layers:1 });
        }
        cube.create_view(&Default::default())
    });
    let native_uniform= device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label:Some("raybend-native-icc"),contents:&native.map_or_else(||vec![0;352],|n|n.uniform()),usage:wgpu::BufferUsages::UNIFORM,
    });
    let dims=native.map_or([1;3],|n|n.dimensions);
    let native_texture=device.create_texture(&wgpu::TextureDescriptor {
        label:Some("raybend-native-icc-clut"),size:wgpu::Extent3d {width:dims[0],height:dims[1],depth_or_array_layers:dims[2]},mip_level_count:1,sample_count:1,
        dimension:wgpu::TextureDimension::D3,format:wgpu::TextureFormat::Rgba32Float,
        usage:wgpu::TextureUsages::TEXTURE_BINDING|wgpu::TextureUsages::COPY_DST,view_formats:&[],
    });
    let dummy=[[0.0,0.0,0.0,1.0]];
    let table=native.map_or(dummy.as_slice(),|n|n.samples.as_slice());
    for (z,slice) in table.chunks((dims[0]*dims[1]) as usize).enumerate() {
        let bytes:Vec<u8>=slice.iter().flatten().flat_map(|v|v.to_ne_bytes()).collect();
        queue.write_texture(wgpu::TexelCopyTextureInfo {texture:&native_texture,mip_level:0,origin:wgpu::Origin3d {x:0,y:0,z:z as u32},aspect:wgpu::TextureAspect::All},
            &bytes,wgpu::TexelCopyBufferLayout {offset:0,bytes_per_row:Some(dims[0]*16),rows_per_image:Some(dims[1])},wgpu::Extent3d {width:dims[0],height:dims[1],depth_or_array_layers:1});
    }
    let native_view=native_texture.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor { label:Some("raybend-display-linear"),mag_filter:wgpu::FilterMode::Linear,min_filter:wgpu::FilterMode::Linear,..Default::default() });
    let mut entries = vec![
        wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
    ];
    if let Some(view) = &clut_view { entries.push(wgpu::BindGroupEntry { binding:2, resource: wgpu::BindingResource::TextureView(view) }); entries.push(wgpu::BindGroupEntry {binding:3,resource:wgpu::BindingResource::Sampler(&sampler)});
        entries.push(wgpu::BindGroupEntry {binding:4,resource:native_uniform.as_entire_binding()});
        entries.push(wgpu::BindGroupEntry {binding:5,resource:wgpu::BindingResource::TextureView(&native_view)}); }
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &entries,
    });
    TransformResources {
        uniform,
        texture,
        bind_group,
    }
}

pub(super) fn transform_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    layout(device, label, false)
}
fn layout(device: &wgpu::Device, label: &str, clut: bool) -> wgpu::BindGroupLayout {
    let mut entries = vec![
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ];
    if clut { entries.push(wgpu::BindGroupLayoutEntry { binding:2, visibility:wgpu::ShaderStages::FRAGMENT,
        ty:wgpu::BindingType::Texture { sample_type:wgpu::TextureSampleType::Float { filterable:true }, view_dimension:wgpu::TextureViewDimension::D3,multisampled:false },count:None });
        entries.push(wgpu::BindGroupLayoutEntry {binding:3,visibility:wgpu::ShaderStages::FRAGMENT,ty:wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),count:None});
        entries.push(wgpu::BindGroupLayoutEntry {binding:4,visibility:wgpu::ShaderStages::FRAGMENT,ty:wgpu::BindingType::Buffer {ty:wgpu::BufferBindingType::Uniform,has_dynamic_offset:false,min_binding_size:None},count:None});
        entries.push(wgpu::BindGroupLayoutEntry {binding:5,visibility:wgpu::ShaderStages::FRAGMENT,ty:wgpu::BindingType::Texture {sample_type:wgpu::TextureSampleType::Float {filterable:false},view_dimension:wgpu::TextureViewDimension::D3,multisampled:false},count:None}); }
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label:Some(label),entries:&entries })
}
