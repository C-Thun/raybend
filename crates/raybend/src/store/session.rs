//! 库连接生命周期。全局登记锁只查句柄，探测/退场在每库锁内执行。
//! 连接代次与前端请求排序无关；租约固定位置，作废后不能借新库继续任务。
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};

use super::db::{CatalogDb, OpenOpts};
use super::file_id::FileId;
use super::repository;
use crate::{Error, Result};

pub(crate) struct SessionGuard {
    valid: AtomicBool,
    failure: Mutex<Option<super::availability::ConnectionStatus>>,
    path: PathBuf,
    repository_id: String,
    identity: Option<FileId>,
    generation: u64,
}

impl SessionGuard {
    pub(crate) fn new(path: &Path, repository_id: &str, generation: u64) -> Self {
        Self {
            valid: AtomicBool::new(true),
            failure: Mutex::new(None),
            path: path.into(),
            repository_id: repository_id.into(),
            identity: FileId::try_read(path).filter(|id| !id.is_zero()),
            generation,
        }
    }
    pub(crate) fn invalidate(&self) {
        self.valid.store(false, Ordering::Release);
    }
    pub(crate) fn check_active(&self) -> Result<()> {
        if self.valid.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(Error::SessionExpired)
        }
    }
    pub(crate) fn failure(&self) -> Option<super::availability::ConnectionStatus> {
        self.failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
    fn fail(&self, error: &Error) {
        let mut failure = self
            .failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if failure.is_none() {
            let mut status =
                super::availability::ConnectionStatus::failed(&self.repository_id, error);
            status.generation = self.generation.to_string();
            *failure = Some(status);
        }
        self.invalidate();
    }
    pub(crate) fn check_connection(&self, conn: &rusqlite::Connection) -> Result<()> {
        let result = repository::RepositoryMeta::read(conn, &self.path).and_then(|meta| {
            if meta.id == self.repository_id {
                Ok(())
            } else {
                Err(Error::RepositoryIdentityChanged(self.path.clone()))
            }
        });
        if let Err(error) = &result {
            self.fail(error);
        }
        result
    }
    pub(crate) fn check(&self) -> Result<()> {
        self.check_active()?;
        let result = (|| {
            if !self.path.try_exists()? {
                return Err(Error::PathNotFound(self.path.clone()));
            }
            if let Some(expected) = self.identity
                && FileId::try_read(&self.path) != Some(expected)
            {
                return Err(Error::RepositoryIdentityChanged(self.path.clone()));
            }
            let meta = repository::read_repository_meta(&self.path)?;
            if meta.id != self.repository_id {
                return Err(Error::RepositoryIdentityChanged(self.path.clone()));
            }
            Ok(())
        })();
        if let Err(error) = &result {
            self.fail(error);
        }
        result
    }
    pub(crate) fn observe<T>(&self, result: Result<T>) -> Result<T> {
        if let Err(error) = &result
            && invalidates_session(error)
        {
            // SQLite 句柄的 I/O 错误可能来自拔盘；核对位置的当前事实，
            // 不把已不存在的位置显示成 catalog 损坏。
            match self.path.try_exists() {
                Ok(false) => self.fail(&Error::PathNotFound(self.path.clone())),
                Err(io) => self.fail(&Error::Io(io)),
                _ => self.fail(error),
            }
        }
        result
    }
    pub(crate) fn physical_identity(&self) -> Option<FileId> {
        self.identity
    }
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
}

/// 只把连接/设备故障当库故障；非法输入、约束冲突、照片解码错误不降级整库。
pub fn invalidates_session(error: &Error) -> bool {
    match error {
        Error::SessionExpired
        | Error::RepositoryIdentityChanged(_)
        | Error::PathNotFound(_)
        | Error::NotARepository { .. }
        | Error::WriterGone => true,
        Error::Database(rusqlite::Error::SqliteFailure(code, _)) => matches!(
            code.code,
            rusqlite::ErrorCode::SystemIoFailure
                | rusqlite::ErrorCode::DatabaseCorrupt
                | rusqlite::ErrorCode::NotADatabase
                | rusqlite::ErrorCode::CannotOpen
                | rusqlite::ErrorCode::ReadOnly
                | rusqlite::ErrorCode::DiskFull
        ),
        _ => false,
    }
}

