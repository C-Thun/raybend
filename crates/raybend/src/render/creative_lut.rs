//! **创意 LUT 的 GPU 资源**（搬自 `develop/lut.rs` 的逐像素循环）。
//!
//! 一支 `.cube` / HaldCLUT 在 GPU 上就是一张 **3D 纹理** + 一张 **1D shaper**（可选）：
//! 三线性由纹理单元做，着色器只负责「编码域进出 + 超域残差」，逻辑与 CPU 的
//! [`Lut::eval_extended`](crate::develop::lut::Lut::eval_extended) 逐条对齐
//! （见 `spike.wgsl::lut_encoded`）。
//!
//! ## 为什么挂在 **group 0** 而不是再开一个 group
//!
//! WebGPU 的 `max_bind_groups` 默认就是 4（0..3）。再开 `group(4)` 会直接
//! `create_shader_module` 失败（第一次跑离屏就撞上了），而抬这个上限等于赌
//! 所有目标显卡的驱动都支持 —— 这不是本地桌面软件该赌的东西。
//!
//! 挂在 group 0（图像绑定组）反而更对：**「这张图有没有 LUT」本来就是这张图的事实**。
//! 参照帧（对比左侧）与主图各有自己的绑定组，于是「参照帧不套 LUT」不需要任何运行时开关，
//! 也不可能出现「主图换了 LUT、参照帧跟着变」。
//!
//! 三条与既有资源同一套的规矩（`color_transform.rs` / `tone.rs` 先例）：
//!
//! * **`Arc` 指针相等就跳过重建**：选 LUT 是低频动作，拖滑杆是高频动作 —— 高频那条路必须零上传；
//! * **绑定组与设备同寿命**：设备丢失后整套重建（`recover()`），不能拿旧设备的 view 建组；
//! * **始终有一份合法的「关闭」资源**：参照帧与「CPU 已烘焙 LUT 的帧」都用它。
use crate::develop::lut::Lut;
use std::sync::Arc;

/// uniform 字节数（3 个 `vec4` 共 48，向上对齐到 16 的倍数；留一点给多层链的扩展）。
const UNIFORM_BYTES: usize = 64;

/// 一套可以被多个绑定组共用的句柄：**同一份数据**，主图与参照帧只是选择不同的那一套。
pub(super) struct LutResources {
    pub uniform: wgpu::Buffer,
    /// 纹理本体显式持有（视图自己也会保活，但显式持有不容易被误读成「可以提前释放」）。
    #[allow(dead_code)]
    pub cube: wgpu::Texture,
    pub cube_view: wgpu::TextureView,
    #[allow(dead_code)]
    pub shaper: wgpu::Texture,
    pub shaper_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

pub(super) struct GpuLut {
    /// 「不套 LUT」那一套（1×1×1 占位 + `enabled = 0`）。
    disabled: LutResources,
    /// 当前选中的那一支；`None` 时用 [`Self::disabled`]。
    active: Option<LutResources>,
    pub source: Option<Arc<Lut>>,
}

impl GpuLut {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            disabled: build(device, queue, None),
            active: None,
            source: None,
        }
    }

    /// 主图绑定组该用的那一套。
    pub fn resources(&self) -> &LutResources {
        self.active.as_ref().unwrap_or(&self.disabled)
    }

    /// **不套**创意 LUT 的那一套：参照帧与 CPU 烘焙帧用。
    pub fn disabled_resources(&self) -> &LutResources {
        &self.disabled
    }

    /// 换一支 LUT（`None` = 关闭）。返回「资源是否变了」—— 变了就要重建绑定组。
    /// 同一份 `Arc` 不重建（拖滑杆时每帧都会调到这里）。
    pub fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: Option<Arc<Lut>>,
    ) -> bool {
        if match (&self.source, &source) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        } {
            return false;
        }
        self.active = source
            .as_deref()
            .map(|lut| build(device, queue, Some(lut)));
        self.source = source;
        true
    }
}

