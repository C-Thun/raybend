//! 位置探测与有界调度；远程来源可复用位置分类，catalog 策略仍仅允许本地。
use super::{location, repository, time};
use crate::{Error, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

pub const MAX_PROBES: usize = 4;
pub const PROBE_WAIT: Duration = Duration::from_secs(5);
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Unknown,
    Checking,
    Online,
    Offline,
    Unavailable,
    Releasing,
    Released,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    IdentityMismatch,
    MultipleLocations,
    AccessDenied,
    ReadOnly,
    CatalogInvalid,
    IoFailure,
    StorageFull,
    ConnectionLost,
    SchemaTooNew,
    MigrationFailed,
    Timeout,
    UnsupportedLocation,
    Busy,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    pub repository_id: String,
    pub state: Availability,
    pub reason: Option<Reason>,
    pub root: Option<String>,
    /// 十进制字符串，避免 JS 的 53 位精度边界。
    pub generation: String,
    pub revision: String,
    pub observed_at: i64,
}
impl Default for ConnectionStatus {
    fn default() -> Self {
        Self::unknown("")
    }
}
impl ConnectionStatus {
    pub fn unknown(id: &str) -> Self {
        Self {
            repository_id: id.into(),
            state: Availability::Unknown,
            reason: None,
            root: None,
            generation: "0".into(),
            revision: "0".into(),
            observed_at: time::now_millis(),
        }
    }
    pub fn failed(id: &str, error: &Error) -> Self {
        let mut status = Self::unknown(id);
        status.state = Availability::Unavailable;
        status.reason = Some(match error {
            Error::RepositoryOffline { .. } | Error::PathNotFound(_) | Error::SessionExpired => {
                status.state = Availability::Offline;
                return status;
            }
            Error::RepositoryReleased => {
                status.state = Availability::Released;
                status.reason = None;
                return status;
            }
            Error::RepositoryCopies => Reason::MultipleLocations,
            Error::RepositoryBusy => Reason::Busy,
            Error::RepositoryIdentityChanged(_) => Reason::IdentityMismatch,
            Error::UnsupportedCatalogLocation(_) => Reason::UnsupportedLocation,
            Error::SchemaTooNew { .. } => Reason::SchemaTooNew,
            Error::Migration { .. } | Error::IntegrityCheck(_) => Reason::MigrationFailed,
            Error::WriterGone => Reason::ConnectionLost,
            Error::StorageProbeBusy => Reason::Busy,
            Error::StorageProbeTimeout => Reason::Timeout,
            Error::Io(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                Reason::AccessDenied
            }
            Error::Io(_) => Reason::IoFailure,
            Error::Database(rusqlite::Error::SqliteFailure(code, _)) => match code.code {
                rusqlite::ErrorCode::PermissionDenied => Reason::AccessDenied,
                rusqlite::ErrorCode::ReadOnly => Reason::ReadOnly,
                rusqlite::ErrorCode::DiskFull => Reason::StorageFull,
                rusqlite::ErrorCode::SystemIoFailure | rusqlite::ErrorCode::CannotOpen => {
                    Reason::IoFailure
                }
                _ => Reason::CatalogInvalid,
            },
            _ => Reason::CatalogInvalid,
        });
        status
    }
}

