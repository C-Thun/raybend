//! 有界目录监听：事件只提示重扫，绝不直接修改照片记录。
//! 活跃目录最多 32 个，非递归监听；200ms 去抖，连续事件最多等待 1s。
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

pub const MAX_SCOPES: usize = 32;
const MAX_BATCH: usize = MAX_SCOPES * 2 + 2;

#[derive(Default)]
pub struct DirtyBatch {
    scopes: BTreeSet<String>,
    overflow: bool,
    first: Option<u64>,
    last: u64,
}
impl DirtyBatch {
    pub fn add(&mut self, scopes: impl IntoIterator<Item = String>, now: u64) {
        for scope in scopes {
            if self.scopes.len() < MAX_BATCH {
                self.scopes.insert(scope);
            } else if !self.scopes.contains(&scope) {
                self.overflow = true;
            }
        }
        if !self.scopes.is_empty() {
            self.first.get_or_insert(now);
            self.last = now;
        }
    }
    pub fn take_ready(&mut self, now: u64) -> Option<Vec<String>> {
        let first = self.first?;
        if now.saturating_sub(self.last) < 200 && now.saturating_sub(first) < 1000 {
            return None;
        }
        self.first = None;
        let scopes = std::mem::take(&mut self.scopes);
        if std::mem::take(&mut self.overflow) {
            Some(Vec::new())
        } else {
            Some(scopes.into_iter().collect())
        }
    }
}

fn includes_path(photos: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(photos) else {
        return false;
    };
    if rel
        .components()
        .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
    {
        return false;
    }
    !path.extension().is_some_and(|ext| {
        matches!(
            ext.to_string_lossy().to_ascii_lowercase().as_str(),
            "tmp" | "db" | "db-wal" | "db-shm" | "xmp"
        )
    })
}

/// 同一批改名含旧/新路径；父范围与被移动的活跃子范围同时重扫。
pub fn affected_scopes(photos: &Path, active: &VecDeque<String>, paths: &[PathBuf]) -> Vec<String> {
    let mut scopes = BTreeSet::new();
    for path in paths {
        if !includes_path(photos, path) {
            continue;
        }
        let rel = path.strip_prefix(photos).expect("已经检查过范围");
        let relative = rel.to_string_lossy().replace('\\', "/");
        let scoped = if relative.is_empty() {
            "photos".into()
        } else {
            format!("photos/{relative}")
        };
        let is_directory = path.is_dir()
            || active
                .iter()
                .any(|s| s == &scoped || s.starts_with(&format!("{scoped}/")))
            || path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("_RAW"));
        let dir = if is_directory {
            scoped
        } else {
            scoped
                .rsplit_once('/')
                .map_or("photos", |(dir, _)| dir)
                .to_string()
        };
        let dir = dir.strip_suffix("/_RAW").unwrap_or(&dir).to_string();
        scopes.insert(dir.clone());
        if let Some((parent, _)) = dir.rsplit_once('/') {
            scopes.insert(parent.to_string());
        }
        for scope in active {
            if scope.starts_with(&format!("{dir}/")) {
                scopes.insert(scope.clone());
            }
        }
    }
    // 搬走的目录可能带着已访问的子目录；新位置同样纳入，不全库递归。
    if paths.len() == 2
        && let (Ok(old), Ok(new)) = (paths[0].strip_prefix(photos), paths[1].strip_prefix(photos)) {
            let old = format!("photos/{}", old.to_string_lossy().replace('\\', "/"));
            let new = format!("photos/{}", new.to_string_lossy().replace('\\', "/"));
            for scope in active {
                if let Some(suffix) = scope.strip_prefix(&format!("{old}/")) {
                    scopes.insert(format!("{new}/{suffix}"));
                }
            }
        }
    scopes.into_iter().take(MAX_BATCH).collect()
}

