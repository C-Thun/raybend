//! wgpu 上下文：把 surface 挂到窗口、上传测试图、画一帧、resize、设备丢失与恢复。
//!
//! 本模块**不依赖 Tauri**（`AGENTS.md` §4：业务 crate 不许依赖 tauri）——
//! 它只接受一对 `raw-window-handle` 的裸句柄（[`RawHandles`]），
//! 从 Tauri 窗口上取句柄是 `src-tauri/src/spike_viewport.rs` 的事。
//!
//! # 为什么用裸句柄 + `unsafe`
//!
//! 安全路径（`Instance::create_surface(&window)`）要求 surface 的生存期**不长于**那个窗口对象，
//! 于是渲染器要么持着窗口（`Arc`）并把它和 surface 放进同一个自引用结构里，要么就得
//! 每次绘制都重新建 surface。裸句柄是 wgpu 官方给这种情况留的出口
//! （[`wgpu::SurfaceTargetUnsafe::RawHandle`]），代价是**我们自己要保证窗口活得比 surface 久** ——
//! 这一条由 spike 窗口的生命周期保证：窗口先建、后建上下文，关窗时先丢上下文再丢窗口。
//!
//! # A.2 里跟这里有关的几项
//!
//! - `raw-window-handle` 版本一致性：本 crate 用的 `wgpu 30` 与 `tauri 2` 都吃 rwh 0.6
//!   （`Cargo.lock` 里只有一份 `raw-window-handle 0.6.2`）—— 这是最容易翻车的地方，
//!   版本一旦分叉，`create_surface` 会在**运行时**失败而不是编译期。
//! - 透明挖洞：surface 的 `alpha_mode` 与 clear 色决定透出来的东西对不对，报告里会记下来。
//! - 后端回退：`WGPU_BACKEND=dx12|vulkan|gl` 由 wgpu 自己读环境变量，这里只把最终选中的后端如实报上去。
//! - 设备丢失：`device.destroy()` 演练 + 恢复重建。

use std::sync::Arc;

/*
 * 句柄类型用 **wgpu 自己 re-export 的 rwh**（`wgpu::rwh`）而不是直接依赖 `raw-window-handle`：
 * A.2 把「rwh 版本不一致」列为最常见的集成失败点 —— 版本一旦分叉，
 * `create_surface` 是**运行时**失败而不是编译期。用 wgpu 的那一份就永远同版本。
 * （`tauri 2` 也吃 rwh 0.6，`Cargo.lock` 里只有一份 `raw-window-handle 0.6.2`。）
 */
use wgpu::rwh::{RawDisplayHandle, RawWindowHandle};

use super::scene::TestImage;
use super::stats::AdapterInfo;
use super::viewport::{AlphaMode, Viewport};

/// 从窗口上取下来的一对裸句柄（rwh 0.6）。
#[derive(Debug, Clone, Copy)]
pub struct RawHandles {
    pub display: RawDisplayHandle,
    pub window: RawWindowHandle,
}

/// 上下文相关的失败。文案要能直接给用户看（spike 界面上会显示出来）。
#[derive(Debug, Clone)]
pub enum GpuError {
    /// 建 surface 失败（多半是窗口句柄类型不被后端支持）
    Surface(String),
    /// 找不到适配器（驱动缺失 / 软件渲染都没装）
    NoAdapter(String),
    /// 请求设备失败
    Device(String),
    /// 表面能力里没有可用的 alpha 模式
    NoAlphaMode,
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface(msg) => write!(f, "创建 surface 失败：{msg}"),
            Self::NoAdapter(msg) => write!(f, "找不到可用的 GPU 适配器：{msg}"),
            Self::Device(msg) => write!(f, "请求设备失败：{msg}"),
            Self::NoAlphaMode => write!(f, "surface 没有可用的 alpha 模式"),
        }
    }
}

impl std::error::Error for GpuError {}

/// 一次绘制的结局（给上层决定要不要重配 surface / 恢复设备）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderOutcome {
    /// 画完了
    Drawn,
    /// 没画（窗口尺寸为 0：最小化时是常态，**不是错误**）
    Skipped,
    /// surface 过期/丢失 → 已重配，下一帧继续
    Reconfigured(String),
}

