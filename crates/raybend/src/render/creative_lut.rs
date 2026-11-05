//! **创意 LUT 链的 GPU 资源**：整条链装进**一张 3D 图集** + 一张 shaper 数组。
//!
//! 一支 `.cube` / HaldCLUT 的本质是一张 3D 查找表；多层链就是"表的串联"。
//! GPU 侧不按层开纹理（会撞 `max_sampled_textures_per_shader_stage`），而是：
//!
//! * **3D 图集**：每层占一段 Z（slot = `max_n + 2` 片），X/Y/Z 各留 1 texel **边界复制**，
//!   于是三线性插值永远不会从一层"漏"到相邻层里；每层按自己的 `n` 求值。
//! * **shaper 数组**：`texture_2d_array`，layer = 层序号（`textureSampleLevel` 的数组下标
//!   是运行时值，所以 8 层共用**一个**绑定）。
//! * **uniform**：每层 3 个 `vec4`（域端点 / 尺寸+强度 / 偏移+shaper 长度+开关）。
//!
//! 设计口径与 CPU 完全一致（`develop/lut.rs::LutChain`）：
//! 编码域求值、`eval_extended` 的超域残差、`mix(输入, 映射, strength)`。
//!
//! 三条与既有资源同一套的规矩（`color_transform.rs` / `tone.rs` 先例）：
//!
//! * **`Arc` 指针相等就跳过重建**：选 LUT / 拖强度是低频动作，拖滑杆是高频动作；
//! * **绑定组与设备同寿命**：设备丢失后整套重建（`recover()`），不能拿旧设备的 view 建组；
//! * **始终有一份合法的「关闭」资源**：参照帧与「CPU 已烘焙 LUT 的帧」都用它。
use crate::develop::lut::LutChain;
use std::sync::Arc;

/// 链长上限（与 `store::develop::MAX_LUT_LAYERS` 同一个数；这里再写一次是为了
/// 让"shader 只认这么多层"和"数据层不收更多"两件事能对上，单测盯着）。
const MAX_LAYERS: usize = 8;

/// 每层 3 个 `vec4`：域端点 / 尺寸+强度 / 图集偏移+shaper 长度+开关。
const LAYER_VECTORS: usize = 3;
const HEADER_VECTORS: usize = 1;
const UNIFORM_VECTORS: usize = HEADER_VECTORS + LAYER_VECTORS * MAX_LAYERS;

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
    /// 当前链的那一套；`None` 时用 [`Self::disabled`]。
    active: Option<LutResources>,
    pub source: Option<Arc<LutChain>>,
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

    /// 换一条链（`None` / 空链 = 关闭）。返回「资源是否变了」—— 变了就要重建绑定组。
    ///
    /// 同一份 `Arc` 不重建。**强度变化也走这里**（`LutChain` 里带着强度，所以强度改了
    /// `Arc` 就是新的 → 重建一次，含图集；这是低频动作，代价见 `build` 的注释）。
    pub fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: Option<Arc<LutChain>>,
    ) -> bool {
        let source = source.filter(|chain| !chain.is_empty());
        if match (&self.source, &source) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        } {
            return false;
        }
        self.active = source
            .as_deref()
            .map(|chain| build(device, queue, Some(chain)));
        self.source = source;
        true
    }
}

/// 建一套句柄（+ 上传图集）。`None` = 「关闭」占位。
///
/// 上传量随链长 × 立方表尺寸增长（33³ × 3 层约 1.7 MB / 十几毫秒；65³ 会到几十毫秒）。
/// 它只发生在"链变了"这一种时刻（选 LUT / 加删层 / 拖顺序 / 松手后的强度），
/// **不在拖滑杆的每帧路径上**。
fn build(device: &wgpu::Device, queue: &wgpu::Queue, source: Option<&LutChain>) -> LutResources {
    let layers = source.map_or(&[][..], |chain| chain.layers());
    let count = layers.len().min(MAX_LAYERS);
    let max_edge = layers
        .iter()
        .take(count)
        .map(|layer| layer.lut.cube_size())
        .max()
        .unwrap_or(0)
        .max(1);
    // 每层在自己的 slot 里各留 1 texel 边界（X/Y/Z 都是），插值不会跨层串味。
    let slot = max_edge + 2;
    let depth = slot * count.max(1);
    let max_shaper = layers
        .iter()
        .take(count)
        .map(|layer| layer.lut.shaper().len())
        .max()
        .unwrap_or(0)
        .max(1);

    let mut uniform = [0_u8; UNIFORM_VECTORS * 16];
    let mut write = |index: usize, values: [f32; 4]| {
        for (offset, value) in values.iter().enumerate() {
            let at = index * 16 + offset * 4;
            uniform[at..at + 4].copy_from_slice(&value.to_ne_bytes());
        }
    };
    write(
        0,
        [
            if count > 0 { 1.0 } else { 0.0 },
            count as f32,
            slot as f32,
            0.0,
        ],
    );
    for (index, layer) in layers.iter().take(count).enumerate() {
        let (domain_min, domain_max) = layer.lut.domain();
        let cube = layer.lut.cube_size();
        let shaper = layer.lut.shaper().len();
        write(
            HEADER_VECTORS + index * LAYER_VECTORS,
            [domain_min[0], domain_min[1], domain_min[2], domain_max[0]],
        );
        write(
            HEADER_VECTORS + index * LAYER_VECTORS + 1,
            [
                domain_max[1],
                domain_max[2],
                cube as f32,
                layer.strength.clamp(0.0, 1.0),
            ],
        );
        write(
            HEADER_VECTORS + index * LAYER_VECTORS + 2,
            [
                (index * slot) as f32,
                if shaper == 0 { 0.0 } else { shaper as f32 },
                if cube > 0 { 1.0 } else { 0.0 },
                0.0,
            ],
        );
    }

    let cube = upload_atlas(device, queue, layers, count, slot, max_edge, depth);
    let shaper = upload_shapers(device, queue, layers, count, max_shaper);
    let cube_view = cube.create_view(&wgpu::TextureViewDescriptor::default());
    let shaper_view = shaper.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("raybend-creative-lut"),
        size: uniform.len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: true,
    });
    {
        buffer
            .slice(..)
            .get_mapped_range_mut()
            .expect("new mapped buffer")
            .copy_from_slice(&uniform);
    }
    buffer.unmap();
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
        uniform: buffer,
        cube,
        cube_view,
        shaper,
        shaper_view,
        sampler,
    }
}

