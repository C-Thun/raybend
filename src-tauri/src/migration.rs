//! 数据库升级的唯一活动快照。事件只是通知，晚订阅可查询状态补齐。
//! SQL 迁移仍由核心执行；外壳不碰连接、不承载图片生成。
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use raybend::store::migration::{DbKind, MigrationNotice, MigrationPhase};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

pub const MIGRATION_EVENT: &str = "db://migration";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationEvent {
    /// u64 以十进制字符串跨 IPC，避免 JS 精度损失。
    pub id: String,
    pub kind: &'static str,
    pub label: &'static str,
    pub from: i64,
    pub to: i64,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationSnapshot {
    pub revision: String,
    pub active: Vec<MigrationEvent>,
}

#[derive(Default)]
struct Inner {
    revision: u64,
    active: BTreeMap<u64, MigrationEvent>,
}
impl Inner {
    fn snapshot(&self) -> MigrationSnapshot {
        MigrationSnapshot {
            revision: self.revision.to_string(),
            active: self.active.values().cloned().collect(),
        }
    }
}

#[derive(Clone, Default)]
pub struct MigrationState {
    shared: Arc<Mutex<Inner>>,
}
impl MigrationState {
    fn apply(&self, notice: MigrationNotice) -> Result<MigrationSnapshot, String> {
        let mut inner = self.shared.lock().map_err(|_| "升级状态锁已损坏")?;
        let revision = inner
            .revision
            .checked_add(1)
            .ok_or("升级状态版本已耗尽，请重启应用")?;
        if notice.phase == MigrationPhase::Start {
            inner.active.insert(notice.id, to_event(notice));
        } else {
            inner.active.remove(&notice.id);
        }
        inner.revision = revision;
        Ok(inner.snapshot())
    }

    pub(crate) fn snapshot(&self) -> Result<MigrationSnapshot, String> {
        Ok(self
            .shared
            .lock()
            .map_err(|_| "升级状态锁已损坏")?
            .snapshot())
    }

    pub(crate) fn is_running(&self) -> Result<bool, String> {
        Ok(!self
            .shared
            .lock()
            .map_err(|_| "升级状态锁已损坏")?
            .active
            .is_empty())
    }
}

const fn kind_id(kind: DbKind) -> &'static str {
    match kind {
        DbKind::App => "app",
        DbKind::Catalog => "catalog",
        DbKind::Thumbs => "thumbs",
    }
}

pub(crate) fn to_event(notice: MigrationNotice) -> MigrationEvent {
    MigrationEvent {
        id: notice.id.to_string(),
        kind: kind_id(notice.kind),
        label: notice.kind.label(),
        from: notice.from,
        to: notice.to,
        running: notice.phase == MigrationPhase::Start,
    }
}

/// 只读内存，app.db 仍在打开时也能调用。
#[tauri::command]
pub fn migration_snapshot(state: State<'_, MigrationState>) -> Result<MigrationSnapshot, String> {
    state.snapshot()
}

/// 第一次开库之前安装。持久状态先更新，emit 在释放锁后进行。
pub fn install<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    let registry = app.state::<MigrationState>().inner().clone();
    let installed = raybend::store::migration::set_progress_hook(Box::new(move |notice| {
        match registry.apply(notice) {
            Ok(snapshot) => {
                if let Err(err) = handle.emit(MIGRATION_EVENT, snapshot) {
                    eprintln!("[raybend] 迁移事件发送失败：{err}");
                }
            }
            Err(err) => eprintln!("[raybend] 迁移状态更新失败：{err}"),
        }
    }));
    if !installed {
        eprintln!("[raybend] 迁移通知钩子已经注册过，忽略重复安装");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(id: u64, phase: MigrationPhase) -> MigrationNotice {
        MigrationNotice {
            id,
            kind: DbKind::Catalog,
            from: 3,
            to: 4,
            phase,
        }
    }

    #[test]
    fn start_and_done_map_to_running_flag() {
        let start = to_event(notice(1, MigrationPhase::Start));
        assert_eq!(
            (start.id.as_str(), start.kind, start.label, start.running),
            ("1", "catalog", "照片库", true)
        );
        assert!(!to_event(notice(1, MigrationPhase::Done)).running);
        assert_eq!(kind_id(DbKind::App), "app");
        assert_eq!(kind_id(DbKind::Thumbs), "thumbs");
    }

    #[test]
    fn late_listener_recovers_both_catalogs_and_done_only_removes_its_own() {
        let state = MigrationState::default();
        assert_eq!(state.snapshot().unwrap().revision, "0");
        state.apply(notice(1, MigrationPhase::Start)).unwrap();
        state.apply(notice(2, MigrationPhase::Start)).unwrap();
        let snapshot = state.snapshot().unwrap();
        assert_eq!(snapshot.revision, "2");
        assert_eq!(snapshot.active.len(), 2);
        assert!(state.is_running().unwrap());
        // 重复 Start 幂等；未知 Done 不会清走另一个 catalog。
        state.apply(notice(2, MigrationPhase::Start)).unwrap();
        state.apply(notice(99, MigrationPhase::Done)).unwrap();
        let remaining = state.apply(notice(1, MigrationPhase::Done)).unwrap();
        assert_eq!(remaining.active.len(), 1);
        assert_eq!(remaining.active[0].id, "2");
        state.apply(notice(2, MigrationPhase::Done)).unwrap();
        assert!(!state.is_running().unwrap());
    }

    #[test]
    fn concurrent_updates_are_serialized_and_large_numbers_remain_exact() {
        let state = MigrationState::default();
        std::thread::scope(|scope| {
            for id in 1..=8 {
                let state = &state;
                scope.spawn(move || {
                    state.apply(notice(id, MigrationPhase::Start)).unwrap();
                    state.apply(notice(id, MigrationPhase::Done)).unwrap();
                });
            }
        });
        let snapshot = state.snapshot().unwrap();
        assert_eq!(snapshot.revision, "16");
        assert!(snapshot.active.is_empty());
        let large = state
            .apply(notice(u64::MAX, MigrationPhase::Start))
            .unwrap();
        assert_eq!(large.active[0].id, u64::MAX.to_string());
        state.shared.lock().unwrap().revision = u64::MAX;
        assert!(state.apply(notice(u64::MAX, MigrationPhase::Done)).is_err());
        assert!(state.is_running().unwrap(), "溢出不能静默清掉等待状态");
    }
}
