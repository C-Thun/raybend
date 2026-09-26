//! **全屏看图**：另开一个无边框窗口，按主窗口所在屏幕全屏显示当前那张照片。
//!
//! 三条设计口径（人类 2026-09-23 口述）：
//!
//! 1. **跟主窗口形态无关** —— 它是**另一个原生窗口**，主窗口最大化 / 分栏 / 在编辑工作流
//!    都不影响它（所以不是「把 webview 全屏」，而是真的开一个窗）；
//! 2. **落在主窗口所在那块屏幕上** —— 多屏时「我在这块屏上看图，全屏也该在这块屏」；
//! 3. **只支持单图**：清单 = 当前目录的显示序（与网格同一份来源），
//!    ←/→、PageUp/PageDown 在清单里步进；界面本身**没有任何控件**（沉浸式）。
//!
//! ```text
//!  主窗口（点 flowbar 的全屏按钮）
//!        │ fullscreen_open { items, index }
//!        ▼
//!  FullscreenState（清单 + 下标，进程级）      ← 页面挂载时用 fullscreen_payload 取一次
//!        │ 同时 emit "fullscreen://payload"   ← 窗口已在（连点两次开）时，页面收到就换图
//!        ▼
//!  全屏窗口（label = fullscreen-viewer，url = index.html?fullscreen=1）
//! ```
//!
//! ⚠️ **为什么清单走 Rust 而不是 URL**：一个目录几千张照片时 URL 装不下（也不该装）；
//! 而页面又要在「窗口已经在建/已在」时拿到新清单 —— 所以状态放 Rust、事件通知页面。
//!
//! ⚠️ **窗口不复用**（人类 2026-09-23：「怎么可能这个窗口还能复用啊？优化的点一定是
//! 载入速度而不是想着复用」）：`Esc` 走 [`fullscreen_close`] **真销毁**，每次打开都
//! 重新建窗、页面重新挂载并主动读一次清单 —— 「打开看到上一次那张」那条链从根上不存在。
//! 秒开靠三件事：建窗即带几何、窗口底色防白闪、Rust 侧图像缓存。
//! 旧实现为省建窗时间把 `Esc` 做成 `hide()`，重开靠事件 + `visibilitychange` 推新清单；
//! 而原生窗口 hide/show 不保证触发页面可见性变化、隐藏期间 WebView 也可能被挂起 ——
//! 清单就停在旧的。**别再回到那条路。**
//!
//! ⚠️ `current_monitor()` 属于「拖动期间可能阻塞」那一类窗口查询（`AGENTS.md` §7.9 的
//! 工程侧血泪第 4 条）。这里只在**用户点按钮的那一刻**问一次（不是渲染循环里反复问），
//! 而且那会儿手不可能同时在拖窗口 —— 所以可以接受。**别把它搬到每帧都跑的地方去。**

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State, WebviewUrl, WebviewWindowBuilder};

use crate::MAIN_WINDOW_LABEL;

/// 全屏看图窗口的标签（`src/index.tsx` 用 `?fullscreen=1` 那条查询串选页）。
pub const FULLSCREEN_LABEL: &str = "fullscreen-viewer";

/// 清单更新事件（页面已在运行时换图用）。
pub const FULLSCREEN_EVENT: &str = "fullscreen://payload";

/// 全屏页需要的一张照片及可选的明确导出稿引用。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullscreenItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_variant: Option<FullscreenVariant>,
    pub id: String,
    /// 绝对路径（`view_image` 直接吃它）
    pub path: String,
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullscreenVariant {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured: Option<serde_json::Value>,
    pub repository_id: String,
    pub reference: raybend::export::VariantRef,
}

/// 一次全屏会话的清单与当前下标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FullscreenPayload {
    /// **单调递增的版本号**（由 [`FullscreenState::store`] 递增）。
    ///
    /// 为什么需要它：页面挂载时的「取一次清单」与后来的「事件推新清单」**可能乱序到达** ——
    /// 初始读取若晚于事件返回，就会拿**旧清单覆盖新清单**（症状：换图后显示的还是上一张）。
    /// 页面只应用版本号更大的包，两条路径就都安全了（`AGENTS.md` §7.9 的 revision 口径）。
    pub revision: u64,
    pub items: Vec<FullscreenItem>,
    pub index: usize,
}

