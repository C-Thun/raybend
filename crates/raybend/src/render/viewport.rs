//! 视口状态与坐标变换（`AGENTS.md` §6.1 红线 #1、#2）。
//!
//! > 视口状态由 Rust **独有**：前端只发送交互意图，不做坐标数学。
//!
//! 这个文件是整个渲染架构里**最容易被写错、又最贵**的一块：
//! 一旦坐标系的口径不统一，症状是「图比鼠标慢半拍」「缩放着缩着偏出去」，
//! 而且只在某些 DPI 下出现（`memory/PLAN.md` A.2 记着 RapidRAW 踩过「差 1 像素就图跟不上鼠标」）。
//! 所以它被写成**纯函数式的数学**并配了一整套测试。
//!
//! # 三个坐标系，一处收口
//!
//! | 名字 | 单位 | 原点 | 谁在用 |
//! | --- | --- | --- | --- |
//! | **图像像素** `image` | 像素（整张图的自然分辨率） | 图像左上角 | 解码/缩略图/EXIF |
//! | **物理像素** `physical` | 设备像素（= 窗口内容区的真实像素） | 窗口内容区左上角 | wgpu surface / 命中测试 |
//! | **CSS 像素** `css` | `physical / dpr` | 同窗口内容区 | 前端的鼠标事件、DOM 像素 |
//!
//! **任何跨层传坐标都必须写明是哪一种**。前端只发 CSS 像素的指针位置，
//! 由这里换算（`dpr` 也由 Rust 持有）—— 前端拿不到也不该推出物理像素。
//!
//! # 变换长什么样
//!
//! ```text
//! physical = 视口中心(洞口中心 + pan) + R(rotation) · (image − 图像中心) · zoom
//! ```
//!
//! pan 在**旋转之后**叠加（它是屏幕空间里的平移，不跟着图转）——
//! 这一条如果写反，旋转之后拖图会「斜着跑」。

/// 图像摆放方式。
///
/// `Free` 是用户手动缩放/平移之后的常态；其余三个是**指令**，
/// 在图像尺寸或洞口尺寸变化时由上层重新求值（见 [`Viewport::refit`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitMode {
    /// 不自动适配（用当前的 zoom 与 pan）
    Free,
    /// 整张图都看得见（可能有黑边）—— 默认
    Fit,
    /// 铺满洞口（可能裁掉边）—— 网格之外的看图常用
    Fill,
    /// 1 图像像素 : 1 物理像素（评估画质时用）
    OneToOne,
}

/// 绘制区域（「挖洞」的洞口），**物理像素**，相对窗口内容区左上角。
///
/// 有了它，照片适配的是**洞口**而不是整窗 —— 这正是方案 B 的用法：
/// webview 覆盖整窗，中间留出一块透明区，wgpu 在透明区里出图。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// surface 的 alpha 处理方式。
///
/// **必须与窗口的合成约定一致**，否则透明挖洞会出现「边缘发白/整体偏暗」——
/// 那就是 A.2 里那句「无 alpha 预乘色偏」。前端 WebView 底下透出来的东西
/// 由系统合成，所以这里存下来是为了**在界面上显示出来给人类核对**（spike 报告要它）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaMode {
    /// 颜色已乘以 alpha（`PreMultiplied`）
    PreMultiplied,
    /// 颜色未乘 alpha（`PostMultiplied`）
    PostMultiplied,
    /// 不要 alpha（不透明）
    Opaque,
}

/// 「几下算拖动」的阈值（CSS px）——与前端那套同一个口径
/// （`src/lib/editor-intent.ts` 的 `CLICK_SLOP_PX`）：手抖几像素不该被当成拖动。
pub const COMPARE_DRAG_SLOP_PX: f32 = 3.0;

/// 对比手势的下一步
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompareGestureStep {
    /// 什么也没变（不需要重画）
    Idle,
    /// 分线移到这个归一化位置（0..1）
    Handle(f32),
    /// 图像平移这么多**物理像素**
    Pan((f32, f32)),
}

/// 对比态的手势状态机（崔总 2026-10-08：对比态下也要能拖图）。
///
/// 判据只有一条，且**只看起手**：
///
/// * 起手命中分线把手 ⇒ 这一手势拖分线（与原行为一致）；
/// * 否则 ⇒ 这一手势**平移图像**（与普通视口拖动同一条路：累加位移、
///   松手补尾样本，只是这里不经过前端那个累加器 —— 指针的原始位置本来就在我们手里）。
///
/// 拖动阈值与普通拖动同一口径（[`COMPARE_DRAG_SLOP_PX`]）：手抖不算拖，
/// 免得“点一下”就把「适合窗口」悄悄切成自由模式（`pan_by` 会置 `FitMode::Free`）。
#[derive(Debug, Default, Clone, Copy)]
pub struct CompareGesture {
    /// 起手时抓的是分线把手
    handle: bool,
    /// 这一手势在平移图像
    panning: bool,
    /// 平移是否已经越过拖动阈值
    started: bool,
    /// 起手位置（CSS；阈值按它算）
    origin: (f32, f32),
    /// 上一次采样（CSS；逐样本位移按它算）
    last: (f32, f32),
}

impl CompareGesture {
    /// 换算一次位移：CSS → 物理像素（DPR 只在 Rust 里乘一次，与 `Pan` 意图同一规矩）
    fn pan_step(&self, viewport: &Viewport, css: (f32, f32), include_origin: bool) -> CompareGestureStep {
        let (dx, dy) = if include_origin {
            (css.0 - self.origin.0, css.1 - self.origin.1)
        } else {
            (css.0 - self.last.0, css.1 - self.last.1)
        };
        if dx == 0.0 && dy == 0.0 {
            return CompareGestureStep::Idle;
        }
        CompareGestureStep::Pan((dx * viewport.dpr, dy * viewport.dpr))
    }

    fn move_to(&mut self, viewport: &Viewport, css: (f32, f32)) -> CompareGestureStep {
        if self.handle {
            self.last = css;
            return viewport
                .compare_fraction_at(css.0)
                .map_or(CompareGestureStep::Idle, CompareGestureStep::Handle);
        }
        if !self.panning {
            return CompareGestureStep::Idle;
        }
        if !self.started {
            let travelled = (css.0 - self.origin.0).hypot(css.1 - self.origin.1);
            if travelled <= COMPARE_DRAG_SLOP_PX {
                self.last = css;
                return CompareGestureStep::Idle;
            }
            // 越过阈值：把**从起手算起**的位移一次补上（阈值内那几像素也要跟手，
            // 不然图像会落后手指一小段）
            self.started = true;
            self.last = css;
            return self.pan_step(viewport, css, true);
        }
        // ⚠️ 先算位移再更新 `last`（反过来的话 dx 永远是 0 —— 第一版就是这么写错的，
        // 单测当场抽到）
        let step = self.pan_step(viewport, css, false);
        self.last = css;
        step
    }

    pub fn step(
        &mut self,
        viewport: &Viewport,
        fraction: f32,
        phase: &str,
        css: (f32, f32),
    ) -> CompareGestureStep {
        if !css.0.is_finite() || !css.1.is_finite() {
            return CompareGestureStep::Idle;
        }
        match phase {
            "down" => {
                self.handle = viewport.compare_handle_hit(css, fraction);
                self.panning = !self.handle;
                self.started = false;
                self.origin = css;
                self.last = css;
                CompareGestureStep::Idle
            }
            "move" => self.move_to(viewport, css),
            "up" => {
                // 松手那一下自带最终位置：尾样本不能丢，不然图像停在半路
                let step = self.move_to(viewport, css);
                *self = CompareGesture::default();
                step
            }
            "cancel" => {
                *self = CompareGesture::default();
                CompareGestureStep::Idle
            }
            _ => CompareGestureStep::Idle,
        }
    }
}

/// 视口状态。**Rust 独有**。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// **逻辑图像尺寸**（**原图 / 解码尺寸**，图像像素）—— 不是当前纹理的尺寸。
    ///
    /// 坐标系、`1:1`、Fit 都按它算；纹理可能只是预览档（长边 1920），
    /// 渲染时被拉伸到这块矩形上（M3-W4：把两者分开，否则「1:1」会是预览图的 1:1）。
    pub image_size: (u32, u32),
    /// 窗口内容区的物理像素尺寸
    pub viewport_size: (f32, f32),
    /// 1.0 = 1 图像像素 : 1 物理像素
    pub zoom: f32,
    /// 屏幕空间平移（物理像素，正数=图向右下移动）
    pub pan_px: (f32, f32),
    /// 旋转角度（度，顺时针）
    pub rotation: f32,
    pub fit_mode: FitMode,
    pub clip_rect: Option<ClipRect>,
    /// CSS → surface 物理像素的有效比例（WebView `devicePixelRatio`）。
    /// 包含页面缩放：系统 125% × 页面 110% = 1.375，不能仅用 Tauri `scale_factor`。
    pub dpr: f32,
    pub alpha_mode: AlphaMode,
}

