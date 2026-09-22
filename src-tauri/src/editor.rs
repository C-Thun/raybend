//! 编辑视口的**洞口状态**（M3-W1 的契约层）。
//!
//! 这一层只做一件事：**把前端报上来的事实收下来，并算出它的物理像素版本**。
//! 它不做任何布局推导、不猜 DPR、不补偏移 —— 那是 §7.9 真机事故的全部教训。
//!
//! ```text
//!   前端（CSS 矩形 + 运行时 DPR + CSS 视口尺寸）
//!            │  editor_set_viewport
//!            ▼
//!   EditorState（进程级，Mutex<Stored>）   ← W2 的渲染线程按它摆图 / 裁切 / 命中测试
//!            │  editor_viewport_state
//!            ▼
//!   回读（诊断 / 冒烟：CSS 与物理两份都带回去，一比就知道单位有没有错）
//! ```
//!
//! **为什么要回读**：`AGENTS.md` §7.9 的四个「假通过」里有两个都是「程序自己跟自己对」。
//! 回读把前端报的 CSS 值与 Rust 算的物理值放在一起，肉眼一比就能看出
//! 「DPR 有没有乘进去、乘对没有」——这不需要真机、也不需要渲染线程。
//!
//! W2 接渲染线程时，这个模块是**唯一的入口**：线程从 [`EditorState::snapshot`] 取事实，
//! 前端继续只报原始值。

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use tauri::State;

/// 一个矩形（**单位由字段名标明**：`hole_css` 是 CSS 像素，`hole_physical` 是物理像素）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct RectDto {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl RectDto {
    /// 每个数都是有限数、且宽高非负。
    ///
    /// 不做「取绝对值」「夹到 0」这类补救：非法值说明上游算错了，
    /// 静默修好只会把错误带到下一层（§7.9：静默回退比报错可怕）。
    fn is_valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width >= 0.0
            && self.height >= 0.0
    }
}

/// 一个尺寸（CSS 像素）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct SizeDto {
    pub width: f64,
    pub height: f64,
}

impl SizeDto {
    fn is_valid(self) -> bool {
        self.width.is_finite() && self.height.is_finite() && self.width >= 0.0 && self.height >= 0.0
    }
}

/// 前端 `editor_set_viewport` 的一次载荷（**原始事实**，见模块文档）。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetViewportArgs {
    pub hole: RectDto,
    /// WebView 的 `devicePixelRatio`：**含显示器 DPI + 系统文字缩放 + 页面缩放**。
    /// 不许拿 Tauri 的 `scale_factor()` 顶替（它不含文字缩放，§7.9 铁律 3）。
    pub dpr: f64,
    pub viewport: SizeDto,
}

/// 校验后的事实（**只有合法的值才进得来**）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValidatedViewport {
    pub hole: RectDto,
    pub dpr: f64,
    pub viewport: SizeDto,
}

/// 校验 + 换算成物理像素。纯函数，边界都在测试里钉住。
///
/// # Errors
/// 任一数字不是有限数、宽高为负、或 `dpr <= 0`（它是所有 CSS→物理换算的分母）。
pub fn validate(args: SetViewportArgs) -> Result<ValidatedViewport, String> {
    if !args.hole.is_valid() {
        return Err(format!("洞口矩形非法：{:?}", args.hole));
    }
    if !args.viewport.is_valid() {
        return Err(format!("视口尺寸非法：{:?}", args.viewport));
    }
    if !args.dpr.is_finite() || args.dpr <= 0.0 {
        return Err(format!("DPR 必须是有限正数，收到 {}", args.dpr));
    }
    Ok(ValidatedViewport {
        hole: args.hole,
        dpr: args.dpr,
        viewport: args.viewport,
    })
}

/// CSS 矩形 → 物理像素矩形（只有这一步乘 DPR，别处都不许再乘）。
#[must_use]
pub fn physical_rect(hole: RectDto, dpr: f64) -> RectDto {
    RectDto {
        x: hole.x * dpr,
        y: hole.y * dpr,
        width: hole.width * dpr,
        height: hole.height * dpr,
    }
}

/// 收到的事实（存起来的那一份）。
#[derive(Debug, Clone, Copy, Default)]
pub struct StoredViewport {
    pub hole_css: Option<RectDto>,
    pub dpr: f64,
    pub viewport: SizeDto,
    /// 收到过几次（诊断：一直不涨 = 前端根本没报）
    pub updates: u64,
}

impl StoredViewport {
    /// 物理像素洞口（还没收到过就是 `None`）。
    #[must_use]
    pub fn hole_physical(&self) -> Option<RectDto> {
        self.hole_css.map(|hole| physical_rect(hole, self.dpr))
    }

    /// 存下一次上报。
    pub fn apply(&mut self, next: ValidatedViewport) {
        self.hole_css = Some(next.hole);
        self.dpr = next.dpr;
        self.viewport = next.viewport;
        self.updates = self.updates.saturating_add(1);
    }
}

/// 回给前端的整块状态。
#[derive(Debug, Clone, Copy, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ViewportStateDto {
    pub hole_css: Option<RectDto>,
    pub hole_physical: Option<RectDto>,
    pub dpr: f64,
    pub viewport_css: SizeDto,
    pub updates: u64,
}