/// 校验一次开窗请求。**非法就报错，不静默夹取**（`AGENTS.md` §7.9 的老教训：
/// 把错的输入修好只会把错带到下一层）。
///
/// # Errors
/// 清单为空；或下标越界。
pub fn validate(items: &[FullscreenItem], index: usize) -> Result<(), String> {
    if items.is_empty() {
        return Err("全屏看图至少要有一张照片".to_string());
    }
    if index >= items.len() {
        return Err(format!(
            "全屏看图的下标越界：{index}（清单共 {} 张）",
            items.len()
        ));
    }
    Ok(())
}

/// 全屏会话状态（挂在 Tauri 上）。
#[derive(Default)]
pub struct FullscreenState {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// 已发放的最大版本号（每存一次 +1）
    revision: u64,
    payload: Option<FullscreenPayload>,
    /// 上一扇窗口已派发销毁、但还没从窗口管理器里消失（`destroy()` 是异步派发的）。
    /// 新开窗要等它先消失，否则会撞上「标签仍被占用」。
    closing: bool,
}

impl FullscreenState {
    /// 存一份新清单，**版本号在这里递增**（调用方不用管，返回带版本号的那一份去发事件）。
    fn store(&self, mut payload: FullscreenPayload) -> Result<FullscreenPayload, String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "全屏看图状态锁中毒".to_string())?;
        guard.revision += 1;
        payload.revision = guard.revision;
        guard.payload = Some(payload.clone());
        Ok(payload)
    }

    fn read(&self) -> Result<Option<FullscreenPayload>, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "全屏看图状态锁中毒".to_string())?;
        Ok(guard.payload.clone())
    }

    fn mark_closing(&self) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "全屏看图状态锁中毒".to_string())?;
        guard.closing = true;
        Ok(())
    }

    fn is_closing(&self) -> Result<bool, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "全屏看图状态锁中毒".to_string())?;
        Ok(guard.closing)
    }

    fn clear_closing(&self) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "全屏看图状态锁中毒".to_string())?;
        guard.closing = false;
        Ok(())
    }
}

/// 等上一扇窗口真正从窗口管理器里消失（`destroy()` 只是把销毁派发给事件循环，
/// 管理器里的条目要等运行时发出 `Destroyed` 才移除）。
///
/// 上限 500ms：真机上销毁是毫秒级的，等这么久基本只有一种情况 —— 系统卡住了；
/// 那时宁可去撞一次建窗错误（有复用兜底），也不要把命令挂死。
/// 用 `spawn_blocking` 让出 tokio worker：等的时候主线程还要跑事件循环去处理销毁。
async fn wait_until_window_gone<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        while handle.get_webview_window(FULLSCREEN_LABEL).is_some() {
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    })
    .await;
}

/// 主窗口**所在那块**屏幕（拿不到就 `None` → 交给系统决定开在哪）。
fn main_window_monitor<R: Runtime>(app: &AppHandle<R>) -> Option<tauri::Monitor> {
    let main = app.get_webview_window(MAIN_WINDOW_LABEL)?;
    match main.current_monitor() {
        Ok(found) => found,
        Err(error) => {
            eprintln!("[fullscreen] 取主窗口所在屏幕失败（按系统默认位置开窗）：{error}");
            None
        }
    }
}

/// 把窗口摆到主窗口那块屏幕的**整个矩形**上（物理像素）。
///
/// 每次打开都要调 —— 用户可能把主窗口拖到另一块屏之后再点全屏，
/// 「全屏跟着主窗口走」这条口径才是活的。
fn place_on_main_monitor<R: Runtime>(window: &tauri::WebviewWindow<R>, app: &AppHandle<R>) {
    let Some(monitor) = main_window_monitor(app) else {
        return;
    };
    let position = monitor.position();
    let size = monitor.size();
    let _ = window.set_position(tauri::PhysicalPosition::new(position.x, position.y));
    let _ = window.set_size(tauri::PhysicalSize::new(size.width, size.height));
}

