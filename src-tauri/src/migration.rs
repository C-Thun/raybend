//! 数据库升级 → 前端事件（`AGENTS.md` §6.4 的版本闸门与 §8 第 5 条）。
//!
//! **为什么要这条桥**：`store::migration` 只负责「升库」这一件事，它**不依赖 Tauri**
//! （分层纪律，`AGENTS.md` §4）；而「升级时把界面挡住、升级完再放开」是外壳的事。
//! 于是 store 发一条进程内通知（`set_progress_hook`），这里把它转成前端事件。
//!
//! 前端拿到 `running: true` 就弹**阻塞遮罩**（`src/features/migration/`），
//! 拿到 `running: false` 就撤 —— 迁移失败也会发 `false`（否则遮罩撤不掉，
//! 界面直接卡死；失败原因由那条命令的 `Result` 负责报给用户）。
//!
//! 两个库走的都是这条桥：
//!   * `app.db` —— 启动时（`db::warm_up`）或第一次用到时开；
//!   * `catalog.db` —— **只在真正调用时**才检查 + 升级（人类 2026-09-19 的要求），
//!     也就是 `browse::with_catalog` / `import` 里那几处 `CatalogDb::open`。

use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

use raybend::store::migration::{DbKind, MigrationNotice, MigrationPhase};

/// 事件名（前端 `listen` 的就是它）。
pub const MIGRATION_EVENT: &str = "db://migration";

/// 事件负载。字段与 `src/api/types.ts` 的 `MigrationNotice` 一一对应
/// （漂移由 `dto-contract.test.ts` + `contract.rs` 两侧对着 `dto-contract.json` 兜住）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationEvent {
    /// 库的种类：`app` / `catalog` / `thumbs`
    pub kind: &'static str,
    /// 中文名（提示文案用；界面不自己拼库种类）
    pub label: &'static str,
    /// 升级前的 schema 版本
    pub from: i64,
    /// 升级后的 schema 版本
    pub to: i64,
    /// `true` = 开始（弹遮罩）；`false` = 结束（撤遮罩）
    pub running: bool,
}

/// 库种类的稳定标识（前端按它选文案，**不要用 `label` 做判断**）。
const fn kind_id(kind: DbKind) -> &'static str {
    match kind {
        DbKind::App => "app",
        DbKind::Catalog => "catalog",
        DbKind::Thumbs => "thumbs",
    }
}

/// 通知 → 事件（纯函数，方便单测与契约测试）。
pub(crate) fn to_event(notice: MigrationNotice) -> MigrationEvent {
    MigrationEvent {
        kind: kind_id(notice.kind),
        label: notice.kind.label(),
        from: notice.from,
        to: notice.to,
        running: notice.phase == MigrationPhase::Start,
    }
}

/// 注册进度钩子（**只生效一次**；进程内只该有一个外壳）。
///
/// 必须在**第一次开库之前**调用 —— 否则 `app.db` 的迁移通知没人听。
/// 调用点是 `lib.rs` 的 `setup()`，位置在 `db::warm_up` 之上。
pub fn install<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    let installed = raybend::store::migration::set_progress_hook(Box::new(move |notice| {
        // 事件发不出去不是致命问题（前端可能还没挂监听），但不能静默
        if let Err(err) = handle.emit(MIGRATION_EVENT, to_event(notice)) {
            eprintln!("[raybend] 迁移事件发送失败：{err}");
        }
    }));
    if !installed {
        eprintln!("[raybend] 迁移通知钩子已经注册过 —— 忽略这一次（进程内只该有一个外壳）");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_and_done_map_to_running_flag() {
        let start = to_event(MigrationNotice {
            kind: DbKind::Catalog,
            from: 3,
            to: 4,
            phase: MigrationPhase::Start,
        });
        assert_eq!(
            (start.kind, start.label, start.from, start.to, start.running),
            ("catalog", "照片库", 3, 4, true)
        );

        let done = to_event(MigrationNotice {
            kind: DbKind::App,
            from: 3,
            to: 4,
            phase: MigrationPhase::Done,
        });
        assert_eq!((done.kind, done.running), ("app", false));
    }

    #[test]
    fn every_db_kind_has_its_own_id() {
        assert_eq!(kind_id(DbKind::App), "app");
        assert_eq!(kind_id(DbKind::Catalog), "catalog");
        assert_eq!(kind_id(DbKind::Thumbs), "thumbs");
    }
}