/// 缩放上下限：太大算不动也没意义，太小图会变成一个点。
pub const MIN_ZOOM: f32 = 0.01;
pub const MAX_ZOOM: f32 = 64.0;

impl Default for Viewport {
    fn default() -> Self {
        Self {
            image_size: (1, 1),
            viewport_size: (1.0, 1.0),
            zoom: 1.0,
            pan_px: (0.0, 0.0),
            rotation: 0.0,
            fit_mode: FitMode::Fit,
            clip_rect: None,
            dpr: 1.0,
            alpha_mode: AlphaMode::Opaque,
        }
    }
}

impl Viewport {
    /// 图像中心（图像像素）。
    pub fn image_center(&self) -> (f32, f32) {
        (self.image_size.0 as f32 / 2.0, self.image_size.1 as f32 / 2.0)
    }

    /// **实际用于摆图的矩形**：有洞口就用洞口，否则整窗。
    ///
    /// 照片适配的是洞口 —— 这是方案 B 的语义（webview 挖洞，原生内容画在洞里）。
    pub fn effective_rect(&self) -> ClipRect {
        match self.clip_rect {
            Some(rect) => rect,
            None => ClipRect {
                x: 0.0,
                y: 0.0,
                width: self.viewport_size.0,
                height: self.viewport_size.1,
            },
        }
    }

    /// 洞口/视口的中心（物理像素）—— 变换的基准点。
    fn rect_center(&self) -> (f32, f32) {
        let rect = self.effective_rect();
        (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
    }

    /// 图像像素 → 物理像素。
    pub fn image_to_physical(&self, point: (f32, f32)) -> (f32, f32) {
        let center = self.image_center();
        let vx = point.0 - center.0;
        let vy = point.1 - center.1;
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let area = self.rect_center();
        (
            area.0 + self.pan_px.0 + (vx * cos - vy * sin) * self.zoom,
            area.1 + self.pan_px.1 + (vx * sin + vy * cos) * self.zoom,
        )
    }

    /// 物理像素 → 图像像素（[`Self::image_to_physical`] 的逆）。
    pub fn physical_to_image(&self, point: (f32, f32)) -> (f32, f32) {
        let area = self.rect_center();
        let sx = point.0 - area.0 - self.pan_px.0;
        let sy = point.1 - area.1 - self.pan_px.1;
        // 逆旋转 = 转置（先反旋转，再反缩放）
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let zoom = self.zoom.max(MIN_ZOOM);
        let center = self.image_center();
        (
            center.0 + (sx * cos + sy * sin) / zoom,
            center.1 + (-sx * sin + sy * cos) / zoom,
        )
    }

    /* ── CSS 像素 ↔ 物理像素（前端只碰 CSS 像素）────────────── */

    pub fn css_to_physical(&self, point: (f32, f32)) -> (f32, f32) {
        (point.0 * self.dpr, point.1 * self.dpr)
    }

    pub fn physical_to_css(&self, point: (f32, f32)) -> (f32, f32) {
        let dpr = self.dpr.max(f32::EPSILON);
        (point.0 / dpr, point.1 / dpr)
    }

    /// 前端送来的指针位置（CSS 像素）→ 图像像素。
    pub fn pointer_to_image(&self, css_point: (f32, f32)) -> (f32, f32) {
        self.physical_to_image(self.css_to_physical(css_point))
    }

    /* ── 适配与缩放 ──────────────────────────────────────── */

    /// 按当前 `fit_mode` 求出该有的 zoom。
    ///
    /// `Free` 保持现状。图像或洞口尺寸为 0 时返回现状（不去除零）。
    pub fn zoom_for_fit(&self) -> f32 {
        let rect = self.effective_rect();
        let (iw, ih) = (self.image_size.0 as f32, self.image_size.1 as f32);
        if iw <= 0.0 || ih <= 0.0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return self.zoom;
        }
        let zoom = match self.fit_mode {
            FitMode::Free => self.zoom,
            FitMode::OneToOne => 1.0,
            FitMode::Fit => (rect.width / iw).min(rect.height / ih),
            FitMode::Fill => (rect.width / iw).max(rect.height / ih),
        };
        zoom.clamp(MIN_ZOOM, MAX_ZOOM)
    }

    /// 图像尺寸或洞口尺寸变化后重新适配：**zoom 重算、pan 归零**。
    ///
    /// 为什么 pan 一定要归零：换了尺寸还留着旧 pan，图会歪在一个说不清的位置上，
    /// 用户看到的是「转屏之后照片跑到一边去了」。
    pub fn refit(&mut self) {
        self.zoom = self.zoom_for_fit();
        self.pan_px = (0.0, 0.0);
    }

    /// 工具模式把旋转后的整张原图放进洞口，避免边角被裁掉。
    pub fn refit_rotated(&mut self) {
        let rect = self.effective_rect();
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let (w, h) = (self.image_size.0 as f32, self.image_size.1 as f32);
        let bounds = ((w * cos.abs() + h * sin.abs()).max(1.0),
                      (w * sin.abs() + h * cos.abs()).max(1.0));
        if rect.width > 0.0 && rect.height > 0.0 {
            self.fit_mode = FitMode::Fit;
            self.zoom = (rect.width / bounds.0).min(rect.height / bounds.1).clamp(MIN_ZOOM, MAX_ZOOM);
            self.pan_px = (0.0, 0.0);
        }
    }

    /// 以某个**物理像素**点为锚点缩放：缩放前后锚点下方的图像像素不动。
    ///
    /// 这是「差 1 像素图就跟不上鼠标」的那一处：手写时最容易漏掉
    /// 「按新 zoom 重新算 pan」这一步（浮点上看起来只差一点点，
    /// 连续缩放几十次就偏出屏幕了）。测试里对 DPR 1.0 / 1.25 / 1.5 各验一遍。
    pub fn zoom_at(&mut self, anchor_physical: (f32, f32), factor: f32) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let before = self.physical_to_image(anchor_physical);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.fit_mode = FitMode::Free;
        let after = self.image_to_physical(before);
        self.pan_px.0 += anchor_physical.0 - after.0;
        self.pan_px.1 += anchor_physical.1 - after.1;
    }

    /// 平移到某个物理像素位移（拖动时用）。
    pub fn pan_by(&mut self, delta_physical: (f32, f32)) {
        self.pan_px.0 += delta_physical.0;
        self.pan_px.1 += delta_physical.1;
        self.fit_mode = FitMode::Free;
    }

    /// 这个图像像素点落在洞口里吗（命中测试 + 覆盖层裁剪用）。
    ///
    /// 注意**洞口之外不算命中** —— 透明区如果算命中，webview 那层就收不到点击了。
    pub fn is_inside_clip(&self, physical: (f32, f32)) -> bool {
        let rect = self.effective_rect();
        physical.0 >= rect.x
            && physical.0 <= rect.x + rect.width
            && physical.1 >= rect.y
            && physical.1 <= rect.y + rect.height
    }

    /// 由前端指针位置命中到哪一张图的哪个像素（命中测试的完整口径）。
    ///
    /// 洞口外、或落在图像范围外 → `None`。
    pub fn hit_test_css(&self, css_point: (f32, f32)) -> Option<(f32, f32)> {
        let physical = self.css_to_physical(css_point);
        if !self.is_inside_clip(physical) {
            return None;
        }
        let image = self.physical_to_image(physical);
        let (iw, ih) = (self.image_size.0 as f32, self.image_size.1 as f32);
        if image.0 < 0.0 || image.1 < 0.0 || image.0 >= iw || image.1 >= ih {
            return None;
        }
        Some(image)
    }

    /// wgpu 的 scissor 矩形（整型 + 夹到 surface 内）：透明挖洞就靠它把绘制裁进洞里。
    ///
    /// wgpu 要求：`x + width <= surface 宽`，否则整条命令无效（**不是**报错，是静默不画）。
    /// 所以这里自己夹一遍。
    pub fn scissor(&self) -> Option<(u32, u32, u32, u32)> {
        let rect = self.effective_rect();
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return None;
        }
        let x = rect.x.max(0.0).floor();
        let y = rect.y.max(0.0).floor();
        let max_w = self.viewport_size.0.max(0.0) - x;
        let max_h = self.viewport_size.1.max(0.0) - y;
        let width = rect.width.min(max_w).floor();
        let height = rect.height.min(max_h).floor();
        if width < 1.0 || height < 1.0 {
            return None;
        }
        Some((x as u32, y as u32, width as u32, height as u32))
    }