/// 一个长任务持有的逻辑库租约。恢复只接纳原 catalog 实体的新健康会话。
/// 各业务共享它，写入不会静默转到同 ID 副本。
#[derive(Debug)]
pub struct TaskCatalog {
    current: Mutex<Arc<CatalogDb>>,
    repository_id: String,
    identity: Option<FileId>,
}
impl TaskCatalog {
    pub fn new(db: Arc<CatalogDb>) -> Self {
        Self {
            repository_id: db.meta().id.clone(),
            identity: db.physical_identity(),
            current: Mutex::new(db),
        }
    }
    pub fn current(&self) -> Arc<CatalogDb> {
        Arc::clone(
            &self
                .current
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
    pub fn install(&self, next: Arc<CatalogDb>) -> Result<()> {
        next.ensure_current()?;
        if next.meta().id != self.repository_id
            || self.identity.is_none()
            || next.physical_identity() != self.identity
        {
            return Err(Error::RepositoryIdentityChanged(next.path().into()));
        }
        *self
            .current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = next;
        Ok(())
    }
}

/// 来源根的设备/目录身份；无可靠 ID 时首次失联后禁止自动认路径续跑。
pub struct SourceIdentity {
    root: PathBuf,
    identity: Option<FileId>,
    lost: AtomicBool,
}
impl SourceIdentity {
    pub fn capture(root: &Path) -> Self {
        Self {
            root: root.into(),
            identity: FileId::try_read(root).filter(|id| !id.is_zero()),
            lost: AtomicBool::new(!root.is_dir()),
        }
    }
    /// 尚不存在的输出子目录固定其最近已存在的父目录身份。
    pub fn capture_output(path: &Path) -> Self {
        let mut anchor = path;
        while !anchor.is_dir() {
            match anchor.parent() {
                Some(parent) if parent != anchor => anchor = parent,
                _ => break,
            }
        }
        Self::capture(anchor)
    }
    pub fn ready(&self) -> bool {
        if !self.root.is_dir() {
            self.lost.store(true, Ordering::Release);
            return false;
        }
        match self.identity {
            Some(expected) => FileId::try_read(&self.root) == Some(expected),
            None => !self.lost.load(Ordering::Acquire),
        }
    }
}

#[derive(Default)]
struct Slot {
    current: Mutex<Weak<CatalogDb>>,
    lifecycle: AtomicU8, // 0 active, 1 releasing, 2 released
    tasks: Mutex<usize>,
    drained: Condvar,
}
pub struct TaskPermit {
    slot: Arc<Slot>,
}
impl std::fmt::Debug for TaskPermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TaskPermit")
    }
}
impl PartialEq for TaskPermit {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.slot, &other.slot)
    }
}
impl Eq for TaskPermit {}
#[derive(Clone)]
pub struct TaskAccess {
    slot: Arc<Slot>,
}
impl TaskPermit {
    pub fn access(&self) -> TaskAccess {
        TaskAccess {
            slot: Arc::clone(&self.slot),
        }
    }
}
impl TaskAccess {
    pub fn enter(&self) -> Result<TaskPermit> {
        let slot = Arc::clone(&self.slot);
        let current = slot
            .current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut tasks = slot
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.lifecycle.load(Ordering::Acquire) != 0 {
            return Err(Error::RepositoryReleased);
        }
        if let Some(db) = current.upgrade() {
            db.ensure_alive()?;
        }
        *tasks = tasks.checked_add(1).ok_or(Error::RepositoryBusy)?;
        drop(tasks);
        drop(current);
        Ok(TaskPermit { slot })
    }
}
impl Drop for TaskPermit {
    fn drop(&mut self) {
        let mut tasks = self
            .slot
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *tasks -= 1;
        self.slot.drained.notify_all();
    }
}

/// 弱登记协调所有持有者；最多保留 4 个空闲会话，长期任务自己持有租约。
/// 弱登记在下一次获取时清理，关写者及读池始终在全局锁外。
#[derive(Default)]
pub struct CatalogSessions {
    slots: Mutex<HashMap<String, Arc<Slot>>>,
    idle: Mutex<Vec<Arc<CatalogDb>>>,
    generation: AtomicU64,
}