/// 把窗口摆到主窗口那块屏幕上、全屏、拿到最前面。
fn focus_existing_window<R: Runtime>(window: &tauri::WebviewWindow<R>, app: &AppHandle<R>) {
    place_on_main_monitor(window, app);
    let _ = window.set_fullscreen(true);
    let _ = window.show();
    let _ = window.set_focus();
}

/// 打开全屏看图。
///
/// 正常路径是**每次新建窗口**（见模块头：不复用）。只有两种情况会走「窗口已在就换图」：
/// ① 用户连点两次按钮，第一扇还在建/已建好；② 等待旧窗销毁超时（500ms）但它其实还在。
///
/// `background` 是前端报的**画布底色**（CSS 颜色串，如 `#17191c`）：拿它设窗口背景色，
/// 消除 WebView 首帧的**白闪**。解不开不是错误（不设而已），与编辑视口那条上报同一口径。
///
/// # Errors
/// 清单 / 下标非法；或窗口建不起来（且确实没有可复用的窗口）。
#[tauri::command]
pub async fn fullscreen_open<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, FullscreenState>,
    items: Vec<FullscreenItem>,
    index: usize,
    background: Option<String>,
) -> Result<(), String> {
    validate(&items, index)?;
    // 存一份**带新版本号**的清单（页面据此丢弃迟到的旧包）
    let payload = state.store(FullscreenPayload {
        revision: 0,
        items,
        index,
    })?;
    let canvas = background
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .and_then(raybend::render::Srgb8::parse_css);

    // 已经开着（且不在销毁中）：通知页面换图（事件先发 —— 页面订阅着就立刻响应），
    // 再摆位/全屏/拿到前面。这是「连点两次开」那条路，不是正常开窗路径。
    if !state.is_closing()?
        && let Some(window) = app.get_webview_window(FULLSCREEN_LABEL)
    {
        let _ = app.emit(FULLSCREEN_EVENT, &payload);
        focus_existing_window(&window, &app);
        return Ok(());
    }

    /*
     * 上一扇正在销毁（`Esc` 之后马上又按 `F11`）：等它从管理器里消失再建新的。
     * 不等的话 `builder.build()` 会撞上「标签已被占用」，而那时的兜底是
     * 「复用」——一复用又回到了「打开看到上一次那张」的老路。
     */
    if state.is_closing()? {
        wait_until_window_gone(&app).await;
    }
    let monitor = main_window_monitor(&app);
    let mut builder = WebviewWindowBuilder::new(
        &app,
        FULLSCREEN_LABEL,
        // 与 spike 页同一种做法：查询串选页，dev 与打包版都不影响资源解析
        WebviewUrl::App("index.html?fullscreen=1".into()),
    )
    .title("RayBend")
    .decorations(false)
    .resizable(false)
    // 它不是一个「工作窗口」：不出现在任务栏、不抢 Alt+Tab 的位置感
    .skip_taskbar(true)
    .visible(false);

    /*
     * 建窗时就带上目标屏幕的**几何**与**底色**（人类 2026-09-23：「进全屏很慢，
     * 屏幕一半先黑、再全黑、再出图，中间还有几帧白框闪烁」）：
     *
     *   * **几何**：不先给尺寸的话，窗口会以默认尺寸出生在主屏、露一下再被挪到目标屏
     *     并放大 —— 那两下就是「一半先黑 → 再全黑」；
     *   * **底色**：WebView 首帧是**白**的，不设背景色就会闪白框。
     *
     * 两个都吃**逻辑像素**（builder 的 `position`/`inner_size` 是逻辑值），
     * 所以用监视器自己的 `scale_factor` 从物理值换过去 —— 这是**原生窗口**坐标系，
     * 与 WebView 的 DPR 不是同一个量（`AGENTS.md` §7.9），别混。
     */
    if let Some(monitor) = &monitor {
        let scale = monitor.scale_factor();
        if scale > 0.0 {
            let position = monitor.position();
            let size = monitor.size();
            builder = builder
                .position(f64::from(position.x) / scale, f64::from(position.y) / scale)
                .inner_size(
                    f64::from(size.width) / scale,
                    f64::from(size.height) / scale,
                );
        }
    }
    if let Some(canvas) = canvas {
        builder = builder.background_color(tauri::window::Color(canvas.r, canvas.g, canvas.b, 255));
    }

    let window = match builder.build() {
        Ok(window) => window,
        Err(error) => {
            /*
             * 两个现实场景会走到这里，都**不该**报错给用户：
             *   * **连点两下按钮**：第一下正在建窗，第二下也到这里 —— 标签已被占；
             *   * 等 500ms 后旧窗仍未从管理器消失（极少见）。
             * 这时若窗口已在就退化成「复用 + 换图」（事件在这里补发，页面收到就换），
             * 确实不在才把真错报出去。
             */
            if let Some(window) = app.get_webview_window(FULLSCREEN_LABEL) {
                let _ = app.emit(FULLSCREEN_EVENT, &payload);
                focus_existing_window(&window, &app);
                return Ok(());
            }
            return Err(format!("建全屏看图窗口失败：{error}"));
        }
    };

    /*
     * 再摆一次位（`monitor` 取不到时上面那段没跑）与全屏：
     * `set_fullscreen(true)` 是「在窗口**当前所在**屏幕上全屏」，所以必须在摆位之后调 ——
     * 顺序反了会先在主屏全屏一下再被挪过去。
     */
    focus_existing_window(&window, &app);
    state.clear_closing()?;
    Ok(())
}