/// 不折叠权限/损坏/身份错配为拔盘；优先返回可用路径，全部失败保留具体原因。
pub fn resolve_paths(id: &str, paths: &[String]) -> Result<PathBuf> {
    resolve_preferred(id, paths, None)
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PreferredLocation {
    pub path: String,
    pub identity: super::file_id::FileId,
}
pub fn preference_key(id: &str) -> String {
    format!("repository.preferred_location.v1.{id}")
}
pub fn resolve_preferred(
    id: &str,
    paths: &[String],
    preferred: Option<&PreferredLocation>,
) -> Result<PathBuf> {
    let mut failure = None;
    let mut found: Vec<(PathBuf, Option<super::file_id::FileId>)> = Vec::new();
    for path in paths {
        let root = Path::new(path);
        if let Err(error) = location::require_catalog_location(root) {
            failure.get_or_insert(error);
            continue;
        }
        let catalog = root.join(repository::CATALOG_FILE_NAME);
        match std::fs::metadata(&catalog) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                failure.get_or_insert(Error::Io(e));
                continue;
            }
            Ok(meta) if !meta.is_file() => {
                failure.get_or_insert(Error::NotARepository {
                    path: catalog.clone(),
                    reason: "catalog 不是文件".into(),
                });
                continue;
            }
            _ => {}
        }
        match repository::read_repository_meta(&catalog) {
            Ok(meta) if meta.id == id => {
                let identity =
                    super::file_id::FileId::try_read(&catalog).filter(|id| !id.is_zero());
                if !found.iter().any(|(existing, file_id)| {
                    identity.is_some() && *file_id == identity
                        || existing
                            .canonicalize()
                            .ok()
                            .zip(root.canonicalize().ok())
                            .is_some_and(|(a, b)| a == b)
                }) {
                    found.push((root.into(), identity));
                }
            }
            Ok(_) => {
                failure.get_or_insert(Error::RepositoryIdentityChanged(catalog));
            }
            Err(error) => {
                failure.get_or_insert(error);
            }
        }
    }
    if let Some(preferred) = preferred {
        if let Some((root, _)) = found
            .iter()
            .find(|(_, identity)| *identity == Some(preferred.identity))
        {
            return Ok(root.clone());
        }
        if !found.is_empty() {
            return Err(Error::RepositoryCopies);
        }
    } else {
        if found.len() > 1 {
            return Err(Error::RepositoryCopies);
        }
        if let Some((root, _)) = found.pop() {
            return Ok(root);
        }
    }
    Err(failure.unwrap_or_else(|| Error::RepositoryOffline {
        name: id.into(),
        tried: paths.len(),
    }))
}

#[derive(Default)]
pub struct Snapshots {
    rows: Mutex<HashMap<String, ConnectionStatus>>,
    revision: AtomicU64,
}
impl Snapshots {
    pub fn get(&self, id: &str) -> ConnectionStatus {
        self.rows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(id)
            .cloned()
            .unwrap_or_else(|| ConnectionStatus::unknown(id))
    }
    pub fn publish(&self, status: ConnectionStatus) -> Result<ConnectionStatus> {
        self.publish_if(status, None)?.ok_or(Error::SessionExpired)
    }
    /// 请求代次比较与发布在同一把短锁中，操作结果可使旧探测失去发布权。
    pub fn publish_if(
        &self,
        mut status: ConnectionStatus,
        expected: Option<&str>,
    ) -> Result<Option<ConnectionStatus>> {
        let mut rows = self
            .rows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = rows.get(&status.repository_id);
        if expected.is_some_and(|v| previous.map_or("0", |row| row.revision.as_str()) != v) {
            return Ok(None);
        }
        if previous.is_some_and(|row| {
            status.generation != "0"
                && status.generation.parse::<u64>().unwrap_or(0)
                    < row.generation.parse::<u64>().unwrap_or(0)
        }) {
            return Ok(None);
        }
        if status.generation == "0"
            && let Some(previous) = previous
        {
            status.generation = previous.generation.clone();
        }
        let revision = self
            .revision
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| Error::Unsupported("状态代次已耗尽，请重启应用".into()))?
            + 1;
        status.revision = revision.to_string();
        status.observed_at = time::now_millis();
        rows.insert(status.repository_id.clone(), status.clone());
        Ok(Some(status))
    }
}