/// 把每层的立方表写进同一张 3D 纹理的独立 Z 段（含边界复制）。
fn upload_atlas(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layers: &[crate::develop::lut::LutChainLayer],
    count: usize,
    slot: usize,
    max_edge: usize,
    depth: usize,
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("raybend-creative-lut-atlas"),
        size: wgpu::Extent3d {
            width: slot as u32,
            height: slot as u32,
            depth_or_array_layers: depth as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D3,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    if count == 0 {
        // 1×1×1 的占位：着色器靠 `enabled = 0` 早退，一次采样都不做。
        let pixel: Vec<u8> = [0.0_f32, 0.0, 0.0, 1.0]
            .iter()
            .flat_map(|value| half::f16::from_f32(*value).to_bits().to_le_bytes())
            .collect();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixel,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(8),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        return texture;
    }
    let mut row = Vec::with_capacity(slot * 8);
    for (index, layer) in layers.iter().take(count).enumerate() {
        // 纯 1D LUT（`LUT_1D_SIZE`，没有立方表）没有图集数据可写；
        // 它的求值在 shader 的 `has_cube = false` 分支完成，不能按立方表索引空切片。
        if layer.lut.cube_size() == 0 {
            continue;
        }
        let edge = layer.lut.cube_size().max(1);
        let samples = layer.lut.cube();
        for z in 0..slot {
            row.clear();
            // 边界复制：`0` 映射到 `1`，`slot-1` 映射到 `edge`。
            let sz = z.clamp(1, edge) - 1;
            for y in 0..slot {
                let sy = y.clamp(1, edge) - 1;
                for x in 0..slot {
                    let sx = x.clamp(1, edge) - 1;
                    let sample = samples[sx + edge * sy + edge * edge * sz];
                    for value in [sample[0], sample[1], sample[2], 1.0] {
                        row.extend_from_slice(
                            &half::f16::from_f32(value).to_bits().to_le_bytes(),
                        );
                    }
                }
            }
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: (index * slot + z) as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &row,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some((slot * 8) as u32),
                    rows_per_image: Some(slot as u32),
                },
                wgpu::Extent3d {
                    width: slot as u32,
                    height: slot as u32,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    let _ = max_edge;
    texture
}

/// shaper 全部放进一张 2D 数组纹理（layer = 层序号），一张绑定服务整条链。
fn upload_shapers(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layers: &[crate::develop::lut::LutChainLayer],
    count: usize,
    width: usize,
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("raybend-creative-lut-shapers"),
        size: wgpu::Extent3d {
            width: width as u32,
            height: 1,
            depth_or_array_layers: count.max(1) as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut pixels = vec![0_u8; width * 8];
    for index in 0..count.max(1) {
        let table = layers.get(index).map_or(&[][..], |layer| layer.lut.shaper());
        pixels.fill(0);
        for x in 0..width {
            let sample = if table.is_empty() {
                [0.0_f32, 0.0, 0.0]
            } else {
                table[x.min(table.len() - 1)]
            };
            for (offset, value) in [sample[0], sample[1], sample[2], 1.0].iter().enumerate() {
                pixels[x * 8 + offset * 2..x * 8 + offset * 2 + 2]
                    .copy_from_slice(&half::f16::from_f32(*value).to_bits().to_le_bytes());
            }
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: index as u32,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some((width * 8) as u32),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: width as u32,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
    }
    texture
}