/// 报告里要用的表面信息。
#[derive(Debug, Clone, Default)]
pub struct SurfaceDetails {
    pub format: String,
    pub alpha_mode: String,
    pub size: (u32, u32),
}

/// wgpu 上下文。
pub struct GpuContext {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    texture: wgpu::Texture,
    sampler: wgpu::Sampler,
    image: TestImage,
    viewport: Viewport,
    /// 设备丢失的记录（A.2 要）
    pub device_lost: Arc<std::sync::Mutex<Vec<String>>>,
    frames_drawn: u64,
    /// 上一帧发现 surface 「次优」（尺寸/DPI 刚变）→ **下一帧开头**重配。
    ///
    /// 为什么不能就地重配：wgpu 要求「`get_current_texture()` 拿到的那个 SurfaceOutput
    /// 必须先释放，才能 `configure()`」。而 `Suboptimal` 分支里那个帧**正活着** ——
    /// 在那里顺手 `configure` 会直接校验失败 panic（见 `render()` 里的详细记录）。
    reconfigure_pending: bool,
}

impl GpuContext {
    /// 建上下文并上传测试图。`size` 是窗口内容区的**物理**像素。
    pub fn new(handles: RawHandles, size: (u32, u32), dpr: f32) -> Result<Self, GpuError> {
        // 后端由 wgpu 读 `WGPU_BACKEND`（dx12 / vulkan / gl）—— spike 要试回退，所以不写死。
        // 用 `_from_env()` 那一族构造函数才会读环境变量（别拿 `default()`：wgpu 30 没这个默认实现）。
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        // SAFETY: 句柄由调用方从**活着的**窗口上取（`RawHandles` 的契约），
        // 且本上下文保证在窗口之前销毁（spike 窗口的生命周期由 `src-tauri/src/spike_viewport.rs` 管）。
        // 这是 wgpu 官方给「surface 要活得比窗口对象的长」这种情形留的出口，
        // 安全路径要求 surface 生存期不长于窗口对象，会逼出自引用结构。
        let surface = unsafe {
            instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: Some(handles.display),
                    raw_window_handle: handles.window,
                })
                .map_err(|e| GpuError::Surface(e.to_string()))?
        };

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            // 反指纹用的限制分桶：那条是给「把 wgpu 暴露给不可信内容」的场景（浏览器）用的，
            // 本地桌面应用要的是适配器真实的限制。
            apply_limit_buckets: false,
        }))
        .map_err(|e| GpuError::NoAdapter(e.to_string()))?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("raybend-spike"),
            required_features: wgpu::Features::empty(),
            // 用默认上限即可：spike 只需要一张 6000×4000 的纹理与最简单的管线
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .map_err(|e| GpuError::Device(e.to_string()))?;

        let device_lost = Arc::new(std::sync::Mutex::new(Vec::new()));
        install_device_lost_logger(&device, device_lost.clone());

        let caps = surface.get_capabilities(&adapter);
        let alpha_mode = pick_alpha_mode(&caps)?;
        let format = pick_format(&caps);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.0.max(1),
            height: size.1.max(1),
            present_mode: wgpu::PresentMode::Fifo, // 垂直同步：帧时间的下限就是屏幕刷新率
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);

        let image = super::scene::make_test_image(6000, 4000);
        let texture = create_image_texture(&device, &queue, &image);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("spike-sampler"),
            // 放大用最近邻：1:1 档位要能看出「一个图像像素就是一个屏幕像素」
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("spike-uniforms"),
            // 80 = mat4x4(64) + vec4(16)。**别改成「f32 + vec3」**：vec3 要 16 字节对齐，
            // 那样实际是 96，wgpu 会在绘制时报「expects 96」（WGSL 侧的注释里记着这个坑）
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("spike-bind-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    // ⚠️ 必须包含 **VERTEX**：顶点着色器要用 `textureDimensions(image, 0)`
                    // 算出四个角的图像像素坐标（整块的「四角由 vertex_index 现算」就是靠它）。
                    // 只给 FRAGMENT 的话，wgpu 会在**建管线时**报
                    // 「binding 1 is not available in the pipeline layout」——
                    // 这个错是离屏冒烟抓到的，否则会在 Windows 上当着人的面炸。
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = make_bind_group(&device, &layout, &texture, &sampler, &uniform);
        let pipeline = create_pipeline(&device, &layout, config.format);

        let mut viewport = Viewport {
            image_size: (image.width, image.height),
            viewport_size: (config.width as f32, config.height as f32),
            dpr,
            alpha_mode: match alpha_mode {
                wgpu::CompositeAlphaMode::PreMultiplied => AlphaMode::PreMultiplied,
                wgpu::CompositeAlphaMode::PostMultiplied => AlphaMode::PostMultiplied,
                _ => AlphaMode::Opaque,
            },
            ..Default::default()
        };
        viewport.refit();

        Ok(Self {
            instance,
            adapter,
            surface,
            device,
            queue,
            config,
            pipeline,
            bind_group,
            uniform,
            texture,
            sampler,
            image,
            viewport,
            device_lost,
            frames_drawn: 0,
            reconfigure_pending: false,
        })
    }

    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    pub fn viewport_mut(&mut self) -> &mut Viewport {
        &mut self.viewport
    }

    pub fn image(&self) -> &TestImage {
        &self.image
    }

    pub fn image_size(&self) -> (u32, u32) {
        (self.image.width, self.image.height)
    }

    pub fn frames_drawn(&self) -> u64 {
        self.frames_drawn
    }

    pub fn surface_details(&self) -> SurfaceDetails {
        SurfaceDetails {
            format: format!("{:?}", self.config.format),
            alpha_mode: format!("{:?}", self.config.alpha_mode),
            size: (self.config.width, self.config.height),
        }
    }

    pub fn adapter_info(&self) -> AdapterInfo {
        let info = self.adapter.get_info();
        AdapterInfo {
            backend: format!("{:?}", info.backend),
            name: info.name.clone(),
            device_type: format!("{:?}", info.device_type),
            driver: info.driver.clone(),
            driver_info: info.driver_info.clone(),
        }
    }

    /// 窗口尺寸变化（物理像素）。**最小化时宽高会是 0** —— 那种情况不重配 surface
    /// （wgpu 不允许 0 尺寸，而且最小化时也没必要画）。
    pub fn resize(&mut self, width: u32, height: u32, dpr: f32) {
        self.viewport.dpr = dpr;
        self.viewport.viewport_size = (width as f32, height as f32);
        if width == 0 || height == 0 {
            return;
        }
        if (self.config.width, self.config.height) == (width, height) {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        // 尺寸变了要按当前档位重新适配（`Viewport::refit` 会清 pan）
        let mode = self.viewport.fit_mode;
        if mode != super::viewport::FitMode::Free {
            self.viewport.refit();
        }
    }

    /// 画一帧：写 uniform → 开 render pass（scissor = 洞口）→ 呈现。
    pub fn render(&mut self) -> Result<RenderOutcome, GpuError> {
        if self.config.width == 0 || self.config.height == 0 {
            return Ok(RenderOutcome::Skipped);
        }
        /*
         * 补做上一帧挂起的重配 —— **位置很关键：必须在 `get_current_texture()` 之前**。
         *
         * 2026-09-17 实测事故：把 spike 窗口拖到另一块屏（触发 DPI 变化）→ surface 返回
         * `Suboptimal` → 当时我在那个分支里就地 `configure`，而那一帧还活着 →
         * `Validation Error: The SurfaceOutput ... must be dropped before re-configuring`。
         * 更糟的是：解开 panic 时要释放那个帧，而 surface 已不再处于可呈现状态 →
         * **第二次 panic** → 双重 panic 直接 abort（人类看到的就是「卡几秒直接崩掉」）。
         *
         * 所以规矩就一句：**持帧期间绝不 `configure`** —— 挂个标记，下一帧开头（此时无帧在手）再配。
         */
        if self.reconfigure_pending {
            self.reconfigure_pending = false;
            self.surface.configure(&self.device, &self.config);
        }
        self.write_uniforms();

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            // 次优（比如尺寸/DPI 刚变）：**这一帧仍然能画**，先画完呈现出去，
            // 重配推到下一帧开头（见上）。
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                self.reconfigure_pending = true;
                frame
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(RenderOutcome::Reconfigured("surface 过期/丢失 → 已重配".into()));
            }
            // 超时与「被遮住」（最小化/被完全覆盖）都不是错误：这一帧不画而已
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(RenderOutcome::Skipped);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(GpuError::Surface("surface 校验失败（看 wgpu 的报错日志）".into()));
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("spike-frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("spike-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        // 透明黑：洞口之外不画东西，透出下面的桌面/窗口（这就是「挖洞」）
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                // 不用 multiview（spike 用不到）
                multiview_mask: None,
            });
            // 洞口：scissor 之外一个像素都不碰 —— 挖洞就靠它
            if let Some((x, y, w, h)) = self.viewport.scissor() {
                pass.set_scissor_rect(x, y, w, h);
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.draw(0..4, 0..1);
            }
        }
        self.queue.submit(Some(encoder.finish()));
        // wgpu 30：呈现走 `queue.present`（不再是 `frame.present()`）
        self.queue.present(frame);
        self.frames_drawn += 1;
        Ok(RenderOutcome::Drawn)
    }

    fn write_uniforms(&self) {
        // 与离屏那条路共用同一份布局写法人（矩阵只有一处推导，见 `Viewport::matrix`）
        write_matrix(&self.queue, &self.uniform, &self.viewport);
    }

    /// 演练设备丢失：`device.destroy()`。
    ///
    /// 之后 `render()` 会失败/空转 —— 这正是真机上显卡驱动重启时的样子，
    /// 恢复走 [`Self::recover`]（A.2 要求把这条路走通）。
    pub fn simulate_device_loss(&mut self) {
        self.device.destroy();
        if let Ok(mut log) = self.device_lost.lock() {
            log.push("模拟：调用 device.destroy()".to_string());
        }
    }

    /// 设备丢失后重建：设备、surface 配置、管线、绑定组、纹理全部重来。
    ///
    /// **适配器可以复用**（`adapter` 在设备销毁后仍然有效），所以不必从头建 instance。
    pub fn recover(&mut self) -> Result<(), GpuError> {
        let (device, queue) = pollster::block_on(self.adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("raybend-spike-recovered"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .map_err(|e| GpuError::Device(e.to_string()))?;

        install_device_lost_logger(&device, self.device_lost.clone());
        self.device = device;
        self.queue = queue;
        self.surface.configure(&self.device, &self.config);

        // 纹理与绑定组跟着设备走，必须重建；图还在内存里，重新上传即可
        self.texture = create_image_texture(&self.device, &self.queue, &self.image);
        let layout = self.pipeline.get_bind_group_layout(0);
        self.bind_group = make_bind_group(
            &self.device,
            &layout,
            &self.texture,
            &self.sampler,
            &self.uniform,
        );
        self.pipeline = create_pipeline(&self.device, &layout, self.config.format);

        if let Ok(mut log) = self.device_lost.lock() {
            log.push("恢复：设备/管线/纹理已重建".to_string());
        }
        Ok(())
    }

    /// 重建 instance 与 surface（窗口换了、或者 surface 彻底不可用时的退路）。
    pub fn recreate_surface(&mut self, handles: RawHandles) -> Result<(), GpuError> {
        // SAFETY: 同 `new()` —— 句柄来自活着的窗口，调用方保证窗口比 surface 活得久。
        let surface = unsafe {
            self.instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: Some(handles.display),
                    raw_window_handle: handles.window,
                })
                .map_err(|e| GpuError::Surface(e.to_string()))?
        };
        self.surface = surface;
        self.surface.configure(&self.device, &self.config);
        Ok(())
    }
}