/// 对比分线在洞口宽度上的归一化位置。指针只上报窗口 CSS 坐标；
    /// 洞口原点和 DPR 的换算只在这里做。
    #[must_use]
    pub fn compare_fraction_at(&self, css_x: f32) -> Option<f32> {
        let rect = self.clip_rect?;
        if !css_x.is_finite() || rect.width <= 0.0 || !rect.width.is_finite() {
            return None;
        }
        Some(((css_x * self.dpr - rect.x) / rect.width).clamp(0.0, 1.0))
    }

    /// 指针 → 旋转后的水平画框坐标（按原图宽高归一化）。
    /// 与 `pointer_to_image` 不同，这里不逆旋转：裁切框始终水平。
    #[must_use]
    pub fn frame_pointer_normalized(&self, css: (f32, f32), source: (u32, u32)) -> Option<(f32, f32)> {
        if !css.0.is_finite() || !css.1.is_finite() || source.0 == 0 || source.1 == 0
            || self.zoom <= 0.0 || self.clip_rect.is_none() { return None; }
        let physical = self.css_to_physical(css);
        if !self.is_inside_clip(physical) { return None; }
        let area = self.rect_center();
        Some((0.5 + (physical.0 - area.0 - self.pan_px.0) / (self.zoom * source.0 as f32),
              0.5 + (physical.1 - area.1 - self.pan_px.1) / (self.zoom * source.1 as f32)))
    }

    /// 归一化成片框 → 洞口内 CSS 矩形。覆盖层只照这个值画，不复制视口变换。
    #[must_use]
    pub fn frame_rect_css(&self, rect: crate::develop::geometry::CropRect, source: (u32, u32)) -> Option<ClipRect> {
        let clip = self.clip_rect?;
        if source.0 == 0 || source.1 == 0 { return None; }
        let area = self.rect_center();
        let dpr = self.dpr.max(f32::EPSILON);
        Some(ClipRect {
            x: (area.0 + self.pan_px.0 + (rect.x - 0.5) * source.0 as f32 * self.zoom - clip.x) / dpr,
            y: (area.1 + self.pan_px.1 + (rect.y - 0.5) * source.1 as f32 * self.zoom - clip.y) / dpr,
            width: rect.width * source.0 as f32 * self.zoom / dpr,
            height: rect.height * source.1 as f32 * self.zoom / dpr,
        })
    }

    /// 把手命中的 CSS 半径 → 两轴归一化容差。
    #[must_use]
    pub fn frame_hit_tolerance(&self, css_radius: f32, source: (u32, u32)) -> (f32, f32) {
        let scale = self.zoom.max(MIN_ZOOM) / self.dpr.max(f32::EPSILON);
        (css_radius / (scale * source.0.max(1) as f32), css_radius / (scale * source.1.max(1) as f32))
    }

    /// 洞口内的 CSS 分线位置；覆盖层直接放在这个坐标，不自行乘 DPR。
    #[must_use]
    pub fn compare_css_x(&self, fraction: f32) -> Option<f32> {
        let rect = self.clip_rect?;
        if !fraction.is_finite() || !rect.width.is_finite() || self.dpr <= 0.0 { return None; }
        Some(rect.width * fraction.clamp(0.0, 1.0) / self.dpr)
    }

    /// 分线把手命中（窗口 CSS 坐标）。只允许在图像洞口内、且靠近分线开始拖。
    #[must_use]
    pub fn compare_handle_hit(&self, css: (f32, f32), fraction: f32) -> bool {
        if !css.0.is_finite() || !css.1.is_finite() { return false; }
        let Some(rect) = self.clip_rect else { return false; };
        let physical = self.css_to_physical(css);
        let line = rect.x + rect.width * fraction.clamp(0.0, 1.0);
        self.is_inside_clip(physical) && (physical.0 - line).abs() <= 14.0 * self.dpr
    }

    /// 左边参考帧、右边当前结果的物理 scissor。两张纹理共用本视口矩阵。
    #[must_use]
    pub fn compare_scissors(&self, fraction: f32) -> Option<[(u32, u32, u32, u32); 2]> {
        let (x, y, width, height) = self.scissor()?;
        if !fraction.is_finite() { return None; }
        let left = ((width as f32 * fraction.clamp(0.0, 1.0)).round() as u32).min(width);
        Some([(x, y, left, height), (x + left, y, width - left, height)])
    }

    /// 图像像素 → NDC（-1..1）的 4×4 矩阵，**列主序**（WGSL 的 `mat4x4<f32>` 直接吃）。
    ///
    /// NDC 的定义域是**整个 surface**（不是洞口）：洞口靠 scissor 裁，
    /// 这样矩阵只依赖视口尺寸，缩放窗口时不需要重算矩阵以外的东西。
    ///
    /// 实现只有 [`Self::matrix_for`] 一处 —— 这里只是把当前画布尺寸递进去。
    pub fn matrix(&self) -> [[f32; 4]; 4] {
        self.matrix_for((self.image_size.0 as f32, self.image_size.1 as f32))
    }

    /// 与 [`Self::matrix`] **同一套推导**，但画布尺寸按参数给（列主序）。
    ///
    /// 用途只有一个：**对比参照**。传 [`Self::cover_size`] 的结果，就把参照帧
    /// 等比放大后居中贴在同一块窗口上（多出来的部分在窗口外，由 scissor 裁掉）。
    /// 矩阵数学仍然只有这一处 —— 别在外面再推一份。
    pub fn matrix_for(&self, source: (f32, f32)) -> [[f32; 4]; 4] {
        let (w, h) = (self.viewport_size.0.max(1.0), self.viewport_size.1.max(1.0));
        let center = (source.0 / 2.0, source.1 / 2.0);
        let area = self.rect_center();
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let scale = self.zoom;
        // 平移：把画布中心搬到 (area + pan)，再换到 NDC
        let tx = area.0 + self.pan_px.0;
        let ty = area.1 + self.pan_px.1;
        // physical = (tx + (vx·cos − vy·sin)·z, ty + (vx·sin + vy·cos)·z)
        // ndc.x = physical.x/w·2 − 1 ; ndc.y = 1 − physical.y/h·2
        //
        // ⚠️ y 方向两个系数**必须带负号**：NDC 的 y 轴是向上的，屏幕像素坐标是向下的。
        // 漏掉负号的症状是画面上下颠倒（或整体偏移）—— 矩阵测试就是拿这个钉住的。
        let a = cos * scale * 2.0 / w;
        let b = -sin * scale * 2.0 / w;
        let c = -sin * scale * 2.0 / h;
        let d = -cos * scale * 2.0 / h;
        // 平移项里含 −center·R·z（把画布中心搬到原点）
        let tx_ndc = (tx - (a * center.0 + b * center.1) * w / 2.0) * 2.0 / w - 1.0;
        let ty_ndc = 1.0 - (ty + (c * center.0 + d * center.1) * h / 2.0) * 2.0 / h;
        [
            [a, c, 0.0, 0.0],
            [b, d, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [tx_ndc, ty_ndc, 0.0, 1.0],
        ]
    }

    /// 参照帧按**等比 cover** 填进当前画面后的画布尺寸（参照帧自己的像素坐标系）。
    ///
    /// 「只裁不空、不拉伸」（崔总 2026-10-08）：`s = max(cur.w/ref.w, cur.h/ref.h)` ——
    /// 两个方向都不小于当前画面，多出来的那部分落在窗口外，由
    /// [`Self::reference_scissor`] 裁掉。比例与参照帧自己一致，
    /// 所以「不同裁切 / 不同像素尺寸的两张图」也能对比。
    ///
    /// 参照帧与当前画面**比例相同**时（例如 SOOC 路径：参照就是按当前几何裁的）
    /// `s` 正好等于两边的尺寸比 ⇒ 贴出来与旧行为一模一样，不是新特效。
    pub fn cover_size(&self, source: (u32, u32)) -> (f32, f32) {
        let (w, h) = (source.0 as f32, source.1 as f32);
        let (current_w, current_h) = (self.image_size.0 as f32, self.image_size.1 as f32);
        if w <= 0.0 || h <= 0.0 {
            return (w.max(1.0), h.max(1.0));
        }
        if current_w <= 0.0 || current_h <= 0.0 {
            return (w, h);
        }
        let scale = (current_w / w).max(current_h / h);
        (w * scale, h * scale)
    }

    /// 当前画面在屏幕上的矩形（物理像素）。
    ///
    /// 对比参照只在这块里画 —— **以当前画面为蒙版**（崔总 2026-10-08）：
    /// cover 之后多出来的部分不该溢到画面外的留白区去。
    /// 旋转时取外接矩形（对比与旋转工具不会同时开，这里只是不把话说死）。
    #[must_use]
    pub fn image_rect(&self) -> ClipRect {
        let w = self.image_size.0 as f32 * self.zoom;
        let h = self.image_size.1 as f32 * self.zoom;
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let area = self.rect_center();
        let center = (area.0 + self.pan_px.0, area.1 + self.pan_px.1);
        let width = w * cos.abs() + h * sin.abs();
        let height = w * sin.abs() + h * cos.abs();
        ClipRect {
            x: center.0 - width / 2.0,
            y: center.1 - height / 2.0,
            width,
            height,
        }
    }

    /// 对比左侧参照的 scissor：洞口左半（[`Self::compare_scissors`] 的前半）∩ **当前画面矩形**。
    ///
    /// 焦点是「当前画面多大，参照就裁多大」：放大到铺满洞口时交集就是洞口左半
    /// （与旧行为一致），适配时有留白则收进画面矩形里（不溢到留白上）。
    #[must_use]
    pub fn reference_scissor(&self, left: (u32, u32, u32, u32)) -> Option<(u32, u32, u32, u32)> {
        let rect = self.image_rect();
        let x0 = (left.0 as f32).max(rect.x).max(0.0);
        let y0 = (left.1 as f32).max(rect.y).max(0.0);
        let x1 = ((left.0 + left.2) as f32)
            .min((rect.x + rect.width).max(0.0))
            .min(self.viewport_size.0.max(0.0));
        let y1 = ((left.1 + left.3) as f32)
            .min((rect.y + rect.height).max(0.0))
            .min(self.viewport_size.1.max(0.0));
        let width = (x1 - x0).floor();
        let height = (y1 - y0).floor();
        if width < 1.0 || height < 1.0 {
            return None;
        }
        Some((x0.floor() as u32, y0.floor() as u32, width as u32, height as u32))
    }

    /// **图像像素 → 洞口内 CSS 像素**的仿射变换（覆盖层专用，M3-W3 定契约）。
    ///
    /// 返回 `[a, b, c, d, e, f]`，语义与 CSS 的 `matrix()` 完全一致：
    ///
    /// ```text
    /// css_x = a·px + c·py + e
    /// css_y = b·px + d·py + f
    /// ```
    ///
    /// # 为什么要有它（`AGENTS.md` §6.1 红线 2）
    ///
    /// 覆盖层（裁切柄、旋转框、对比分线、将来的蒙版）必须与照片**像素级对齐**，
    /// 而视口数学（缩放 / 平移 / 旋转 / DPR / 洞口）是 Rust 独有的。
    /// 与其让每个覆盖层自己推导一遍（一定会有人漏掉旋转或 DPR），
    /// 不如把**同一个变换**交出去：覆盖层把子元素写成**图像像素坐标**，
    /// 再整层套上这个矩阵 —— 数学只有一份。
    ///
    /// 洞口缺失（还没上报过）时返回 `None`。
    #[must_use]
    pub fn css_overlay_transform(&self) -> Option<[f32; 6]> {
        let rect = self.clip_rect?;
        let dpr = if self.dpr > 0.0 { self.dpr } else { 1.0 };
        let scale = self.zoom / dpr;
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let a = scale * cos;
        let b = scale * sin;
        let c = -scale * sin;
        let d = scale * cos;
        let area = self.rect_center();
        // 物理 → CSS：先减洞口原点，再除 DPR
        let tx = (area.0 + self.pan_px.0 - rect.x) / dpr;
        let ty = (area.1 + self.pan_px.1 - rect.y) / dpr;
        let center = self.image_center();
        Some([
            a,
            b,
            c,
            d,
            tx - (a * center.0 + c * center.1),
            ty - (b * center.0 + d * center.1),
        ])
    }

    /// 矩阵作用在图像四角上，得到 NDC 四角（给测试与覆盖层对齐用）。
    pub fn projected_corners_ndc(&self) -> [(f32, f32); 4] {
        let m = self.matrix();
        let (iw, ih) = (self.image_size.0 as f32, self.image_size.1 as f32);
        [(0.0, 0.0), (iw, 0.0), (iw, ih), (0.0, ih)].map(|(x, y)| {
            (
                m[0][0] * x + m[1][0] * y + m[3][0],
                m[0][1] * x + m[1][1] * y + m[3][1],
            )
        })
    }

    /// 判断某个设置组合是否会出问题（真机上的「白屏 / 什么都不画」自查）。
    pub fn sanity_problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.viewport_size.0 <= 0.0 || self.viewport_size.1 <= 0.0 {
            problems.push("viewport 尺寸为 0（窗口最小化了？）".to_string());
        }
        if self.image_size.0 == 0 || self.image_size.1 == 0 {
            problems.push("图像尺寸为 0".to_string());
        }
        if !self.zoom.is_finite() || self.zoom < MIN_ZOOM || self.zoom > MAX_ZOOM {
            problems.push(format!("zoom 超出范围：{}", self.zoom));
        }
        if !self.pan_px.0.is_finite() || !self.pan_px.1.is_finite() {
            problems.push("pan 不是有限数".to_string());
        }
        if let Some(rect) = self.clip_rect
            && (rect.width <= 0.0 || rect.height <= 0.0) {
                problems.push("洞口尺寸为 0（这一帧不会画任何东西）".to_string());
            }
        if self.scissor().is_none() {
            problems.push("scissor 算不出来（wgpu 会静默不画）".to_string());
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp() -> Viewport {
        Viewport {
            image_size: (4000, 3000),
            viewport_size: (1600.0, 1200.0),
            zoom: 1.0,
            pan_px: (0.0, 0.0),
            rotation: 0.0,
            fit_mode: FitMode::Free,
            clip_rect: None,
            dpr: 1.0,
            alpha_mode: AlphaMode::PreMultiplied,
        }
    }

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn image_center_maps_to_area_center() {
        let v = vp();
        let mid = v.image_to_physical(v.image_center());
        assert!(close(mid.0, 800.0, 1e-3) && close(mid.1, 600.0, 1e-3), "{mid:?}");
    }

    #[test]
    fn round_trip_is_identity() {
        let mut v = vp();
        v.zoom = 2.5;
        v.pan_px = (37.0, -12.5);
        v.rotation = 17.0;
        for point in [(0.0, 0.0), (4000.0, 3000.0), (1234.5, 987.25), (-50.0, 10.0)] {
            let back = v.physical_to_image(v.image_to_physical(point));
            assert!(close(back.0, point.0, 1e-2) && close(back.1, point.1, 1e-2), "{point:?} → {back:?}");
        }
    }

    #[test]
    fn zoom_at_keeps_the_anchor_pixel_under_the_cursor() {
        // 这条就是 A.2 的「坐标同步（快速缩放是否漂移）」：连续缩放 50 次之后，
        // 锚点下的图像像素必须**一点没动**。
        for dpr in [1.0_f32, 1.25, 1.5] {
            let mut v = vp();
            v.dpr = dpr;
            v.zoom = 0.7;
            v.rotation = 8.0;
            v.pan_px = (12.0, -33.0);
            let anchor_css = (613.0, 402.0);
            let anchor = v.css_to_physical(anchor_css);
            let before = v.physical_to_image(anchor);
            for i in 0..50 {
                // 一进一退交替，模拟滚轮来回
                let factor = if i % 2 == 0 { 1.15 } else { 1.0 / 1.15 };
                v.zoom_at(anchor, factor);
            }
            let after = v.physical_to_image(anchor);
            assert!(
                close(before.0, after.0, 0.01) && close(before.1, after.1, 0.01),
                "dpr={dpr}：锚点像素漂了 {before:?} → {after:?}"
            );
        }
    }

    #[test]
    fn zoom_at_is_stable_at_zoom_limits() {
        let mut v = vp();
        let anchor = (100.0, 100.0);
        // 顶到上限后再缩放，锚点仍不该动
        for _ in 0..20 {
            v.zoom_at(anchor, 4.0);
        }
        assert!(close(v.zoom, MAX_ZOOM, 1e-3));
        let before = v.physical_to_image(anchor);
        v.zoom_at(anchor, 4.0); // 已经到顶
        let after = v.physical_to_image(anchor);
        assert!(close(before.0, after.0, 0.01) && close(before.1, after.1, 0.01));
        assert!(close(v.zoom, MAX_ZOOM, 1e-3), "到顶就不该再涨");
    }

    #[test]
    fn zoom_at_ignores_nonsense_factors() {
        let mut v = vp();
        let snapshot = v;
        v.zoom_at((10.0, 10.0), 0.0);
        v.zoom_at((10.0, 10.0), -2.0);
        v.zoom_at((10.0, 10.0), f32::NAN);
        v.zoom_at((10.0, 10.0), f32::INFINITY);
        assert_eq!(v, snapshot);
    }

    #[test]
    fn fit_modes_use_the_hole_not_the_window() {
        let mut v = vp();
        v.clip_rect = Some(ClipRect {
            x: 400.0,
            y: 300.0,
            width: 800.0,
            height: 600.0,
        });
        v.fit_mode = FitMode::Fit;
        // 洞口 800×600，图 4000×3000 → 0.2 倍刚好放进去
        assert!(close(v.zoom_for_fit(), 0.2, 1e-4));
        v.fit_mode = FitMode::Fill;
        // 比例相同，Fill 也是 0.2
        assert!(close(v.zoom_for_fit(), 0.2, 1e-4));
        v.fit_mode = FitMode::OneToOne;
        assert!(close(v.zoom_for_fit(), 1.0, 1e-6));
        // 洞口更扁时 Fill 取大者
        v.clip_rect = Some(ClipRect {
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 600.0,
        });
        v.fit_mode = FitMode::Fit;
        assert!(close(v.zoom_for_fit(), 0.1, 1e-4), "Fit 取小者：{}", v.zoom_for_fit());
        v.fit_mode = FitMode::Fill;
        assert!(close(v.zoom_for_fit(), 0.2, 1e-4), "Fill 取大者：{}", v.zoom_for_fit());
    }

    #[test]
    fn refit_resets_pan() {
        let mut v = vp();
        v.fit_mode = FitMode::Fit;
        v.pan_px = (100.0, -50.0);
        v.zoom = 9.0;
        v.refit();
        assert_eq!(v.pan_px, (0.0, 0.0));
        assert!(close(v.zoom, 0.4, 1e-4), "1600×1200 装 4000×3000 = 0.4：{}", v.zoom);
        let center = v.image_to_physical(v.image_center());
        assert!(close(center.0, 800.0, 1e-3) && close(center.1, 600.0, 1e-3));
    }

    #[test]
    fn hole_is_the_frame_of_reference() {
        let mut v = vp();
        v.clip_rect = Some(ClipRect {
            x: 300.0,
            y: 200.0,
            width: 1000.0,
            height: 800.0,
        });
        v.fit_mode = FitMode::Fit;
        v.refit();
        let center = v.image_to_physical(v.image_center());
        assert!(close(center.0, 800.0, 1e-3), "洞口中心 x：{center:?}");
        assert!(close(center.1, 600.0, 1e-3), "洞口中心 y：{center:?}");
    }

    #[test]
    fn rotation_is_clockwise_and_pan_is_in_screen_space() {
        let mut v = vp();
        v.zoom = 1.0;
        v.rotation = 90.0;
        // 图像中心右侧 100px 的点，顺时针转 90° 后应当在中心**下方** 100px
        let center = v.image_center();
        let right = (center.0 + 100.0, center.1);
        let mapped = v.image_to_physical(right);
        assert!(close(mapped.0, 800.0, 1e-2) && close(mapped.1, 700.0, 1e-2), "{mapped:?}");
        // pan 在屏幕空间：加 (10, 20) 就是屏幕上的 (10, 20)，与旋转无关
        v.pan_px = (10.0, 20.0);
        let mapped = v.image_to_physical(right);
        assert!(close(mapped.0, 810.0, 1e-2) && close(mapped.1, 720.0, 1e-2), "{mapped:?}");
    }

    #[test]
    fn hit_test_respects_clip_and_image_bounds() {
        let mut v = vp();
        v.clip_rect = Some(ClipRect {
            x: 100.0,
            y: 100.0,
            width: 500.0,
            height: 400.0,
        });
        v.dpr = 2.0; // CSS 像素 = 物理/2
        v.fit_mode = FitMode::Fill;
        v.refit();
        // 洞口中心（物理 350,300 → CSS 175,150）命中图像中心
        let hit = v.hit_test_css((175.0, 150.0)).expect("洞口中心该命中");
        assert!(close(hit.0, 2000.0, 1.0) && close(hit.1, 1500.0, 1.0), "{hit:?}");
        // 洞口外面（物理 50,50 → CSS 25,25）不命中 —— 透明区不吞事件
        assert!(v.hit_test_css((25.0, 25.0)).is_none());
        // 洞口里但图像外面：缩到 1:1、把图推到够远（pan 要把图的右边界推到洞口左边界之外）
        v.fit_mode = FitMode::OneToOne;
        v.refit();
        v.pan_px = (-2500.0, 0.0);
        assert!(v.hit_test_css((175.0, 150.0)).is_none(), "图已经被推走了，不该命中");
    }

    #[test]
    fn scissor_is_clamped_to_surface() {
        let mut v = vp();
        v.clip_rect = Some(ClipRect {
            x: 700.0,
            y: 100.0,
            width: 1200.0, // 右边界超出 1600
            height: 400.0,
        });
        let (x, y, w, h) = v.scissor().expect("应当算得出来");
        assert_eq!((x, y), (700, 100));
        assert_eq!(w, 900, "必须夹到 surface 内，否则 wgpu 静默不画");
        assert_eq!(h, 400);
        // 完全在窗口外 → 没有可画的
        v.clip_rect = Some(ClipRect {
            x: 1700.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        });
        assert!(v.scissor().is_none());
        // 尺寸为 0 的洞口 → 没有可画的
        v.clip_rect = Some(ClipRect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        });
        assert!(v.scissor().is_none());
    }

    #[test]
    fn matrix_matches_direct_mapping() {
        let mut v = vp();
        v.zoom = 0.37;
        v.pan_px = (21.0, -8.0);
        v.rotation = 23.0;
        v.clip_rect = Some(ClipRect {
            x: 120.0,
            y: 60.0,
            width: 900.0,
            height: 700.0,
        });
        let m = v.matrix();
        for point in [(0.0, 0.0), (4000.0, 3000.0), (777.0, 1234.0), (4000.0, 0.0)] {
            // 矩阵 → NDC
            let ndc = (
                m[0][0] * point.0 + m[1][0] * point.1 + m[3][0],
                m[0][1] * point.0 + m[1][1] * point.1 + m[3][1],
            );
            // 直接映射 → 物理 → NDC
            let physical = v.image_to_physical(point);
            let direct = (
                physical.0 / v.viewport_size.0 * 2.0 - 1.0,
                1.0 - physical.1 / v.viewport_size.1 * 2.0,
            );
            assert!(
                close(ndc.0, direct.0, 1e-4) && close(ndc.1, direct.1, 1e-4),
                "{point:?}：矩阵 {ndc:?} vs 直接 {direct:?}"
            );
        }
    }

    #[test]
    fn projected_corners_are_centered_when_fitted() {
        let mut v = vp();
        v.fit_mode = FitMode::Fit;
        v.refit();
        let corners = v.projected_corners_ndc();
        let sum_x: f32 = corners.iter().map(|c| c.0).sum();
        let sum_y: f32 = corners.iter().map(|c| c.1).sum();
        // 适配居中 ⇒ 四角 NDC 求和为 0
        assert!(close(sum_x, 0.0, 1e-3) && close(sum_y, 0.0, 1e-3), "{corners:?}");
        // 且都在 -1..1 之内（Fit 的定义）
        for (x, y) in corners {
            assert!(x.abs() <= 1.001 && y.abs() <= 1.001, "Fit 之后不该出界：{corners:?}");
        }
        assert_eq!(v.sanity_problems(), Vec::<String>::new());
    }

    #[test]
    fn sanity_flags_degenerate_states() {
        let mut v = vp();
        v.viewport_size = (0.0, 0.0);
        v.image_size = (0, 0);
        v.clip_rect = Some(ClipRect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        });
        let problems = v.sanity_problems();
        assert!(problems.len() >= 3, "{problems:?}");
    }

    #[test]
    fn zero_size_inputs_do_not_panic_or_divide_by_zero() {
        let mut v = Viewport {
            image_size: (0, 0),
            viewport_size: (0.0, 0.0),
            ..Default::default()
        };
        assert!(v.zoom_for_fit().is_finite());
        v.refit();
        let mapped = v.image_to_physical((0.0, 0.0));
        assert!(mapped.0.is_finite() && mapped.1.is_finite());
        let back = v.physical_to_image(mapped);
        assert!(back.0.is_finite() && back.1.is_finite());
        assert!(v.scissor().is_none());
        assert!(v.matrix().iter().flatten().all(|n| n.is_finite()));
    }

    #[test]
    fn css_physical_conversion_round_trips_at_every_dpr() {
        for dpr in [1.0_f32, 1.25, 1.5, 2.0] {
            let mut v = vp();
            v.dpr = dpr;
            for css in [(0.0, 0.0), (123.5, 456.25), (1600.0, 1200.0)] {
                let back = v.physical_to_css(v.css_to_physical(css));
                assert!(close(back.0, css.0, 1e-3) && close(back.1, css.1, 1e-3));
            }
        }
    }

    #[test]
    fn pointer_to_image_matches_manual_scaling() {
        // 前端送 CSS 像素、Rust 换算 —— 中间少乘一次 dpr 是这里最容易犯的错
        let mut v = vp();
        v.dpr = 1.5;
        v.fit_mode = FitMode::Fit;
        v.refit();
        let css = (400.0, 300.0); // 洞口取整窗时是 1600×1200 物理 = 1066.7×800 CSS
        let image = v.pointer_to_image(css);
        let physical = (css.0 * 1.5, css.1 * 1.5);
        let expected = v.physical_to_image(physical);
        assert!(close(image.0, expected.0, 1e-3) && close(image.1, expected.1, 1e-3));
    }

    #[test]
    fn overlay_transform_agrees_with_the_physical_mapping() {
        // 覆盖层矩阵必须与**渲染那套** `image_to_physical` 给出同一个结果
        // （两份数学一旦分家，覆盖层就会与照片错位 —— 这条测试就是防它的）
        let viewport = Viewport {
            viewport_size: (1600.0, 1200.0),
            image_size: (4000, 3000),
            clip_rect: Some(ClipRect {
                x: 320.0,
                y: 140.0,
                width: 960.0,
                height: 900.0,
            }),
            dpr: 1.5,
            zoom: 0.42,
            pan_px: (37.0, -21.0),
            rotation: 17.0,
            ..Viewport::default()
        };
        let matrix = viewport.css_overlay_transform().expect("有洞口就有矩阵");
        for point in [(0.0f32, 0.0f32), (4000.0, 3000.0), (1234.5, 987.25)] {
            let physical = viewport.image_to_physical(point);
            let css = viewport.physical_to_css(physical);
            let rect = viewport.clip_rect.expect("有洞口");
            // 洞口内的 CSS = 物理减洞口原点再除 DPR（`physical_to_css` 是整窗口的，
            // 所以这里要自己减一次洞口原点 —— 这正是覆盖层要的那一套）
            let expected = (
                (physical.0 - rect.x) / viewport.dpr,
                (physical.1 - rect.y) / viewport.dpr,
            );
            let got = (
                matrix[0] * point.0 + matrix[2] * point.1 + matrix[4],
                matrix[1] * point.0 + matrix[3] * point.1 + matrix[5],
            );
            assert!(
                (got.0 - expected.0).abs() < 0.01 && (got.1 - expected.1).abs() < 0.01,
                "{point:?}：矩阵给 {got:?}，物理映射给 {expected:?}（CSS {css:?}）"
            );
        }
    }

    #[test]
    fn overlay_transform_is_identity_when_nothing_changes() {
        // 1:1、无旋转、无平移、DPR=1、洞口就是整窗口 ⇒ 图像像素直接等于 CSS 像素
        let viewport = Viewport {
            viewport_size: (1000.0, 800.0),
            image_size: (1000, 800),
            clip_rect: Some(ClipRect {
                x: 0.0,
                y: 0.0,
                width: 1000.0,
                height: 800.0,
            }),
            dpr: 1.0,
            zoom: 1.0,
            ..Viewport::default()
        };
        let matrix = viewport.css_overlay_transform().expect("有洞口");
        assert!((matrix[0] - 1.0).abs() < 1e-6, "a 应当是 1：{matrix:?}");
        assert!(matrix[1].abs() < 1e-6 && matrix[2].abs() < 1e-6);
        assert!((matrix[3] - 1.0).abs() < 1e-6, "d 应当是 1：{matrix:?}");
        assert!(matrix[4].abs() < 1e-6 && matrix[5].abs() < 1e-6);
    }

    #[test]
    fn overlay_transform_scales_by_zoom_over_dpr() {
        let viewport = Viewport {
            viewport_size: (1000.0, 800.0),
            image_size: (1000, 800),
            clip_rect: Some(ClipRect {
                x: 0.0,
                y: 0.0,
                width: 1000.0,
                height: 800.0,
            }),
            dpr: 2.0,
            zoom: 0.5,
            ..Viewport::default()
        };
        let matrix = viewport.css_overlay_transform().expect("有洞口");
        // 0.5 倍缩放、DPR 2 ⇒ CSS 比例 = 0.25
        assert!((matrix[0] - 0.25).abs() < 1e-6, "a = zoom/dpr：{matrix:?}");
        // 图像中心落在**洞口的 CSS 中心**：洞口 1000×800 物理、DPR 2 ⇒ CSS 500×400，
        // 中心就是 (250, 200)
        let center = (
            matrix[0] * 500.0 + matrix[2] * 400.0 + matrix[4],
            matrix[1] * 500.0 + matrix[3] * 400.0 + matrix[5],
        );
        assert!((center.0 - 250.0).abs() < 0.01 && (center.1 - 200.0).abs() < 0.01, "{center:?}");
    }

    #[test]
    fn compare_uses_hole_and_dpr_and_clamps_pointer() {
        let mut viewport = vp();
        viewport.dpr = 1.5;
        viewport.clip_rect = Some(ClipRect { x: 150.0, y: 75.0, width: 900.0, height: 600.0 });
        assert_eq!(viewport.compare_fraction_at(400.0), Some(0.5));
        assert_eq!(viewport.compare_css_x(0.5), Some(300.0));
        assert_eq!(viewport.compare_fraction_at(-100.0), Some(0.0));
        assert_eq!(viewport.compare_fraction_at(2000.0), Some(1.0));
        assert!(viewport.compare_fraction_at(f32::NAN).is_none());
        assert!(viewport.compare_handle_hit((400.0, 100.0), 0.5));
        assert!(!viewport.compare_handle_hit((430.0, 100.0), 0.5));
        assert!(!viewport.compare_handle_hit((400.0, 900.0), 0.5));
        let [left, right] = viewport.compare_scissors(0.5).expect("有洞口");
        assert_eq!(left, (150, 75, 450, 600));
        assert_eq!(right, (600, 75, 450, 600));
        assert_eq!(viewport.compare_scissors(0.0).unwrap()[0].2, 0);
        assert_eq!(viewport.compare_scissors(1.0).unwrap()[1].2, 0);
    }

    #[test]
    fn compare_gesture_drags_the_handle_only_when_it_starts_on_it() {
        let mut viewport = vp();
        viewport.dpr = 1.5;
        viewport.clip_rect = Some(ClipRect { x: 150.0, y: 75.0, width: 900.0, height: 600.0 });
        let mut gesture = CompareGesture::default();

        // 起手在分线上（CSS 400 = 物理 600 = 洞口 150+900×0.5）
        assert_eq!(gesture.step(&viewport, 0.5, "down", (400.0, 100.0)), CompareGestureStep::Idle);
        // 拖到 0.75（CSS 550 ⇒ 物理 825 = 150 + 900×0.75）
        assert_eq!(
            gesture.step(&viewport, 0.5, "move", (550.0, 100.0)),
            CompareGestureStep::Handle(0.75),
        );
        // 松手那一下自带最终位置：尾样本要算进去
        assert_eq!(
            gesture.step(&viewport, 0.75, "up", (250.0, 120.0)),
            CompareGestureStep::Handle(0.25),
        );
        // 手势结束：后面的 move 不再属于它
        assert_eq!(gesture.step(&viewport, 0.5, "move", (600.0, 100.0)), CompareGestureStep::Idle);
    }

    #[test]
    fn compare_gesture_pans_the_image_when_it_starts_off_the_handle() {
        let mut viewport = vp();
        viewport.dpr = 2.0;
        viewport.clip_rect = Some(ClipRect { x: 150.0, y: 75.0, width: 900.0, height: 600.0 });
        let mut gesture = CompareGesture::default();

        // 起手离分线很远（0.5 的线在 CSS 400；在 100 处起手）
        assert_eq!(gesture.step(&viewport, 0.5, "down", (100.0, 200.0)), CompareGestureStep::Idle);
        // 阈值内（3 CSS px）不算拖动：免得点一下就退出「适合窗口」
        assert_eq!(gesture.step(&viewport, 0.5, "move", (102.0, 200.0)), CompareGestureStep::Idle);
        // 越过阈值：一次性把**从起手算起**的位移补上（CSS → 物理乘 DPR）
        assert_eq!(
            gesture.step(&viewport, 0.5, "move", (110.0, 190.0)),
            CompareGestureStep::Pan((20.0, -20.0)),
        );
        // 之后按逐样本位移走
        assert_eq!(
            gesture.step(&viewport, 0.5, "move", (115.0, 200.0)),
            CompareGestureStep::Pan((10.0, 20.0)),
        );
        // 松手：尾样本不能丢（否则图像停在半路）
        assert_eq!(
            gesture.step(&viewport, 0.5, "up", (120.0, 200.0)),
            CompareGestureStep::Pan((10.0, 0.0)),
        );
        assert_eq!(gesture.step(&viewport, 0.5, "move", (200.0, 200.0)), CompareGestureStep::Idle);

        // 全程位移 = 手指位移 × DPR（20−20−10−0 = 50 CSS ⇒ 100 物理）
        let mut gesture = CompareGesture::default();
        gesture.step(&viewport, 0.5, "down", (100.0, 200.0));
        let mut moved = 0.0_f32;
        for css in [110.0_f32, 115.0, 120.0] {
            if let CompareGestureStep::Pan((dx, _)) = gesture.step(&viewport, 0.5, "move", (css, 200.0)) {
                moved += dx;
            }
        }
        assert!((moved - 40.0).abs() < 1e-4, "CSS 20 px ⇒ 物理 40 px，实际 {moved}");
    }

    #[test]
    fn compare_gesture_cancel_and_bad_input_do_nothing() {
        let mut viewport = vp();
        viewport.dpr = 1.0;
        viewport.clip_rect = Some(ClipRect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 });
        let mut gesture = CompareGesture::default();
        gesture.step(&viewport, 0.5, "down", (10.0, 10.0));
        assert_eq!(gesture.step(&viewport, 0.5, "cancel", (200.0, 200.0)), CompareGestureStep::Idle);
        // 取消之后这个手势就死了
        assert_eq!(gesture.step(&viewport, 0.5, "move", (300.0, 300.0)), CompareGestureStep::Idle);
        // NaN 坐标不改变任何状态
        gesture.step(&viewport, 0.5, "down", (10.0, 10.0));
        assert_eq!(
            gesture.step(&viewport, 0.5, "move", (f32::NAN, 10.0)),
            CompareGestureStep::Idle,
        );
        assert_eq!(
            gesture.step(&viewport, 0.5, "move", (100.0, 10.0)),
            CompareGestureStep::Pan((90.0, 0.0)),
        );
    }

    #[test]
    fn cover_size_fills_the_window_without_distorting() {
        let mut v = vp();
        v.image_size = (3000, 2000); // 当前画面 3:2
        // 比例相同（SOOC 那种：参照就是按当前几何裁的）⇒ cover 后正好等于当前画面
        assert_eq!(v.cover_size((1500, 1000)), (3000.0, 2000.0));
        assert_eq!(v.cover_size((3000, 2000)), (3000.0, 2000.0));
        // 更方（4:3）⇒ 等比放大到宽度对齐，高比当前多（多出去的交给 scissor）
        let (w, h) = v.cover_size((4000, 3000));
        assert!(close(w, 3000.0, 1e-3) && close(h, 2250.0, 1e-3), "{w}x{h}");
        assert!(h > 2000.0, "比当前高——这部分会被裁掉");
        // 更宽（16:9）⇒ 高对齐、宽超出；比例始终是参照自己的比例
        let (w, h) = v.cover_size((3840, 2160));
        assert!(close(h, 2000.0, 1e-3) && w > 3000.0, "{w}x{h}");
        assert!(close(w / h, 3840.0 / 2160.0, 1e-3), "不能硬拉伸");
        // 退化输入不 panic（交回一个 1×1 级的可用值）
        assert_eq!(v.cover_size((0, 0)), (1.0, 1.0));
    }

    /// 矩阵投影：图像像素 → NDC。**按 GPU 的做法带齐次 `w` 再除**：
    /// 平移列的 w 写错（该 1 写成 0）时整个四边形会被裁掉 ——
    /// 手写 `m[3][0]` 的投影看不出来，只有这把尺子能提前量到（2026-10-08 踩过）。
    fn project(matrix: &[[f32; 4]; 4], point: (f32, f32)) -> (f32, f32) {
        let x = matrix[0][0] * point.0 + matrix[1][0] * point.1 + matrix[3][0];
        let y = matrix[0][1] * point.0 + matrix[1][1] * point.1 + matrix[3][1];
        let w = matrix[0][3] * point.0 + matrix[1][3] * point.1 + matrix[3][3];
        (x / w, y / w)
    }

    /// 一个画布在 NDC 里的矩形：`(left, top, right, bottom)`（top 的 NDC y 更大）
    fn ndc_rect(viewport: &Viewport, matrix: &[[f32; 4]; 4], size: (f32, f32)) -> (f32, f32, f32, f32) {
        let a = project(matrix, (0.0, 0.0));
        let b = project(matrix, size);
        (
            a.0.min(b.0),
            a.1.max(b.1),
            a.0.max(b.0),
            a.1.min(b.1),
        )
    }

    #[test]
    fn reference_cover_matrix_fills_the_current_window_and_keeps_its_own_aspect() {
        let mut viewport = vp();
        viewport.clip_rect = Some(ClipRect { x: 0.0, y: 0.0, width: 1600.0, height: 1200.0 });
        viewport.image_size = (4000, 3000); // 当前画面 4:3
        viewport.zoom = 0.3; // 适配那种情形：画面比洞口小（左右有留白）
        let current = ndc_rect(&viewport, &viewport.matrix(), (4000.0, 3000.0));

        // 参照 1:1（比当前更方）⇒ 应上下超出、左右对齐
        let cover = viewport.cover_size((3000, 3000));
        let reference = ndc_rect(&viewport, &viewport.matrix_for(cover), cover);
        assert!(reference.0 <= current.0 + 1e-3 && reference.2 >= current.2 - 1e-3, "左右必须盖住：{reference:?} vs {current:?}");
        assert!(reference.1 >= current.1 - 1e-3 && reference.3 <= current.3 + 1e-3, "上下必须盖住：{reference:?} vs {current:?}");
        // 只应有一根轴超出（cover 的紧贴性：另一根轴正好对齐）
        let over_w = (reference.2 - reference.0) - (current.2 - current.0);
        let over_h = (reference.1 - reference.3) - (current.1 - current.3);
        assert!(over_w.min(over_h).abs() < 2e-3, "只应一根轴超出：{over_w} / {over_h}（ref {reference:?} / cur {current:?}）");
        assert!(over_w.max(over_h) > 1e-3, "另一根轴必须真的超出（才能盖满窗口）");
        // 中心重合（超出部分两边均分 ⇒ 居中裁切）
        let current_mid = ((current.0 + current.2) / 2.0, (current.1 + current.3) / 2.0);
        let reference_mid = ((reference.0 + reference.2) / 2.0, (reference.1 + reference.3) / 2.0);
        assert!(close(current_mid.0, reference_mid.0, 1e-3) && close(current_mid.1, reference_mid.1, 1e-3), "{current_mid:?} vs {reference_mid:?}");
        /*
         * 比例是参照自己的（没有硬拉伸）：1:1 的参照在屏幕上仍是 1:1。
         * ⚠️ NDC 两轴刻度不同（除的是 viewport 宽高），比值必须换回**物理像素**再比。
         */
        let physical_aspect = ((reference.2 - reference.0) / 2.0 * viewport.viewport_size.0)
            / ((reference.1 - reference.3) / 2.0 * viewport.viewport_size.1);
        assert!(close(physical_aspect, 1.0, 2e-3), "参照 1:1 就该是 1:1，实际 {physical_aspect}");
    }

    #[test]
    fn reference_scissor_is_the_image_rect_inside_the_hole() {
        let mut viewport = vp();
        viewport.viewport_size = (1000.0, 800.0);
        viewport.clip_rect = Some(ClipRect { x: 100.0, y: 100.0, width: 800.0, height: 600.0 });
        viewport.image_size = (1000, 1000);
        viewport.pan_px = (0.0, 0.0);
        viewport.zoom = 0.5; // 画面 500×500，居在洞口中心 (500, 400) ⇒ 矩形 (250,150)-(750,650)
        let rect = viewport.image_rect();
        assert!(close(rect.x, 250.0, 1e-3) && close(rect.y, 150.0, 1e-3) && close(rect.width, 500.0, 1e-3), "{rect:?}");
        let [left, _] = viewport.compare_scissors(0.5).expect("有洞口");
        // 洞口左半 = (100,100)-(500,700)：与画面矩形相交后，左边从 250 起、右边到 500
        assert_eq!(viewport.reference_scissor(left), Some((250, 150, 250, 500)));
        // 完全无交集（画面缩到左上角之外）⇒ 不画
        viewport.zoom = 0.05;
        viewport.pan_px = (-400.0, -350.0);
        assert_eq!(viewport.reference_scissor(left), None);
        // 放大到铺满洞口：交集就是洞口左半（与旧行为一致）
        viewport.zoom = 3.0;
        viewport.pan_px = (0.0, 0.0);
        let [left, _] = viewport.compare_scissors(0.5).expect("有洞口");
        assert_eq!(viewport.reference_scissor(left), Some(left));
    }

    #[test]
    fn cover_reference_matches_the_old_shared_quad_when_the_aspect_is_the_same() {
        /*
         * **这就是「SOOC 路径行为不变」的机器证据**（崔总 2026-10-08 问的）：
         * SOOC 参照帧是按**当前几何**裁出来的，比例与当前画面相同，只是像素尺寸不同
         * （帧是 1920 档，当前画面是原图逻辑尺寸）—— cover 后缩放系数正好把参照放成当前画面大小，
         * 于是参照的 quad/矩阵与「与主图共用同一个 uniform」的旧做法**逐位相同**。
         */
        let mut viewport = vp();
        viewport.image_size = (4000, 3000);
        viewport.clip_rect = Some(ClipRect { x: 0.0, y: 0.0, width: 1600.0, height: 1200.0 });
        viewport.zoom = 0.4;
        viewport.pan_px = (37.0, -12.0);
        let cover = viewport.cover_size((1920, 1440)); // 同一 4:3 的 SOOC 帧
        /*
         * 比例相同 ⇒ cover 后就是当前画面尺寸。**不是逐位相等**：`w * scale` 里
         * f32 除法与乘法各舍入一次，实测差 0.0002 图像像素（约 1 个 ULP）——
         * 旧做法是把这同一个舍入差当拉伸吸收掉，谈不上「更准」。
         */
        assert!(close(cover.0, 4000.0, 0.001) && close(cover.1, 3000.0, 0.001), "{cover:?}");
        // 零可见差异的判据：两套矩阵投出来的四角一致（1e-5 NDC ≈ 千分之几个屏幕像素）
        let reference = ndc_rect(&viewport, &viewport.matrix_for(cover), cover);
        let current = ndc_rect(&viewport, &viewport.matrix(), (4000.0, 3000.0));
        for (left, right) in [
            (reference.0, current.0),
            (reference.1, current.1),
            (reference.2, current.2),
            (reference.3, current.3),
        ] {
            assert!(close(left, right, 1e-5), "SOOC 路径不该有可见差异：{reference:?} vs {current:?}");
        }
    }

    #[test]
    fn cover_reference_stays_sub_pixel_when_the_aspect_only_differs_by_rounding() {
        /*
         * 更接近真实的一种：SOOC 帧的比例与当前画面**只差舍入**（两边各自按自己的分辨率取整裁切）。
         * 例：1920×1280 的帧（3:2）对 4000×2667 的当前画面。
         * 旧做法：拉伸到当前矩形（非等比）——横/纵差约 0.01%；
         * 新做法：等比 cover + 裁 ——纵向完全对齐，横向多出不到 1 物理像素。
         * 两种做法在屏幕上都是零可见差异；下面把数字钉住，以后一眼能看出这个前提变了没有。
         */
        let mut viewport = vp();
        viewport.image_size = (4000, 2667);
        viewport.clip_rect = Some(ClipRect { x: 0.0, y: 0.0, width: 1600.0, height: 1200.0 });
        viewport.zoom = 0.4;
        let cover = viewport.cover_size((1920, 1280));
        assert!(close(cover.1, 2667.0, 0.001), "高度对齐到当前画面：{}", cover.1);
        let overflow_px = (cover.0 - 4000.0) * viewport.zoom; // 屏幕上多出来的物理像素
        assert!(overflow_px >= 0.0 && overflow_px < 1.0, "横向多出 {overflow_px} 物理像素（应 < 1）");
        let relative = (cover.0 - 4000.0) / 4000.0;
        assert!(relative < 0.0002, "相对差 {relative} 应当小于 0.02%");
        // 比例不变（参照自己的比例），且居中：中心与当前画面中心重合
        assert!(close(cover.0 / cover.1, 1920.0 / 1280.0, 1e-4));
        let reference = ndc_rect(&viewport, &viewport.matrix_for(cover), cover);
        let current = ndc_rect(&viewport, &viewport.matrix(), (4000.0, 2667.0));
        let reference_mid = ((reference.0 + reference.2) / 2.0, (reference.1 + reference.3) / 2.0);
        let current_mid = ((current.0 + current.2) / 2.0, (current.1 + current.3) / 2.0);
        assert!(close(reference_mid.0, current_mid.0, 1e-4) && close(reference_mid.1, current_mid.1, 1e-4));
    }

    #[test]
    fn rotated_fit_and_crop_frame_share_the_same_css_hole() {
        use crate::develop::geometry::CropRect;
        let mut viewport = vp();
        viewport.image_size = (4000, 3000);
        viewport.dpr = 1.5;
        viewport.clip_rect = Some(ClipRect { x: 150.0, y: 75.0, width: 900.0, height: 600.0 });
        viewport.rotation = 45.0;
        viewport.refit_rotated();
        let bounds = (4000.0 * 45.0_f32.to_radians().cos().abs()
            + 3000.0 * 45.0_f32.to_radians().sin().abs()) * viewport.zoom;
        assert!(bounds <= 900.01, "旋转后整张原图要装进洞口");
        let center = viewport.frame_pointer_normalized((400.0, 250.0), (4000, 3000)).unwrap();
        assert!((center.0 - 0.5).abs() < 1e-5 && (center.1 - 0.5).abs() < 1e-5);
        let rect = CropRect { x: 0.25, y: 0.25, width: 0.5, height: 0.5 };
        let css = viewport.frame_rect_css(rect, (4000, 3000)).unwrap();
        assert!((css.x + css.width / 2.0 - 300.0).abs() < 1e-4);
        assert!((css.y + css.height / 2.0 - 200.0).abs() < 1e-4);
        assert!(viewport.frame_pointer_normalized((0.0, 0.0), (4000, 3000)).is_none());
    }

    #[test]
    fn overlay_transform_needs_a_hole() {
        let viewport = Viewport {
            clip_rect: None,
            ..Viewport::default()
        };
        assert!(viewport.css_overlay_transform().is_none(), "没有洞口就没有覆盖层");
    }
}