pub struct CatalogWatcher {
    watcher: Option<RecommendedWatcher>,
    sender: mpsc::SyncSender<Option<Vec<PathBuf>>>,
    active: Arc<Mutex<VecDeque<String>>>,
    watched: BTreeSet<PathBuf>,
    identities: BTreeMap<PathBuf, Option<crate::store::file_id::FileId>>,
    photos: PathBuf,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl CatalogWatcher {
    pub fn new(
        root: &Path,
        on_dirty: impl Fn(Vec<String>) + Send + 'static,
    ) -> Result<Self, String> {
        let photos = root.join("photos");
        let (sender, receiver) = mpsc::sync_channel::<Option<Vec<PathBuf>>>(64);
        let overflow = Arc::new(AtomicBool::new(false));
        let tx = sender.clone();
        let overloaded = overflow.clone();
        let event_photos = photos.clone();
        let watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            // Access/read events are not changes; errors trigger reread of active scopes.
            match result {
                Ok(mut event) if !matches!(event.kind, notify::EventKind::Access(_)) => {
                    event
                        .paths
                        .retain(|path| includes_path(&event_photos, path));
                    if !event.paths.is_empty() && tx.try_send(Some(event.paths)).is_err() {
                        overloaded.store(true, Ordering::Relaxed);
                    }
                }
                Err(_) => {
                    overloaded.store(true, Ordering::Relaxed);
                }
                _ => {}
            }
        })
        .map_err(|e| e.to_string())?;
        let active = Arc::new(Mutex::new(VecDeque::<String>::new()));
        let dirs = active.clone();
        let base = photos.clone();
        let thread = std::thread::spawn(move || {
            let start = Instant::now();
            let mut batch = DirtyBatch::default();
            loop {
                let now = || start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                match receiver.recv_timeout(Duration::from_millis(100)) {
                    Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Ok(Some(paths)) => {
                        if let Ok(active) = dirs.lock() {
                            batch.add(affected_scopes(&base, &active, &paths), now());
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                if overflow.swap(false, Ordering::Relaxed)
                    && let Ok(active) = dirs.lock() {
                        batch.add(active.iter().cloned(), now());
                    }
                if let Some(scopes) = batch.take_ready(now()) {
                    on_dirty(scopes);
                }
            }
        });
        Ok(Self {
            watcher: Some(watcher),
            sender,
            active,
            watched: BTreeSet::new(),
            identities: BTreeMap::new(),
            photos,
            thread: Some(thread),
        })
    }
    pub fn active_scopes(&self) -> Vec<String> {
        self.active
            .lock()
            .map(|active| {
                active
                    .iter()
                    .cloned()
                    .chain(["photos".to_string()])
                    .collect()
            })
            .unwrap_or_else(|_| vec!["photos".into()])
    }
    /// 成功读过的目录才加入；父目录能报告该目录被移走/删掉。
    pub fn visit(&mut self, scope: &str) -> Result<(), String> {
        crate::store::rebuild::validate_scope("photos", scope).map_err(|e| e.to_string())?;
        let mut active = self.active.lock().map_err(|e| e.to_string())?;
        active.retain(|path| path != scope);
        active.push_back(scope.to_string());
        while active.len() > MAX_SCOPES {
            active.pop_front();
        }
        let root = self.photos.parent().ok_or("photos 缺少父目录")?;
        let mut desired = BTreeSet::from([self.photos.clone(), root.to_path_buf()]);
        for scope in active.iter() {
            let dir = root.join(scope);
            let raw = std::fs::read_dir(&dir).ok().and_then(|entries| {
                entries.filter_map(Result::ok).find(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .eq_ignore_ascii_case("_RAW")
                })
            });
            desired.insert(raw.map_or_else(|| dir.join("_RAW"), |entry| entry.path()));
            if let Some(parent) = dir.parent()
                && parent.starts_with(&self.photos) {
                    desired.insert(parent.to_path_buf());
                }
            desired.insert(dir);
        }
        // 不监听符号链接，失效目录由其父目录继续覆盖。
        let canonical_photos = std::fs::canonicalize(&self.photos).map_err(|e| e.to_string())?;
        desired.retain(|path| {
            std::fs::symlink_metadata(path)
                .is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
                && (path == root
                    || std::fs::canonicalize(path)
                        .is_ok_and(|canonical| canonical.starts_with(&canonical_photos)))
        });
        let identities: BTreeMap<_, _> = desired
            .iter()
            .map(|path| (path.clone(), crate::store::file_id::FileId::try_read(path)))
            .collect();
        let stale: Vec<_> = self
            .watched
            .iter()
            .filter(|path| {
                !desired.contains(*path)
                    || identities.get(*path).is_none_or(|identity| {
                        identity.is_none() || self.identities.get(*path) != Some(identity)
                    })
            })
            .cloned()
            .collect();
        let watcher = self.watcher.as_mut().ok_or("监听已停止")?;
        for path in stale {
            let _ = watcher.unwatch(&path);
            self.watched.remove(&path);
        }
        self.identities = identities;
        for path in desired
            .difference(&self.watched)
            .cloned()
            .collect::<Vec<_>>()
        {
            watcher
                .watch(&path, RecursiveMode::NonRecursive)
                .map_err(|e| format!("监听目录失败：{}：{e}", path.display()))?;
            self.watched.insert(path);
        }
        Ok(())
    }
}
impl Drop for CatalogWatcher {
    fn drop(&mut self) {
        self.watcher.take();
        let _ = self.sender.send(None);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn storm_is_merged_with_bounded_latency() {
        let mut batch = DirtyBatch::default();
        for tick in 0..1000 {
            batch.add(["photos/a".into(), "photos/a".into()], tick);
            assert!(batch.take_ready(tick).is_none());
        }
        assert_eq!(batch.take_ready(1000), Some(vec!["photos/a".into()]));
        assert_eq!(batch.take_ready(2000), None);
        batch.add(["photos/b".into()], 2100);
        assert_eq!(batch.take_ready(2299), None);
        assert_eq!(batch.take_ready(2300), Some(vec!["photos/b".into()]));
    }
    #[test]
    fn raw_and_directory_moves_include_both_bounded_scopes() {
        let root = Path::new("/library/photos");
        let active = VecDeque::from(["photos/旧/中文".into()]);
        let scopes = affected_scopes(root, &active, &[root.join("旧"), root.join("新")]);
        assert!(scopes.contains(&"photos/旧/中文".into()));
        assert!(scopes.contains(&"photos/新/中文".into()));
        let scopes = affected_scopes(root, &active, &[root.join("旧/_RAW/a.NEF")]);
        assert!(scopes.contains(&"photos/旧".into()));
        assert!(!scopes.contains(&"photos/旧/_RAW".into()));
        assert!(
            affected_scopes(
                root,
                &active,
                &[
                    PathBuf::from("/library/cache/a.avif"),
                    root.join("a.tmp"),
                    root.join(".hidden/a.jpg")
                ]
            )
            .is_empty()
        );
    }
    #[test]
    fn batch_and_active_watch_sets_are_bounded() {
        let mut batch = DirtyBatch::default();
        batch.add((0..10000).map(|i| format!("photos/{i}")), 0);
        assert!(batch.take_ready(1000).unwrap().len() <= MAX_BATCH);
    }
    #[test]
    fn native_watcher_installs_and_releases_without_recursion() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("photos")).unwrap();
        let mut watcher = CatalogWatcher::new(root.path(), |_| {}).unwrap();
        watcher.visit("photos").unwrap();
        assert_eq!(watcher.watched.len(), 2);
        for i in 0..40 {
            let scope = format!("photos/{i}");
            std::fs::create_dir(root.path().join(&scope)).unwrap();
            watcher.visit(&scope).unwrap();
        }
        assert_eq!(watcher.active.lock().unwrap().len(), MAX_SCOPES);
        assert!(watcher.watched.len() <= MAX_SCOPES * 3 + 2);
    }
    #[test]
    fn native_events_arrive_and_root_changes_do_not_self_trigger() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("photos")).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = CatalogWatcher::new(root.path(), move |scopes| {
            let _ = tx.send(scopes);
        })
        .unwrap();
        watcher.visit("photos").unwrap();
        std::fs::write(root.path().join("catalog.db"), b"not watched").unwrap();
        std::fs::write(root.path().join("photos/new.jpg"), b"photo").unwrap();
        let scopes = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("原生变更通知");
        assert!(scopes.contains(&"photos".into()));
        assert!(!includes_path(
            &root.path().join("photos"),
            &root.path().join("catalog.db")
        ));
    }
    #[cfg(unix)]
    #[test]
    fn directory_replacement_reinstalls_watch_and_does_not_follow_escape_links() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("photos")).unwrap();
        let mut watcher = CatalogWatcher::new(root.path(), |_| {}).unwrap();
        watcher.visit("photos").unwrap();
        let path = root.path().join("photos");
        let original = watcher.identities[&path];
        std::fs::rename(&path, root.path().join("parked")).unwrap();
        std::fs::create_dir(&path).unwrap();
        watcher.visit("photos").unwrap();
        assert_ne!(watcher.identities[&path], original);
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), path.join("outside")).unwrap();
        watcher.visit("photos/outside").unwrap();
        assert!(!watcher.watched.contains(&path.join("outside")));
        assert!(watcher.visit("photos/../cache").is_err());
    }
}