/// 设备丢失回调：把原因记下来（A.2 的报告要它）。
fn install_device_lost_logger(device: &wgpu::Device, sink: Arc<std::sync::Mutex<Vec<String>>>) {
    device.set_device_lost_callback(move |reason, message| {
        if let Ok(mut log) = sink.lock() {
            log.push(format!("{reason:?}: {message}"));
        }
    });
}

fn create_image_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &TestImage,
) -> wgpu::Texture {
    let size = wgpu::Extent3d {
        width: image.width,
        height: image.height,
        depth_or_array_layers: 1,
    };
    let bytes_per_row = image.width * 4;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("spike-image"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        // 图是 sRGB 数据：采样时硬件转线性，写回 surface 时再转回去
        // （色彩管理整体是后期里程碑，第一阶段只要「不二次转换」）
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("spike-upload"),
    });
    /*
     * 整块上传（6000×4000×4 = 96MB 一次拷完）。
     *
     * 本来担心要分块（`write_texture` 对单次大小有上限），但这里用的是
     * `queue.write_texture` 而不是 encoder 拷贝 —— 它没有分块要求，
     * 不需要为一个不存在的限制写一堆积木。真在 Windows 上碰到上限（报告里有上传耗时）
     * 再改成按行分块，改法就是在 `bytes` 上切 `bytes_per_row × rows`。
     */
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &image.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(bytes_per_row),
            rows_per_image: Some(image.height),
        },
        size,
    );
    queue.submit(Some(encoder.finish()));
    texture
}