impl CatalogSessions {
    fn slot(&self, id: &str) -> Arc<Slot> {
        let mut slots = self
            .slots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        slots.retain(|_, slot| {
            slot.lifecycle.load(Ordering::Acquire) != 0
                || Arc::strong_count(slot) > 1
                || slot
                    .current
                    .try_lock()
                    .map_or(true, |v| v.strong_count() > 0)
        });
        Arc::clone(slots.entry(id.into()).or_default())
    }
    pub fn begin_task(&self, id: &str) -> Result<TaskPermit> {
        TaskAccess {
            slot: self.slot(id),
        }
        .enter()
    }
    pub fn has_tasks(&self, id: &str) -> bool {
        *self
            .slot(id)
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            > 0
    }
    pub fn is_released(&self, id: &str) -> bool {
        self.slot(id).lifecycle.load(Ordering::Acquire) != 0
    }
    /// 显式位置切换与任务登记互斥，旧连接退出后才持久化用户的选择。
    pub fn switch_location<T>(&self, id: &str, select: impl FnOnce() -> Result<T>) -> Result<T> {
        let slot = self.slot(id);
        let mut current = slot
            .current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.lifecycle.load(Ordering::Acquire) == 1 || self.has_tasks(id) {
            return Err(Error::RepositoryBusy);
        }
        if let Some(db) = current.upgrade() {
            db.retire()?;
        }
        *current = Weak::new();
        select()
    }
    pub fn begin_release(&self, id: &str) -> bool {
        let slot = self.slot(id);
        if slot
            .lifecycle
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        if let Some(db) = slot
            .current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .upgrade()
        {
            db.invalidate();
        }
        true
    }
    pub fn finish_release(&self, id: &str) -> Result<()> {
        let slot = self.slot(id);
        let mut tasks = slot
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while *tasks > 0 {
            tasks = slot
                .drained
                .wait(tasks)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        drop(tasks);
        self.invalidate(id)?;
        slot.lifecycle.store(2, Ordering::Release);
        Ok(())
    }
    pub fn resume(&self, id: &str) -> Result<()> {
        let slot = self.slot(id);
        match slot
            .lifecycle
            .compare_exchange(2, 0, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => Ok(()),
            Err(0) => Ok(()),
            _ => Err(Error::RepositoryBusy),
        }
    }
    pub fn acquire(
        &self,
        id: &str,
        resolve: impl FnOnce() -> Result<PathBuf>,
        opts: OpenOpts<'_>,
    ) -> Result<Arc<CatalogDb>> {
        self.acquire_with(id, resolve, opts, CatalogDb::open_session)
    }

    /// 位置登记变更与同库 acquire/退场互斥；回调仅可做短本地登记事务。
    pub fn with_current<T>(
        &self,
        id: &str,
        f: impl FnOnce(Option<&CatalogDb>) -> Result<T>,
    ) -> Result<T> {
        let slot = self.slot(id);
        let current = slot
            .current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let db = current.upgrade();
        f(db.as_deref())
    }
    // 单一生产构造器；注入点用于重现定位与实际打开之间的换库/打开失败。
    fn acquire_with<'a>(
        &self,
        id: &str,
        resolve: impl FnOnce() -> Result<PathBuf>,
        opts: OpenOpts<'a>,
        open: impl FnOnce(&Path, &str, u64, OpenOpts<'a>) -> Result<CatalogDb>,
    ) -> Result<Arc<CatalogDb>> {
        let slot = self.slot(id);
        let mut current = slot
            .current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.lifecycle.load(Ordering::Acquire) != 0 {
            return Err(Error::RepositoryReleased);
        }
        if let Some(db) = current.upgrade() {
            match db.ensure_current() {
                Ok(()) => return Ok(db),
                Err(error) => {
                    db.retire()?;
                    *current = Weak::new();
                    return Err(error);
                }
            }
        }
        let root = resolve()?;
        let generation = self
            .generation
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| Error::Unsupported("连接代次已耗尽，请重启应用".into()))?
            + 1;
        let db = Arc::new(open(&root, id, generation, opts)?);
        *current = Arc::downgrade(&db);
        let displaced = {
            let mut idle = self
                .idle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            idle.push(Arc::clone(&db));
            if idle.len() > 4 {
                Some(idle.remove(0))
            } else {
                None
            }
        };
        drop(current);
        drop(displaced);
        Ok(db)
    }
    /// 先作废，再排干拒绝旧任务；同库新写者必须等本次 join 完成。
    pub fn invalidate(&self, id: &str) -> Result<()> {
        self.invalidate_matching(id, None)
    }
    /// 旧状态发起的重连不能关闭并发操作刚建立的新代次。
    pub fn invalidate_observed(&self, id: &str, generation: u64) -> Result<()> {
        self.invalidate_matching(id, Some(generation))
    }
    fn invalidate_matching(&self, id: &str, generation: Option<u64>) -> Result<()> {
        let slot = self.slot(id);
        let mut current = slot
            .current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(db) = current.upgrade() {
            if generation.is_some_and(|expected| db.generation() != expected) {
                return Ok(());
            }
            db.retire()?;
        }
        *current = Weak::new();
        let removed = {
            let mut idle = self
                .idle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut removed = Vec::new();
            let mut i = 0;
            while i < idle.len() {
                if idle[i].meta().id == id {
                    removed.push(idle.remove(i));
                } else {
                    i += 1;
                }
            }
            removed
        };
        drop(current);
        drop(removed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn release_drains_tasks_rejects_new_work_and_requires_explicit_resume() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("库");
        let id = catalog(&root);
        let sessions = Arc::new(CatalogSessions::default());
        let db = sessions
            .acquire(&id, || Ok(root.clone()), OpenOpts::new(None, 0))
            .unwrap();
        let task = sessions.begin_task(&id).unwrap();
        let access = task.access();
        assert!(sessions.begin_release(&id));
        assert!(!sessions.begin_release(&id));
        assert!(matches!(access.enter(), Err(Error::RepositoryReleased)));
        assert!(matches!(
            sessions.begin_task(&id),
            Err(Error::RepositoryReleased)
        ));
        assert!(matches!(
            sessions.acquire(&id, || panic!("不应探测"), OpenOpts::new(None, 0)),
            Err(Error::RepositoryReleased)
        ));
        assert!(matches!(sessions.resume(&id), Err(Error::RepositoryBusy)));
        let (done, rx) = mpsc::channel();
        let worker = sessions.clone();
        let name = id.clone();
        let thread = std::thread::spawn(move || {
            worker.finish_release(&name).unwrap();
            done.send(()).unwrap();
        });
        assert!(rx.recv_timeout(Duration::from_millis(20)).is_err());
        let other = sessions.begin_task("另一库").unwrap();
        drop(other);
        drop(task);
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        thread.join().unwrap();
        assert!(db.ensure_current().is_err());
        assert!(root.join("catalog.db").exists());
        assert!(sessions.is_released(&id));
        assert!(sessions.begin_task(&id).is_err());
        sessions.resume(&id).unwrap();
        let next = sessions
            .acquire(&id, || Ok(root.clone()), OpenOpts::new(None, 0))
            .unwrap();
        assert!(next.generation() > db.generation());
    }
    #[test]
    fn explicit_switch_with_active_task_keeps_current_session_and_choice_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("库");
        let id = catalog(&root);
        let sessions = CatalogSessions::default();
        let db = sessions
            .acquire(&id, || Ok(root.clone()), OpenOpts::new(None, 0))
            .unwrap();
        let permit = sessions.begin_task(&id).unwrap();
        assert!(matches!(
            sessions.switch_location::<()>(&id, || panic!("不应修改选择")),
            Err(Error::RepositoryBusy)
        ));
        assert!(db.ensure_current().is_ok());
        drop(permit);
        sessions.switch_location(&id, || Ok(())).unwrap();
        assert!(db.ensure_current().is_err());
    }
    #[test]
    fn task_rebind_requires_original_catalog_entity_even_for_matching_repository_id() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("原库");
        let id = catalog(&root);
        let sessions = CatalogSessions::default();
        let old = sessions
            .acquire(&id, || Ok(root.clone()), OpenOpts::new(None, 0))
            .unwrap();
        let task = TaskCatalog::new(old.clone());
        sessions.invalidate(&id).unwrap();
        let copy = dir.path().join("副本");
        std::fs::create_dir_all(&copy).unwrap();
        std::fs::copy(root.join("catalog.db"), copy.join("catalog.db")).unwrap();
        let next = Arc::new(CatalogDb::open_expected(&copy, &id, OpenOpts::new(None, 0)).unwrap());
        assert!(matches!(
            task.install(next),
            Err(Error::RepositoryIdentityChanged(_))
        ));
        task.install(
            sessions
                .acquire(&id, || Ok(root.clone()), OpenOpts::new(None, 0))
                .unwrap(),
        )
        .unwrap();
        assert!(task.current().ensure_current().is_ok());
    }
    #[test]
    fn source_identity_rejects_replacement_and_allows_original_return() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("中文源");
        std::fs::create_dir(&root).unwrap();
        let identity = SourceIdentity::capture(&root);
        assert!(identity.ready());
        let original = dir.path().join("detached");
        std::fs::rename(&root, &original).unwrap();
        assert!(!identity.ready());
        std::fs::create_dir(&root).unwrap();
        assert!(!identity.ready());
        std::fs::remove_dir(&root).unwrap();
        std::fs::rename(&original, &root).unwrap();
        assert!(identity.ready());
        let absent = SourceIdentity::capture(&dir.path().join("absent"));
        std::fs::create_dir(dir.path().join("absent")).unwrap();
        assert!(!absent.ready());
    }
    fn catalog(root: &Path) -> String {
        let db = CatalogDb::create(root, "照片库", None, OpenOpts::new(None, 0)).unwrap();
        db.meta().id.clone()
    }
    #[test]
    fn position_transaction_holds_current_session_until_registration_finishes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("库位置");
        let id = catalog(&root);
        let sessions = Arc::new(CatalogSessions::default());
        let db = sessions
            .acquire(&id, || Ok(root.clone()), OpenOpts::new(None, 0))
            .unwrap();
        let (started, start_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let held_sessions = Arc::clone(&sessions);
        let held_id = id.clone();
        let held_root = root.clone();
        let registration = std::thread::spawn(move || {
            held_sessions.with_current(&held_id, |current| {
                assert_eq!(current.unwrap().root(), held_root);
                started.send(()).unwrap();
                release_rx.recv().unwrap();
                assert!(current.unwrap().ensure_alive().is_ok());
                Ok(())
            })
        });
        start_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let (done, done_rx) = mpsc::channel();
        let retiring_sessions = Arc::clone(&sessions);
        let retiring_id = id.clone();
        let retiring = std::thread::spawn(move || {
            retiring_sessions.invalidate(&retiring_id).unwrap();
            done.send(()).unwrap();
        });
        assert!(matches!(
            done_rx.recv_timeout(Duration::from_millis(20)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        assert!(db.ensure_alive().is_ok());
        sessions
            .with_current("另一库", |current| {
                assert!(current.is_none());
                Ok(())
            })
            .unwrap();
        release.send(()).unwrap();
        registration.join().unwrap().unwrap();
        done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        retiring.join().unwrap();
        assert!(db.ensure_alive().is_err());
        sessions
            .with_current(&id, |current| {
                assert!(current.is_none());
                Ok(())
            })
            .unwrap();
    }
    #[test]
    fn stale_reconnect_does_not_retire_new_generation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("库");
        let id = catalog(&root);
        let sessions = CatalogSessions::default();
        let get = || {
            sessions
                .acquire(&id, || Ok(root.clone()), OpenOpts::new(None, 0))
                .unwrap()
        };
        let old = get();
        sessions.invalidate_observed(&id, old.generation()).unwrap();
        let new = get();
        sessions.invalidate_observed(&id, old.generation()).unwrap();
        assert!(Arc::ptr_eq(&new, &get()));
        new.set_import_template(":FILENAME").unwrap();
    }
    #[test]
    fn sqlite_io_fault_at_missing_location_is_offline_not_catalog_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.db");
        let io_fault = || {
            Error::Database(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_IOERR),
                None,
            ))
        };
        let missing = SessionGuard::new(&path, "库", 3);
        assert!(missing.observe::<()>(Err(io_fault())).is_err());
        let failure = missing.failure().unwrap();
        assert_eq!(
            failure.state,
            super::super::availability::Availability::Offline
        );
        assert_eq!(failure.reason, None);
        assert_eq!(failure.generation, "3");
        std::fs::write(&path, b"invalid catalog").unwrap();
        let present = SessionGuard::new(&path, "库", 4);
        assert!(present.observe::<()>(Err(io_fault())).is_err());
        assert_eq!(
            present.failure().unwrap().reason,
            Some(super::super::availability::Reason::IoFailure)
        );
        assert!(matches!(present.check_active(), Err(Error::SessionExpired)));
    }
    #[test]
    fn shared_writer_and_same_path_recovery_with_real_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("中文库");
        let id = catalog(&root);
        let sessions = CatalogSessions::default();
        let get = || {
            sessions
                .acquire(&id, || Ok(root.clone()), OpenOpts::new(None, 1))
                .unwrap()
        };
        let old = get();
        assert!(Arc::ptr_eq(&old, &get()));
        old.set_import_template(":FILENAME").unwrap();
        let generation = old.generation();
        sessions.invalidate(&id).unwrap();
        assert!(matches!(
            old.set_import_template("旧任务"),
            Err(Error::SessionExpired)
        ));
        let new = get();
        assert!(new.generation() > generation);
        assert!(!Arc::ptr_eq(&old, &new));
        assert_eq!(new.meta().import_template, ":FILENAME");
        new.set_import_template(":CYEAR/:FILENAME").unwrap();
        assert_eq!(
            repository::read_repository_meta(new.path())
                .unwrap()
                .import_template,
            ":CYEAR/:FILENAME"
        );
    }
    #[test]
    fn missing_location_invalidates_old_lease_and_never_creates_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("库");
        let id = catalog(&root);
        let sessions = CatalogSessions::default();
        let db = sessions
            .acquire(&id, || Ok(root.clone()), OpenOpts::new(None, 0))
            .unwrap();
        std::fs::rename(&root, dir.path().join("unplugged")).unwrap();
        assert!(db.read(|_| Ok(())).is_err());
        assert!(
            sessions
                .acquire(&id, || Ok(root.clone()), OpenOpts::new(None, 0))
                .is_err()
        );
        assert!(!root.exists());
        std::fs::rename(dir.path().join("unplugged"), &root).unwrap();
        let new = sessions
            .acquire(&id, || Ok(root.clone()), OpenOpts::new(None, 0))
            .unwrap();
        assert_ne!(db.generation(), new.generation());
        new.set_import_template(":FILENAME").unwrap();
    }
    #[test]
    fn wrong_catalog_is_rejected_before_migration() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        let id = catalog(&a);
        let other = catalog(&b);
        assert!(matches!(
            CatalogDb::open_expected(&b, &id, OpenOpts::new(None, 0)),
            Err(Error::RepositoryIdentityChanged(_))
        ));
        assert_eq!(
            repository::read_repository_meta(&b.join("catalog.db"))
                .unwrap()
                .id,
            other
        );
    }
    #[test]
    fn retire_waits_running_transaction_but_does_not_lock_other_repository() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        let id = catalog(&a);
        let other = catalog(&b);
        let sessions = Arc::new(CatalogSessions::default());
        let db = sessions
            .acquire(&id, || Ok(a.clone()), OpenOpts::new(None, 0))
            .unwrap();
        let (started, start_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let writer = Arc::clone(&db);
        let task = std::thread::spawn(move || {
            writer.write(move |_| {
                started.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(42)
            })
        });
        start_rx.recv().unwrap();
        let retired = Arc::clone(&sessions);
        let retire_id = id.clone();
        let join = std::thread::spawn(move || retired.invalidate(&retire_id));
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while db.ensure_alive().is_ok() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(db.ensure_alive().is_err());
        let (opened, open_rx) = mpsc::channel();
        let next_sessions = Arc::clone(&sessions);
        let next_id = id.clone();
        let next_root = a.clone();
        let next = std::thread::spawn(move || {
            next_sessions.acquire_with(
                &next_id,
                || Ok(next_root),
                OpenOpts::new(None, 0),
                |root, id, generation, opts| {
                    opened.send(()).unwrap();
                    CatalogDb::open_session(root, id, generation, opts)
                },
            )
        });
        assert!(matches!(
            open_rx.recv_timeout(Duration::from_millis(20)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        let (done, done_rx) = mpsc::channel();
        let live = Arc::clone(&sessions);
        let second = std::thread::spawn(move || {
            let db = live
                .acquire(&other, || Ok(b), OpenOpts::new(None, 0))
                .unwrap();
            db.set_import_template(":FILENAME").unwrap();
            done.send(()).unwrap();
        });
        done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        release.send(()).unwrap();
        assert_eq!(task.join().unwrap().unwrap(), 42);
        join.join().unwrap().unwrap();
        let new = next.join().unwrap().unwrap();
        assert!(new.generation() > db.generation());
        new.set_import_template(":FILENAME").unwrap();
        second.join().unwrap();
        assert!(matches!(db.ensure_current(), Err(Error::SessionExpired)));
    }
    #[test]
    fn injected_factory_swap_open_failure_and_generation_overflow() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("原库");
        let b = dir.path().join("另一库");
        let id = catalog(&a);
        let other = catalog(&b);
        let sessions = CatalogSessions::default();
        let result = sessions.acquire_with(
            &id,
            || Ok(a.clone()),
            OpenOpts::new(None, 0),
            |root, expected, generation, opts| {
                std::fs::rename(root, dir.path().join("removed")).unwrap();
                std::fs::rename(&b, root).unwrap();
                CatalogDb::open_session(root, expected, generation, opts)
            },
        );
        assert!(matches!(result, Err(Error::RepositoryIdentityChanged(_))));
        assert_eq!(
            repository::read_repository_meta(&a.join("catalog.db"))
                .unwrap()
                .id,
            other
        );
        std::fs::rename(&a, &b).unwrap();
        std::fs::rename(dir.path().join("removed"), &a).unwrap();
        let failure = sessions.acquire_with(
            &id,
            || Ok(a.clone()),
            OpenOpts::new(None, 0),
            |_, _, _, _| Err(Error::WriterGone),
        );
        assert!(matches!(failure, Err(Error::WriterGone)));
        sessions.generation.store(u64::MAX, Ordering::Release);
        let exhausted = sessions.acquire_with(
            &id,
            || Ok(a.clone()),
            OpenOpts::new(None, 0),
            |_, _, _, _| panic!("溢出后不能开连接"),
        );
        assert!(matches!(exhausted, Err(Error::Unsupported(_))));
    }
    #[test]
    fn same_id_copy_at_same_path_invalidates_existing_file_handle() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("库");
        let id = catalog(&root);
        let db = CatalogDb::open_expected(&root, &id, OpenOpts::new(None, 0)).unwrap();
        let copy = dir.path().join("copy.db");
        std::fs::copy(db.path(), &copy).unwrap();
        std::fs::rename(db.path(), root.join("old.db")).unwrap();
        std::fs::rename(copy, db.path()).unwrap();
        assert!(matches!(
            db.set_import_template("禁止写副本"),
            Err(Error::RepositoryIdentityChanged(_))
        ));
        assert_ne!(
            repository::read_repository_meta(db.path())
                .unwrap()
                .import_template,
            "禁止写副本"
        );
    }
    #[test]
    fn write_thread_failure_is_joined_and_next_attempt_can_recover() {
        let dir = tempfile::tempdir().unwrap();
        let id = catalog(dir.path());
        let sessions = CatalogSessions::default();
        let db = sessions
            .acquire(&id, || Ok(dir.path().into()), OpenOpts::new(None, 0))
            .unwrap();
        assert!(db.write::<(), _>(|_| panic!("模拟连接线程失败")).is_err());
        assert!(matches!(db.ensure_alive(), Err(Error::SessionExpired)));
        assert_eq!(
            db.connection_failure().unwrap().reason,
            Some(super::super::availability::Reason::ConnectionLost)
        );
        // actor 已捕获任务 panic 并退出；关闭仍须 join 完成。
        sessions.invalidate(&id).unwrap();
        sessions.invalidate(&id).unwrap();
        let recovered = sessions
            .acquire(&id, || Ok(dir.path().into()), OpenOpts::new(None, 0))
            .unwrap();
        recovered.set_import_template(":FILENAME").unwrap();
    }
    #[test]
    fn constraint_and_input_errors_do_not_disconnect_library() {
        assert!(!invalidates_session(&Error::Unsupported("坏照片".into())));
        assert!(!invalidates_session(&Error::Database(
            rusqlite::Error::InvalidParameterName("x".into())
        )));
        assert!(
            super::super::location::require_catalog_location(Path::new(r"\\nas\photos")).is_err()
        );
    }
}
