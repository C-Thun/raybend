//! 缩略图命令：**导入工作区里还没入库的源文件**也要能出图（`plans/M1-5.md` §3.1）。
//!
//! # 为什么单独一个缓存库
//!
//! M1-3 的缓存是「每库一个 `thumbs.db`」，而源目录还不属于任何库。这里的做法是
//! 给源文件留一个固定的小库：`<app data>/cache/_sources/thumbs.db`。
//! 键仍然优先用**文件身份**（改名/移动后仍命中），读不到身份才退回**绝对路径** ——
//! 所以它将来跟库内的缓存不会互相打架（同一个文件在两处的键是一样的）。
//!
//! # 为什么返回裸字节
//!
//! `tauri::ipc::Response` 走的是**原始字节**通道，不经过 JSON/base64。
//! 一张 384px 的缩略图约 30–60KB，base64 会白白多三分之一的传输量；
//! 前端拿到 `ArrayBuffer` 后转 `Blob` → `URL.createObjectURL`，交给浏览器自己缓存。
//!
//! # 并发
//!
//! 渲染一张要几十到两百毫秒。**限流放在前端**（同时最多 4 个在飞），
//! 这里不再自己排队 —— 免得两处同时限流、互相等。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use raybend::store::time;
use raybend::thumbnail::{SizeClass, ThumbsDb, render_now};
use tauri::{AppHandle, Manager, Runtime};

/// 源文件缩略图缓存所在的子目录名。
///
/// 前面加下划线是**刻意**的：库 ID 是 base62（不会以下划线开头），
/// 所以这个名字永远不可能与某个真实库的缓存目录撞上。
pub const SOURCES_CACHE_DIR: &str = "_sources";

/// 源文件缩略图缓存库的持有者（延迟打开，全程共用一份）。
///
/// 用一个 `Arc<ThumbsDb>` 而不是直接存 `ThumbsDb`：**取出引用就放开锁**，
/// 否则所有缩略图请求会被串行化在一把锁上（渲染本来就慢，再串起来就没法滚动了）。
#[derive(Default)]
pub struct SourcesThumbs {
    inner: Mutex<Option<Arc<ThumbsDb>>>,
}

impl SourcesThumbs {
    /// 取（必要时打开）缓存库。
    fn get(&self, dir: &Path, now_ms: i64) -> Result<Arc<ThumbsDb>, String> {
        let mut guard = self.inner.lock().map_err(|_| "内部锁已损坏".to_string())?;
        if let Some(db) = guard.as_ref() {
            return Ok(Arc::clone(db));
        }
        let db = Arc::new(ThumbsDb::open(dir, now_ms).map_err(|e| e.to_string())?);
        *guard = Some(Arc::clone(&db));
        Ok(db)
    }
}

/// 源文件缩略图的存放目录（`<app data>/cache/_sources/`）。
fn sources_cache_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(crate::db::data_dir(app)?.join("cache").join(SOURCES_CACHE_DIR))
}

/// 取一张缩略图的字节。
///
/// `size`：`"grid"`（网格，长边 384）或 `"strip"`（胶片带，长边 192）；缺省 `grid`。
#[tauri::command]
pub async fn thumb_get<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, SourcesThumbs>,
    path: String,
    size: Option<String>,
) -> Result<tauri::ipc::Response, String> {
    // 缓存库要先拿出来（`State` 不能跨 await 持有）
    let cache_dir = sources_cache_dir(&app)?;
    let db = {
        let now = time::now_millis();
        state.get(&cache_dir, now)?
    };
    let size = SizeClass::parse(size.as_deref().unwrap_or("grid"))
        .ok_or_else(|| format!("未知的缩略图尺度：{}", size.as_deref().unwrap_or("")))?;

    let bytes = crate::source::blocking(move || {
        let bytes = render_now(&db, Path::new(&path), size, time::now_millis())
            .map_err(|e| e.to_string())?;
        Ok(bytes)
    })
    .await?;

    Ok(tauri::ipc::Response::new(bytes))
}

/// 源文件缩略图缓存的统计（设置面板与排错用）。
#[tauri::command]
pub async fn thumb_sources_stats<R: Runtime>(
    app: AppHandle<R>,
) -> Result<raybend::thumbnail::CacheStats, String> {
    let cache_dir = sources_cache_dir(&app)?;
    let state = app.state::<SourcesThumbs>();
    let db = state.get(&cache_dir, time::now_millis())?;
    db.read(raybend::thumbnail::cache::stats)
        .map_err(|e| e.to_string())
}