fn make_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    texture: &wgpu::Texture,
    sampler: &wgpu::Sampler,
    uniform: &wgpu::Buffer,
) -> wgpu::BindGroup {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("spike-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("spike-shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("spike.wgsl").into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("spike-pipeline-layout"),
        bind_group_layouts: &[Some(layout)],
        // 不用 immediate / push constant 数据
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("spike-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                // 源色不透明（alpha=1）：pre/post 两种预乘约定下结果一致，
                // 这样「透明挖洞」是否成立只取决于窗口与合成器，不取决于我们的混合公式
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// 优先要预乘（与合成器的常见约定一致），退而求其次要直通 alpha，最后才是不透明。
fn pick_alpha_mode(caps: &wgpu::SurfaceCapabilities) -> Result<wgpu::CompositeAlphaMode, GpuError> {
    for candidate in [
        wgpu::CompositeAlphaMode::PreMultiplied,
        wgpu::CompositeAlphaMode::PostMultiplied,
        wgpu::CompositeAlphaMode::Auto,
        wgpu::CompositeAlphaMode::Inherit,
    ] {
        if caps.alpha_modes.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(GpuError::NoAlphaMode)
}

/// 优先 sRGB 表面格式（否则画出来偏亮/偏暗）。
fn pick_format(caps: &wgpu::SurfaceCapabilities) -> wgpu::TextureFormat {
    caps.formats
        .iter()
        .copied()
        .find(|f| f.is_srgb())
        .unwrap_or(caps.formats[0])
}

/* ══════════════════════════════════════════════════════════════
 * 离屏渲染（不需要窗口/表面）
 * ══════════════════════════════════════════════════════════════ */

/// 离屏渲染器：把测试图按给定视口画进一张纹理并**回读像素**。
///
/// 存在的理由有三条，都不是「顺手加的」：
///
/// 1. **没有真窗口的环境里也能验证管线**（本机 WSL 只有 lavapipe，开不了 Tauri 窗口）——
///    `GpuContext` 必须挂在一个真窗口上，而这一条不需要；
/// 2. **产出可核对的像素证据**：报告里的「画没画出来」不再只能靠人眼；
/// 3. 将来**导出/生成缩略图**本来就要走离屏路径（`FUTURE.md` C 段），
///    现在写好过以后从 `GpuContext` 里拆。
pub struct OffscreenRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    image: TestImage,
    /// 回读用的缓冲区尺寸（跟随上一次渲染尺寸）
    readback: Option<(u32, u32, wgpu::Buffer)>,
}

impl OffscreenRenderer {
    /// 建离屏渲染器（默认把测试图放进纹理；`WGPU_BACKEND` 同样生效）。
    pub fn new(handles_hint: Option<RawHandles>) -> Result<Self, GpuError> {
        let _ = handles_hint;
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|e| GpuError::NoAdapter(e.to_string()))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("raybend-spike-offscreen"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .map_err(|e| GpuError::Device(e.to_string()))?;

        let image = super::scene::make_test_image(6000, 4000);
        let texture = create_image_texture(&device, &queue, &image);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("spike-offscreen-sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("spike-offscreen-uniforms"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("spike-offscreen-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    // ⚠️ 必须包含 **VERTEX**：顶点着色器要用 `textureDimensions(image, 0)`
                    // 算出四个角的图像像素坐标（整块的「四角由 vertex_index 现算」就是靠它）。
                    // 只给 FRAGMENT 的话，wgpu 会在**建管线时**报
                    // 「binding 1 is not available in the pipeline layout」——
                    // 这个错是离屏冒烟抓到的，否则会在 Windows 上当着人的面炸。
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = make_bind_group(&device, &layout, &texture, &sampler, &uniform);
        let pipeline = create_pipeline(&device, &layout, wgpu::TextureFormat::Rgba8UnormSrgb);
        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group,
            uniform,
            image,
            readback: None,
        })
    }

    pub fn image(&self) -> &TestImage {
        &self.image
    }

    /// 适配器信息（报告里要）。
    pub fn adapter_info(&self) -> AdapterInfo {
        // 离屏不保留适配器对象，这里返回类型名占位 —— 真窗口那条路才需要完整信息
        AdapterInfo {
            backend: "(离屏)".to_string(),
            ..Default::default()
        }
    }

    /// 按给定视口渲染一帧，回读 RGBA8 像素（行主序，尺寸 = `size`）。
    ///
    /// 注意**回读的是 sRGB 编码后的字节**（纹理是 `Rgba8UnormSrgb`），
    /// 也就是「人眼看到的那个值」—— 断言里比对颜色时按这个口径。
    pub fn render(&mut self, viewport: &Viewport, size: (u32, u32)) -> Vec<u8> {
        let (width, height) = (size.0.max(1), size.1.max(1));
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("spike-offscreen-target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());

        write_matrix(&self.queue, &self.uniform, viewport);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("spike-offscreen-frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("spike-offscreen-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some((x, y, w, h)) = viewport.scissor() {
                pass.set_scissor_rect(x, y, w, h);
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.draw(0..4, 0..1);
            }
        }

        // 回读：每行必须按 256 字节对齐（wgpu 的 COPY_BYTES_PER_ROW_ALIGNMENT）
        let unpadded = width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = unpadded.div_ceil(align) * align;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("spike-offscreen-readback"),
            size: (padded as u64) * (height as u64),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        // wgpu 30：`Wait` 是结构体变体（可指定 submission 与超时；都不给＝等最近一次提交）
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        match receiver.recv() {
            Ok(Ok(())) => {}
            _ => return Vec::new(),
        }
        // wgpu 30：`get_mapped_range` 返回 Result（映射失败要给出去，不能 unwrap）
        let mapped = match slice.get_mapped_range() {
            Ok(mapped) => mapped,
            Err(_) => return Vec::new(),
        };
        let mut pixels = Vec::with_capacity((unpadded as usize) * (height as usize));
        for row in 0..height as usize {
            let start = row * padded as usize;
            pixels.extend_from_slice(&mapped[start..start + unpadded as usize]);
        }
        drop(mapped);
        buffer.unmap();
        self.readback = Some((width, height, buffer));
        pixels
    }
}

/// 写 uniform（矩阵 + 不透明度）—— 与 `GpuContext::write_uniforms` 同一份布局。
fn write_matrix(queue: &wgpu::Queue, uniform: &wgpu::Buffer, viewport: &Viewport) {
    let matrix = viewport.matrix();
    let mut bytes = [0u8; 80];
    for (column, values) in matrix.iter().enumerate() {
        for (row, value) in values.iter().enumerate() {
            let offset = (column * 4 + row) * 4;
            bytes[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
        }
    }
    bytes[64..68].copy_from_slice(&1.0f32.to_ne_bytes());
    queue.write_buffer(uniform, 0, &bytes);
}