/// 页面挂载时取一次清单（页面刷新 / 重新加载都靠它）。
///
/// # Errors
/// 锁中毒。
#[tauri::command]
pub fn fullscreen_payload(
    state: State<'_, FullscreenState>,
) -> Result<Option<FullscreenPayload>, String> {
    state.read()
}

/// 关掉全屏看图（`Esc` / `Enter` 都走它）。
///
/// **真销毁**（人类 2026-09-23 定：「怎么可能这个窗口还能复用啊？优化的点一定是载入速度
/// 而不是想着复用」）。旧实现为了「秒开」只 `hide()`，重开靠事件 + `visibilitychange`
/// 推新清单 —— 而原生窗口 hide/show 不保证触发页面可见性变化、隐藏期间 WebView 也可能
/// 被挂起，清单就停在旧的（症状：打开看到上一次那张）。
///
/// 秒开靠三件事，不靠复用窗口：① 建窗即带几何；② 窗口底色防白闪；③ Rust 侧图像缓存。
/// 销毁是**异步派发**的：这里记一笔 `closing`，[`fullscreen_open`] 据此等旧窗消失再建新的
/// （不记的话「`Esc` 后立刻再开」会撞上标签占用，兜底复用又回到旧毛病）。
///
/// 用 `Alt+F4` / 系统方式关掉时窗口同样是真销毁 —— 那条路不经过这里，
/// 所以 `closing` 不会被置位；`fullscreen_open` 发现窗口不在就直接建，两条路都成立。
///
/// # Errors
/// 窗口存在但销毁失败（不存在算成功 —— 幂等）。
#[tauri::command]
pub async fn fullscreen_close<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, FullscreenState>,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(FULLSCREEN_LABEL) {
        state.mark_closing()?;
        window
            .destroy()
            .map_err(|error| format!("销毁全屏看图窗口失败：{error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str) -> FullscreenItem {
        FullscreenItem {
            id: format!("id-{name}"),
            path: format!("/photos/{name}"),
            file_name: name.to_string(),
            export_variant: None,
        }
    }

    #[test]
    fn issue_reference_roundtrips_and_legacy_items_remain_compatible() {
        let legacy = serde_json::from_str::<FullscreenItem>(
            r#"{"id":"1","path":"C:/photos/a.jpg","fileName":"a.jpg"}"#,
        )
        .unwrap();
        assert!(legacy.export_variant.is_none());
        let issue = FullscreenItem {
            export_variant: Some(FullscreenVariant {
                captured: Some(serde_json::json!({"profileHash":"stable"})),
                repository_id: "中文库".into(),
                reference: raybend::export::VariantRef {
                    asset_id: 1,
                    variant: "issue:2".into(),
                },
            }),
            ..legacy
        };
        let json = serde_json::to_value(&issue).unwrap();
        assert_eq!(json["exportVariant"]["reference"]["assetId"], 1);
        assert_eq!(json["exportVariant"]["captured"]["profileHash"], "stable");
        assert_eq!(
            serde_json::from_value::<FullscreenItem>(json).unwrap(),
            issue
        );
    }

    #[test]
    fn validate_rejects_empty_and_out_of_range() {
        assert!(validate(&[], 0).is_err(), "空清单要报错");
        let items = [item("a.jpg"), item("b.jpg")];
        assert!(validate(&items, 0).is_ok());
        assert!(validate(&items, 1).is_ok(), "最后一张是合法的");
        assert!(validate(&items, 2).is_err(), "越界要报错（不夹取）");
        assert!(validate(&items, usize::MAX).is_err());
    }

    #[test]
    fn payload_serializes_with_camel_case_keys() {
        // 页面读的是 camelCase —— 与 `src/api/dto-contract.json` 对齐
        let payload = FullscreenPayload {
            revision: 1,
            items: vec![item("a.jpg")],
            index: 0,
        };
        let value = serde_json::to_value(&payload).expect("能序列化");
        let object = value.as_object().expect("是个对象");
        let mut keys: Vec<&String> = object.keys().collect();
        keys.sort();
        assert_eq!(keys, vec!["index", "items", "revision"]);

        let first = &value["items"][0];
        let mut item_keys: Vec<&String> = first.as_object().expect("是个对象").keys().collect();
        item_keys.sort();
        assert_eq!(item_keys, vec!["fileName", "id", "path"]);
    }

    #[test]
    fn state_keeps_the_last_payload_and_reads_it_back() {
        let state = FullscreenState::default();
        assert!(state.read().expect("读得到").is_none(), "一开始没有清单");

        let stored = state
            .store(FullscreenPayload {
                revision: 0,
                items: vec![item("a.jpg"), item("b.jpg")],
                index: 1,
            })
            .expect("存得进");
        assert_eq!(stored.revision, 1, "版本号由 store 递增（调用方填 0 即可）");

        let payload = state.read().expect("读得到").expect("有值");
        assert_eq!(payload.index, 1);
        assert_eq!(payload.items.len(), 2);
        assert_eq!(payload.items[0].file_name, "a.jpg");
        assert_eq!(payload.revision, 1);
    }

    #[test]
    fn closing_flag_tracks_destroy_window() {
        // `fullscreen_open` 靠它决定「等旧窗消失再建新的」还是「直接复用」——
        // 置了不清会让下次开窗白等 500ms，清了不该清的会撞标签占用。
        let state = FullscreenState::default();
        assert!(!state.is_closing().expect("读得到"), "一开始不在销毁中");
        state.mark_closing().expect("置得上");
        assert!(state.is_closing().expect("读得到"));
        state.clear_closing().expect("清得掉");
        assert!(!state.is_closing().expect("读得到"));
    }

    #[test]
    fn revision_increases_on_every_store() {
        // 页面靠它丢弃迟到的旧清单 —— 不递增等于白加这个字段
        let state = FullscreenState::default();
        let first = state
            .store(FullscreenPayload {
                revision: 0,
                items: vec![item("a.jpg")],
                index: 0,
            })
            .expect("存得进");
        let second = state
            .store(FullscreenPayload {
                revision: 0,
                items: vec![item("b.jpg")],
                index: 0,
            })
            .expect("存得进");
        assert_eq!(first.revision, 1);
        assert_eq!(second.revision, 2);
        assert_eq!(state.read().expect("读得到").expect("有值").revision, 2);
    }

    #[test]
    fn payload_round_trips_through_json() {
        let payload = FullscreenPayload {
            revision: 7,
            items: vec![item("带 空格 的.jpg"), item("b.ORF")],
            index: 1,
        };
        let text = serde_json::to_string(&payload).expect("能序列化");
        let back: FullscreenPayload = serde_json::from_str(&text).expect("能反序列化");
        assert_eq!(back, payload, "中文与空格路径也要原样往返");
    }
}