/// 建一套句柄（+ 上传表）。`None` = 「关闭」占位。
fn build(device: &wgpu::Device, queue: &wgpu::Queue, source: Option<&Lut>) -> LutResources {
    let (domain_min, domain_max) = source.map_or(([0.0; 3], [1.0; 3]), Lut::domain);
    let cube_size = source.map_or(0, Lut::cube_size);
    let shaper = source.map_or(&[][..], Lut::shaper);

    let mut config = [0_u8; UNIFORM_BYTES];
    let mut write = |offset: usize, values: [f32; 4]| {
        for (index, value) in values.iter().enumerate() {
            let at = offset + index * 4;
            config[at..at + 4].copy_from_slice(&value.to_ne_bytes());
        }
    };
    // row0 = domain_min + enabled；row1 = domain_max + strength（当前恒为 1，留给多层链）；
    // row2 = (has_cube, has_shaper, 0, 0)。字段顺序与 `spike.wgsl::CreativeLut` 一字不差。
    write(
        0,
        [
            domain_min[0],
            domain_min[1],
            domain_min[2],
            if source.is_some() { 1.0 } else { 0.0 },
        ],
    );
    write(16, [domain_max[0], domain_max[1], domain_max[2], 1.0]);
    write(
        32,
        [
            if cube_size > 0 { 1.0 } else { 0.0 },
            if shaper.is_empty() { 0.0 } else { 1.0 },
            0.0,
            0.0,
        ],
    );

    let cube = upload_cube(device, queue, source, cube_size);
    let shaper_texture = upload_shaper(device, queue, shaper);
    let cube_view = cube.create_view(&wgpu::TextureViewDescriptor::default());
    let shaper_view = shaper_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("raybend-creative-lut"),
        size: UNIFORM_BYTES as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: true,
    });
    {
        uniform
            .slice(..)
            .get_mapped_range_mut()
            .expect("new mapped buffer")
            .copy_from_slice(&config);
    }
    uniform.unmap();
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("raybend-creative-lut-sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        // 表外一律夹取：CPU 的 `eval` 也是先 clamp 到输入域再查表。
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        ..Default::default()
    });
    LutResources {
        uniform,
        cube,
        cube_view,
        shaper: shaper_texture,
        shaper_view,
        sampler,
    }
}

fn upload_cube(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: Option<&Lut>,
    cube_size: usize,
) -> wgpu::Texture {
    let edge = u32::try_from(cube_size).unwrap_or(1).max(1);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("raybend-creative-lut-cube"),
        size: wgpu::Extent3d {
            width: edge,
            height: edge,
            depth_or_array_layers: edge,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let dummy = [[0.0_f32, 0.0, 0.0]];
    let samples = source.map_or(dummy.as_slice(), Lut::cube);
    // 一个 z 切片一写：33³ 一次打包 431 KiB、65³ 一次 3.3 MiB，按片写把峰值夹在 1/n。
    for (z, slice) in samples.chunks((edge * edge) as usize).enumerate() {
        let pixels: Vec<u8> = slice
            .iter()
            .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 1.0])
            .flat_map(|value| half::f16::from_f32(value).to_bits().to_le_bytes())
            .collect();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: z as u32,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(edge * 8),
                rows_per_image: Some(edge),
            },
            wgpu::Extent3d {
                width: edge,
                height: edge,
                depth_or_array_layers: 1,
            },
        );
    }
    texture
}

fn upload_shaper(device: &wgpu::Device, queue: &wgpu::Queue, shaper: &[[f32; 3]]) -> wgpu::Texture {
    let width = u32::try_from(shaper.len()).unwrap_or(1).max(1);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("raybend-creative-lut-shaper"),
        size: wgpu::Extent3d {
            width,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let dummy = [[0.0_f32, 0.0, 0.0]];
    let samples = if shaper.is_empty() {
        dummy.as_slice()
    } else {
        shaper
    };
    let pixels: Vec<u8> = samples
        .iter()
        .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 1.0])
        .flat_map(|value| half::f16::from_f32(value).to_bits().to_le_bytes())
        .collect();
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
            bytes_per_row: Some(width * 8),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    texture
}