/// 调度提示不是库身份。Windows 按盘符/UNC 共享合并；Unix 常见挂载路径按挂载目录
/// 合并，其余本地路径保守共用根端点。只处理登记字符串，不在预算锁内查盘。
pub fn endpoint_hint(path: &str) -> String {
    let folded = super::path_semantics::PathForms::new(path);
    let p = folded.folded();
    if p.as_bytes().get(1) == Some(&b':') {
        return p[..2].into();
    }
    let parts: Vec<_> = p
        .trim_start_matches('\\')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    if super::path_semantics::is_unc(path) {
        return format!(
            "//{}",
            parts.into_iter().take(2).collect::<Vec<_>>().join("/")
        );
    }
    let count = match parts.first().copied() {
        Some("media" | "run") => 3,
        Some("mnt" | "volumes") => 2,
        _ => 0,
    };
    format!(
        "/{}",
        parts.into_iter().take(count).collect::<Vec<_>>().join("/")
    )
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProbeWait {
    Busy,
    Timeout,
    WorkerGone,
}
/// 超时不会取消 OS I/O；未退出的线程继续占同库/全局名额。
#[derive(Default)]
pub struct ProbeBudget {
    running: Arc<Mutex<HashMap<String, Vec<String>>>>,
}
impl ProbeBudget {
    pub fn run<T: Send + 'static>(
        &self,
        id: &str,
        wait: Duration,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> std::result::Result<T, ProbeWait> {
        self.run_endpoints(id, &[], wait, f)
    }
    pub fn run_endpoints<T: Send + 'static>(
        &self,
        id: &str,
        endpoints: &[String],
        wait: Duration,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> std::result::Result<T, ProbeWait> {
        let id = id.to_string();
        {
            let mut running = self
                .running
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if running.len() >= MAX_PROBES
                || running.contains_key(&id)
                || running
                    .values()
                    .any(|keys| endpoints.iter().any(|key| keys.contains(key)))
            {
                return Err(ProbeWait::Busy);
            }
            running.insert(id.clone(), endpoints.to_vec());
        }
        let active = Arc::clone(&self.running);
        let failed_id = id.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        if std::thread::Builder::new()
            .name("repository-probe".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
                if let Ok(value) = result {
                    let _ = tx.send(value);
                }
                active
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(&id);
            })
            .is_err()
        {
            self.running
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&failed_id);
            return Err(ProbeWait::WorkerGone);
        }
        match rx.recv_timeout(wait) {
            Ok(v) => Ok(v),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(ProbeWait::Timeout),
            Err(_) => Err(ProbeWait::WorkerGone),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::db::{CatalogDb, OpenOpts};
    use super::*;
    #[test]
    fn same_id_copies_require_choice_and_missing_chosen_entity_never_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("库 A");
        let b = dir.path().join("库 B");
        let db = CatalogDb::create(&a, "库", None, OpenOpts::new(None, 0)).unwrap();
        let id = db.meta().id.clone();
        drop(db);
        std::fs::create_dir(&b).unwrap();
        std::fs::copy(a.join("catalog.db"), b.join("catalog.db")).unwrap();
        let paths = vec![
            a.to_string_lossy().into_owned(),
            b.to_string_lossy().into_owned(),
        ];
        assert!(matches!(
            resolve_paths(&id, &paths),
            Err(Error::RepositoryCopies)
        ));
        let preferred = PreferredLocation {
            path: paths[1].clone(),
            identity: super::super::file_id::FileId::try_read(b.join("catalog.db")).unwrap(),
        };
        assert_eq!(resolve_preferred(&id, &paths, Some(&preferred)).unwrap(), b);
        std::fs::rename(&b, dir.path().join("offline")).unwrap();
        assert!(matches!(
            resolve_preferred(&id, &paths, Some(&preferred)),
            Err(Error::RepositoryCopies)
        ));
        let alias = dir.path().join("别名");
        std::fs::create_dir(&alias).unwrap();
        std::fs::hard_link(a.join("catalog.db"), alias.join("catalog.db")).unwrap();
        assert_eq!(
            resolve_paths(
                &id,
                &[paths[0].clone(), alias.to_string_lossy().into_owned()]
            )
            .unwrap(),
            a
        );
    }
    #[test]
    fn storage_faults_do_not_claim_catalog_corruption() {
        for (code, reason) in [
            (rusqlite::ffi::SQLITE_IOERR, Reason::IoFailure),
            (rusqlite::ffi::SQLITE_CANTOPEN, Reason::IoFailure),
            (rusqlite::ffi::SQLITE_FULL, Reason::StorageFull),
            (rusqlite::ffi::SQLITE_READONLY, Reason::ReadOnly),
            (rusqlite::ffi::SQLITE_CORRUPT, Reason::CatalogInvalid),
        ] {
            let error = Error::Database(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(code),
                None,
            ));
            assert_eq!(ConnectionStatus::failed("库", &error).reason, Some(reason));
        }
        assert_eq!(
            ConnectionStatus::failed("库", &Error::WriterGone).reason,
            Some(Reason::ConnectionLost)
        );
        assert_eq!(
            ConnectionStatus::failed(
                "库",
                &Error::Io(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            )
            .reason,
            Some(Reason::AccessDenied)
        );
    }
    #[test]
    fn absence_invalid_identity_and_corrupt_file_have_distinct_reasons() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("中文目录");
        assert!(matches!(
            resolve_paths("x", &[]),
            Err(Error::RepositoryOffline { tried: 0, .. })
        ));
        let db = CatalogDb::create(&path, "库", None, OpenOpts::new(None, 0)).unwrap();
        let id = db.meta().id.clone();
        drop(db);
        assert_eq!(
            resolve_paths(&id, &[path.to_string_lossy().into()]).unwrap(),
            path
        );
        assert!(matches!(
            resolve_paths("other", &[path.to_string_lossy().into()]),
            Err(Error::RepositoryIdentityChanged(_))
        ));
        std::fs::write(path.join("catalog.db"), b"broken").unwrap();
        let error = resolve_paths(&id, &[path.to_string_lossy().into()]).unwrap_err();
        assert_eq!(
            ConnectionStatus::failed(&id, &error).reason,
            Some(Reason::CatalogInvalid)
        );
    }
    #[test]
    fn timeout_retains_budget_and_duplicate_cannot_spawn() {
        let budget = ProbeBudget::default();
        let mut releases = Vec::new();
        for i in 0..MAX_PROBES {
            let (tx, rx) = mpsc::channel();
            releases.push(tx);
            assert_eq!(
                budget.run(&i.to_string(), Duration::from_millis(10), move || rx
                    .recv()
                    .unwrap()),
                Err(ProbeWait::Timeout)
            );
        }
        assert_eq!(
            budget.run("next", Duration::ZERO, || ()),
            Err(ProbeWait::Busy)
        );
        assert_eq!(budget.run("0", Duration::ZERO, || ()), Err(ProbeWait::Busy));
        for tx in releases {
            tx.send(()).unwrap();
        }
    }
    #[test]
    fn endpoint_single_flight_and_late_revision_are_bounded() {
        assert_eq!(endpoint_hint(r"E:\照片\库"), endpoint_hint("e:/另一库"));
        assert_eq!(endpoint_hint(r"\\NAS\Share\照片"), "//nas/share");
        assert_eq!(
            endpoint_hint(r"\\NAS\Share\照片"),
            endpoint_hint("//nas/share/另一库")
        );
        let budget = ProbeBudget::default();
        let (tx, rx) = mpsc::channel();
        assert_eq!(
            budget.run_endpoints("a", &["e:".into()], Duration::from_millis(5), move || rx
                .recv()
                .unwrap()),
            Err(ProbeWait::Timeout)
        );
        assert_eq!(
            budget.run_endpoints("b", &["e:".into()], Duration::ZERO, || ()),
            Err(ProbeWait::Busy)
        );
        assert_eq!(
            budget.run_endpoints("c", &["f:".into()], Duration::from_secs(1), || 7),
            Ok(7)
        );
        tx.send(()).unwrap();
        let snapshots = Snapshots::default();
        let old = snapshots.publish(ConnectionStatus::unknown("库")).unwrap();
        let mut online = ConnectionStatus::unknown("库");
        online.state = Availability::Online;
        online.generation = "9007199254740993".into();
        let new = snapshots.publish(online).unwrap();
        assert!(
            snapshots
                .publish_if(ConnectionStatus::unknown("库"), Some(&old.revision))
                .unwrap()
                .is_none()
        );
        assert_eq!(snapshots.get("库").revision, new.revision);
        let mut stale = ConnectionStatus::unknown("库");
        stale.generation = "1".into();
        stale.state = Availability::Offline;
        assert!(snapshots.publish_if(stale, None).unwrap().is_none());
        snapshots.revision.store(u64::MAX, Ordering::Release);
        assert!(snapshots.publish(ConnectionStatus::unknown("库")).is_err());
        assert_eq!(snapshots.get("库").generation, "9007199254740993");
    }
    #[test]
    fn schema_and_permission_are_not_offline() {
        assert_eq!(
            ConnectionStatus::failed(
                "库",
                &Error::SchemaTooNew {
                    found: 9,
                    supported: 2
                }
            )
            .reason,
            Some(Reason::SchemaTooNew)
        );
        assert_eq!(
            ConnectionStatus::failed(
                "库",
                &Error::Io(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            )
            .reason,
            Some(Reason::AccessDenied)
        );
        let snapshots = Snapshots::default();
        let one = snapshots.publish(ConnectionStatus::unknown("库")).unwrap();
        let two = snapshots.publish(ConnectionStatus::unknown("库")).unwrap();
        assert!(one.revision.parse::<u64>().unwrap() < two.revision.parse::<u64>().unwrap());
    }
}