impl StoredViewport {
    /// 快照（渲染线程与回读命令共用同一个入口）。
    #[must_use]
    pub fn snapshot(&self) -> ViewportStateDto {
        ViewportStateDto {
            hole_css: self.hole_css,
            hole_physical: self.hole_physical(),
            dpr: self.dpr,
            viewport_css: self.viewport,
            updates: self.updates,
        }
    }
}

/// 挂在 Tauri 状态上的编辑器会话状态（M3-W1 只有视口事实）。
#[derive(Default)]
pub struct EditorState {
    viewport: Mutex<StoredViewport>,
}

impl EditorState {
    /// 取一份快照（**唯一读法**：锁不会漏出去 —— 渲染线程以后也走这里）。
    ///
    /// # Errors
    /// 锁中毒（有线程在持锁时 panic）时返回错误字符串，而不是 `unwrap` 让命令层 panic。
    pub fn snapshot(&self) -> Result<ViewportStateDto, String> {
        let guard = self
            .viewport
            .lock()
            .map_err(|_| "编辑器视口状态锁中毒（有线程持锁时崩过）".to_string())?;
        Ok(guard.snapshot())
    }
}

/// 前端上报视口事实（`EDITOR.md` 的洞口契约；前端**只报原始值**）。
#[tauri::command]
pub fn editor_set_viewport(
    state: State<'_, EditorState>,
    hole: RectDto,
    dpr: f64,
    viewport: SizeDto,
) -> Result<ViewportStateDto, String> {
    let validated = validate(SetViewportArgs { hole, dpr, viewport })?;
    let mut guard = state
        .viewport
        .lock()
        .map_err(|_| "编辑器视口状态锁中毒".to_string())?;
    guard.apply(validated);
    Ok(guard.snapshot())
}

/// 回读 Rust 手里的视口事实（诊断 / 冒烟）。
#[tauri::command]
pub fn editor_viewport_state(state: State<'_, EditorState>) -> Result<ViewportStateDto, String> {
    state.snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(dpr: f64) -> SetViewportArgs {
        SetViewportArgs {
            hole: RectDto { x: 300.0, y: 120.0, width: 1280.0, height: 720.0 },
            dpr: dpr,
            viewport: SizeDto { width: 1920.0, height: 1080.0 },
        }
    }

    #[test]
    fn physical_rect_multiplies_dpr_once() {
        // 1.375 = 系统 125% × 页面 110%（§7.9 那次事故的真实比例）
        let hole = RectDto { x: 10.0, y: 20.0, width: 100.0, height: 50.0 };
        let physical = physical_rect(hole, 1.375);
        assert!((physical.x - 13.75).abs() < 1e-9);
        assert!((physical.y - 27.5).abs() < 1e-9);
        assert!((physical.width - 137.5).abs() < 1e-9);
        assert!((physical.height - 68.75).abs() < 1e-9);
    }

    #[test]
    fn validate_rejects_nonsense() {
        assert!(validate(args(1.0)).is_ok());
        assert!(validate(args(0.0)).is_err(), "dpr 不能是 0：它是换算的分母");
        assert!(validate(args(-1.0)).is_err());
        assert!(validate(args(f64::NAN)).is_err());
        assert!(validate(args(f64::INFINITY)).is_err());

        let mut bad = args(1.0);
        bad.hole.width = -1.0;
        assert!(bad.hole.is_valid().eq(&false), "负宽度非法");
        assert!(validate(bad).is_err());

        let mut nan = args(1.0);
        nan.viewport.height = f64::NAN;
        assert!(validate(nan).is_err());
    }

    #[test]
    fn zero_sized_hole_is_legal_but_kept_as_zero() {
        // 最小化 / 布局中间态会给 0 宽：**合法**（不是垃圾值），照实存下来
        let mut zero = args(1.0);
        zero.hole.width = 0.0;
        zero.hole.height = 0.0;
        let ok = validate(zero).expect("0 尺寸是合法输入");
        let mut stored = StoredViewport::default();
        stored.apply(ok);
        assert_eq!(stored.hole_physical().expect("有值").width, 0.0);
    }

    #[test]
    fn stored_keeps_last_value_and_counts_updates() {
        let mut stored = StoredViewport::default();
        assert!(stored.snapshot().hole_css.is_none());
        assert_eq!(stored.snapshot().updates, 0);

        stored.apply(validate(args(1.25)).expect("合法"));
        stored.apply(validate(args(1.375)).expect("合法"));
        let snapshot = stored.snapshot();
        assert_eq!(snapshot.updates, 2, "每一次上报都要计数");
        assert!((snapshot.dpr - 1.375).abs() < 1e-9);
        let physical = snapshot.hole_physical.expect("有洞口");
        assert!((physical.width - 1280.0 * 1.375).abs() < 1e-6);
        assert!(
            (physical.x - 300.0 * 1.375).abs() < 1e-6,
            "物理洞口必须带偏移，不能只算宽高"
        );
    }

    #[test]
    fn snapshot_serializes_with_camel_case_keys() {
        // 前端 `src/api/editor.ts` 读的是 camelCase —— 与 `src/api/dto-contract.json` 对齐
        let mut stored = StoredViewport::default();
        stored.apply(validate(args(2.0)).expect("合法"));
        let value = serde_json::to_value(stored.snapshot()).expect("能序列化");
        let object = value.as_object().expect("是个对象");
        let mut keys: Vec<&String> = object.keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec!["dpr", "holeCss", "holePhysical", "updates", "viewportCss"]
        );
    }
}
