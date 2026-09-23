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
    payload: Mutex<Option<FullscreenPayload>>,
}

impl FullscreenState {
    fn store(&self, payload: FullscreenPayload) -> Result<(), String> {
        let mut guard = self
            .payload
            .lock()
            .map_err(|_| "全屏看图状态锁中毒".to_string())?;
        *guard = Some(payload);
        Ok(())
    }

    fn read(&self) -> Result<Option<FullscreenPayload>, String> {
        let guard = self
            .payload
            .lock()
            .map_err(|_| "全屏看图状态锁中毒".to_string())?;
        Ok(guard.clone())
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
    let payload = FullscreenPayload { items, index };
    state.store(payload.clone())?;

    // 已经开着：通知页面换图（事件先发 —— 页面订阅着就立刻响应），再把它拿到前面
    let _ = app.emit(FULLSCREEN_EVENT, &payload);
    if let Some(window) = app.get_webview_window(FULLSCREEN_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    let monitor = main_window_monitor(&app);
    let window = WebviewWindowBuilder::new(
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
    .map_err(|error| format!("建全屏看图窗口失败：{error}"))?;

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
/// # Errors
/// 窗口存在但关闭失败（不存在算成功 —— 幂等）。
#[tauri::command]
pub async fn fullscreen_close<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(FULLSCREEN_LABEL) {
        window
            .close()
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
            items: vec![item("a.jpg")],
            index: 0,
        };
        let value = serde_json::to_value(&payload).expect("能序列化");
        let object = value.as_object().expect("是个对象");
        let mut keys: Vec<&String> = object.keys().collect();
        keys.sort();
        assert_eq!(keys, vec!["index", "items"]);

        let first = &value["items"][0];
        let mut item_keys: Vec<&String> = first.as_object().expect("是个对象").keys().collect();
        item_keys.sort();
        assert_eq!(item_keys, vec!["fileName", "id", "path"]);
    }

    #[test]
    fn state_keeps_the_last_payload_and_reads_it_back() {
        let state = FullscreenState::default();
        assert!(state.read().expect("读得到").is_none(), "一开始没有清单");

        state
            .store(FullscreenPayload {
                items: vec![item("a.jpg"), item("b.jpg")],
                index: 1,
            })
            .expect("存得进");
        let payload = state.read().expect("读得到").expect("有值");
        assert_eq!(payload.index, 1);
        assert_eq!(payload.items.len(), 2);
        assert_eq!(payload.items[0].file_name, "a.jpg");
    }

    #[test]
    fn payload_round_trips_through_json() {
        let payload = FullscreenPayload {
            items: vec![item("带 空格 的.jpg"), item("b.ORF")],
            index: 1,
        };
        let text = serde_json::to_string(&payload).expect("能序列化");
        let back: FullscreenPayload = serde_json::from_str(&text).expect("能反序列化");
        assert_eq!(back, payload, "中文与空格路径也要原样往返");
    }
}
