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
//!        │ 同时 emit "fullscreen://payload"   ← 已开着的话，页面收到就换图（不重建窗口）
//!        ▼
//!  全屏窗口（label = fullscreen-viewer，url = index.html?fullscreen=1）
//! ```
//!
//! ⚠️ **为什么清单走 Rust 而不是 URL**：一个目录几千张照片时 URL 装不下（也不该装）；
//! 而页面又要在「已经开着的窗口里换图」时拿到新清单 —— 所以状态放 Rust、事件通知页面。
//!
//! ⚠️ `current_monitor()` 属于「拖动期间可能阻塞」那一类窗口查询（`AGENTS.md` §7.9 的
//! 工程侧血泪第 4 条）。这里只在**用户点按钮的那一刻**问一次（不是渲染循环里反复问），
//! 而且那会儿手不可能同时在拖窗口 —— 所以可以接受。**别把它搬到每帧都跑的地方去。**

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, Runtime, State, WebviewUrl, WebviewWindowBuilder,
};

use crate::MAIN_WINDOW_LABEL;

/// 全屏看图窗口的标签（`src/index.tsx` 用 `?fullscreen=1` 那条查询串选页）。
pub const FULLSCREEN_LABEL: &str = "fullscreen-viewer";

/// 清单更新事件（页面已在运行时换图用）。
pub const FULLSCREEN_EVENT: &str = "fullscreen://payload";

/// 全屏页需要的一张照片（**只有页面真的用到的三个字段**）。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullscreenItem {
    pub id: String,
    /// 绝对路径（`view_image` 直接吃它）
    pub path: String,
    pub file_name: String,
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

/// 打开全屏看图（幂等：已经开着就换图 + 聚焦，不重建窗口）。
///
/// # Errors
/// 清单 / 下标非法；或窗口建不起来。
#[tauri::command]
pub async fn fullscreen_open<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, FullscreenState>,
    items: Vec<FullscreenItem>,
    index: usize,
) -> Result<(), String> {
    validate(&items, index)?;
    // 存一份**带新版本号**的清单（页面据此丢弃迟到的旧包）
    let payload = state.store(FullscreenPayload {
        revision: 0,
        items,
        index,
    })?;

    // 已经开着：通知页面换图（事件先发 —— 页面订阅着就立刻响应），再把它拿到前面
    let _ = app.emit(FULLSCREEN_EVENT, &payload);
    if let Some(window) = app.get_webview_window(FULLSCREEN_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    let monitor = main_window_monitor(&app);
    let window = match WebviewWindowBuilder::new(
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
    .visible(false)
    .build()
    {
        Ok(window) => window,
        Err(error) => {
            /*
             * 两个现实场景会走到这里，都**不该**报错给用户：
             *   * **连点两下按钮**：第一下正在建窗，第二下也到这里 —— 标签已被占；
             *   * 上一扇刚被 `destroy`，标签尚未释放。
             * 这时若窗口已在就退化成「复用 + 换图」（清单与事件在上面已经发过，
             * 新页挂载时也会自己读一次），确实不在才把真错报出去。
             */
            if let Some(window) = app.get_webview_window(FULLSCREEN_LABEL) {
                let _ = window.show();
                let _ = window.set_focus();
                return Ok(());
            }
            return Err(format!("建全屏看图窗口失败：{error}"));
        }
    };

    /*
     * 先摆到主窗口那块屏幕上，再全屏 —— 顺序不能反：
     * `set_fullscreen(true)` 是「在窗口当前所在的屏幕上全屏」，先全屏再挪会先闪一下主屏。
     * 坐标系是**物理像素**（monitor.position/size 就是物理值，别再乘 scale）。
     */
    if let Some(monitor) = monitor {
        let position = monitor.position();
        let size = monitor.size();
        let _ = window.set_position(tauri::PhysicalPosition::new(position.x, position.y));
        let _ = window.set_size(tauri::PhysicalSize::new(size.width, size.height));
    }
    let _ = window.set_fullscreen(true);
    let _ = window.show();
    let _ = window.set_focus();
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
/// 用 [`tauri::WebviewWindow::destroy`] 而不是 `close`：`close` 走「请求关闭」，
/// 窗口从标签表里消失是**异步**的 —— 而「按 `Esc` 后马上回主窗口再点全屏」是真实动作，
/// 那一刻 `get_webview_window` 可能还看得到正在死掉的窗口，于是 `fullscreen_open`
/// 走「复用」分支、什么也不发生（要点第二下）。`destroy` 立即销毁，标签当场释放。
///
/// # Errors
/// 窗口存在但销毁失败（不存在算成功 —— 幂等）。
#[tauri::command]
pub async fn fullscreen_close<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(FULLSCREEN_LABEL) {
        window
            .destroy()
            .map_err(|error| format!("关全屏看图窗口失败：{error}"))?;
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
        }
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
