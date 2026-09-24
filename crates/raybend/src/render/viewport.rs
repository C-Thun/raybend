//! 视口状态与坐标变换（`AGENTS.md` §6.1 红线 #1、#2）。
//!
//! > 视口状态由 Rust **独有**：前端只发送交互意图，不做坐标数学。
//!
//! 这个文件是整个渲染架构里**最容易被写错、又最贵**的一块：
//! 一旦坐标系的口径不统一，症状是「图比鼠标慢半拍」「缩放着缩着偏出去」，
//! 而且只在某些 DPI 下出现（`PLAN.md` A.2 记着 RapidRAW 踩过「差 1 像素就图跟不上鼠标」）。
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

    /// 图像像素 → NDC（-1..1）的 4×4 矩阵，**列主序**（WGSL 的 `mat4x4<f32>` 直接吃）。
    ///
    /// NDC 的定义域是**整个 surface**（不是洞口）：洞口靠 scissor 裁，
    /// 这样矩阵只依赖视口尺寸，缩放窗口时不需要重算矩阵以外的东西。
    pub fn matrix(&self) -> [[f32; 4]; 4] {
        let (w, h) = (self.viewport_size.0.max(1.0), self.viewport_size.1.max(1.0));
        let center = self.image_center();
        let area = self.rect_center();
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let scale = self.zoom;
        // 平移：把图像中心搬到 (area + pan)，再换到 NDC
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
        // 平移项里含 −center·R·z（把图像中心搬到原点）
        let tx_ndc = (tx - (a * center.0 + b * center.1) * w / 2.0) * 2.0 / w - 1.0;
        let ty_ndc = 1.0 - (ty + (c * center.0 + d * center.1) * h / 2.0) * 2.0 / h;
        [
            [a, c, 0.0, 0.0],
            [b, d, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [tx_ndc, ty_ndc, 0.0, 1.0],
        ]
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
    fn overlay_transform_needs_a_hole() {
        let viewport = Viewport {
            clip_rect: None,
            ..Viewport::default()
        };
        assert!(viewport.css_overlay_transform().is_none(), "没有洞口就没有覆盖层");
    }
}
