//! 读连接池：**自己写的小池子**（评审决定，不引 `r2d2`）。
//!
//! 为什么不需要 r2d2 那一套：池子里连的是**本机文件**，不是远程服务 ——
//! 没有网络抖动、没有认证过期、没有半开连接。真正需要防的只有两件事：
//!
//! 1. **借出的连接要还回去**（即使中途 panic）→ 用 [`PooledConnection`] 的 `Drop` 保证；
//! 2. **打开的连接数要有上限**（别把文件描述符吃光）→ `max` + `Condvar` 等待。
//!
//! 连接一旦出错（`SQLITE_CORRUPT` 之类），归还时**直接丢弃**、下次重新打开 ——
//! 这就是「基础防护」的全部内容。
//!
//! **写操作不从这里走**：池子里的连接都带 `query_only=ON`（[`pragma::apply_read_only`]），
//! 写一律经 [`super::writer::Writer`]。这是单写者设计在类型/运行时层面的兜底。

use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

use rusqlite::Connection;

use crate::error::{Error, Result};

use super::pragma;

/// 默认最大连接数。照片库的读并发需求不高（浏览是单线程滚动为主），
/// 但后台缩略图/元数据任务会同时读，所以给 8。
pub const DEFAULT_MAX: usize = 8;

/// 读连接池。
pub struct ReadPool {
    inner: Mutex<Inner>,
    cv: Condvar,
    path: PathBuf,
    /// 是否用 URI 形式打开（内存共享库、特殊路径用）
    uri: bool,
    max: usize,
}

#[derive(Default)]
struct Inner {
    idle: Vec<Connection>,
    /// 已打开（含借出）的连接总数
    total: usize,
}

impl ReadPool {
    /// 打开一个指向 `path` 的读池。
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        Self::with_max(path, DEFAULT_MAX)
    }

    /// 指定上限打开。`max` 至少为 1。
    pub fn with_max(path: impl Into<PathBuf>, max: usize) -> Result<Self> {
        let path = path.into();
        let max = max.max(1);
        // 立刻开一个连接：路径错/权限不足就在这里报出来，而不是等到第一次读
        let first = open_read_conn(&path, false)?;
        Ok(Self {
            inner: Mutex::new(Inner {
                idle: vec![first],
                total: 1,
            }),
            cv: Condvar::new(),
            path,
            uri: false,
            max,
        })
    }

    /// 用 URI 打开（`file:xxx?mode=memory&cache=shared` 这类）。
    /// 共享内存库主要用于测试。
    pub fn open_uri(uri: impl Into<String>, max: usize) -> Result<Self> {
        let uri = uri.into();
        let max = max.max(1);
        let first = open_read_conn(Path::new(&uri), true)?;
        Ok(Self {
            inner: Mutex::new(Inner {
                idle: vec![first],
                total: 1,
            }),
            cv: Condvar::new(),
            path: PathBuf::from(uri),
            uri: true,
            max,
        })
    }

    /// 借一个连接。用完 `Drop` 自动归还。
    ///
    /// 池满时**阻塞等待**（而不是无限开新连接）—— 这是「基础防护」的另一半。
    pub fn get(&self) -> Result<PooledConnection<'_>> {
        let conn = self.acquire()?;
        Ok(PooledConnection {
            pool: self,
            conn: Some(conn),
        })
    }

    /// 便捷方法：借一个连接执行 `f`。
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = self.get()?;
        f(&conn)
    }

    /// 池上限。
    #[must_use]
    pub fn max(&self) -> usize {
        self.max
    }

    /// 当前空闲连接数与已打开总数（测试/诊断用）。
    #[must_use]
    pub fn stats(&self) -> (usize, usize) {
        let st = lock(&self.inner);
        (st.idle.len(), st.total)
    }

    fn acquire(&self) -> Result<Connection> {
        let mut st = lock(&self.inner);
        loop {
            if let Some(conn) = st.idle.pop() {
                return Ok(conn);
            }
            if st.total < self.max {
                st.total += 1;
                drop(st);
                match open_read_conn(&self.path, self.uri) {
                    Ok(conn) => return Ok(conn),
                    Err(e) => {
                        // 开不出来就把名额还回去，别让池子永久少一个位置
                        let mut st = lock(&self.inner);
                        st.total -= 1;
                        self.cv.notify_one();
                        return Err(e);
                    }
                }
            }
            st = self.cv.wait(st).unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// 归还连接。**坏的连接直接丢**（丢弃后总数减一）。
    fn release(&self, conn: Connection) {
        let healthy = conn_is_healthy(&conn);
        let mut st = lock(&self.inner);
        if healthy {
            st.idle.push(conn);
        } else {
            st.total -= 1;
        }
        self.cv.notify_one();
    }
}

/// 借出的连接；`Drop` 时归还。
pub struct PooledConnection<'a> {
    pool: &'a ReadPool,
    conn: Option<Connection>,
}

impl std::ops::Deref for PooledConnection<'_> {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        // conn 只在 Drop 里被取走，借用期间一定在
        self.conn.as_ref().expect("连接已被归还")
    }
}

impl Drop for PooledConnection<'_> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.release(conn);
        }
    }
}

/// 打开一个「只读用途」的连接并套上统一设置。
///
/// **刻意不带 `SQLITE_OPEN_CREATE`**：读的一方绝不该创建文件。
/// 否则「路径拼错」会在错误的位置安静地生出一个空 catalog.db —— 那是比报错严重得多的事故。
///
/// 这里仍然用 `READ_WRITE` 打开（而不是 `READ_ONLY`）：WAL 库的读者需要能碰 `-shm`，
/// 真正的「不许写」由 `query_only` 保证。
fn open_read_conn(path: &Path, uri: bool) -> Result<Connection> {
    use rusqlite::OpenFlags;
    if !uri && !path.exists() {
        return Err(Error::PathNotFound(path.to_path_buf()));
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_URI
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags)?;
    pragma::apply_read_only(&conn)?;
    Ok(conn)
}

/// 一次极轻量的探活：能读 `user_version` 就算健康。
fn conn_is_healthy(conn: &Connection) -> bool {
    conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
        .is_ok()
}

/// 加锁；锁被毒化时继续用（连接本身没坏，没理由因此拒绝服务）。
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 池子相关的错误（保留类型便于将来区分）。
#[allow(dead_code)]
fn _assert_error_is_used(_: Error) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migration::{self, Backups, DbKind};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    /// 建一个有真实表的文件库（池子测的都是文件库，内存库语义不同）。
    fn file_db() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.db");
        let mut conn = Connection::open(&path).unwrap();
        pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::Catalog, Backups::none(), 0).unwrap();
        drop(conn);
        dir
    }

    fn insert_asset(conn: &Connection, name: &str) {
        conn.execute(
            "INSERT INTO assets(imported_at, updated_at) VALUES (1, 1)",
            [],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO asset_files(asset_id, role, rel_path, rel_path_folded, ext, created_at, updated_at)
             VALUES (?1, 'bitmap', ?2, ?3, 'jpg', 1, 1)",
            rusqlite::params![id, name, name.to_lowercase()],
        )
        .unwrap();
    }

    #[test]
    fn borrow_and_return() {
        let dir = file_db();
        let pool = ReadPool::open(dir.path().join("catalog.db")).unwrap();
        assert_eq!(pool.stats(), (1, 1));
        {
            let conn = pool.get().unwrap();
            let n: i64 = conn
                .query_row("SELECT count(*) FROM assets", [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0);
            assert_eq!(pool.stats().1, 1, "借出时不新建连接");
        }
        assert_eq!(pool.stats(), (1, 1), "归还后连接回到池里");
    }

    #[test]
    fn reads_see_committed_data() {
        let dir = file_db();
        let path = dir.path().join("catalog.db");
        let pool = ReadPool::open(&path).unwrap();
        {
            let w = Connection::open(&path).unwrap();
            pragma::apply(&w, false).unwrap();
            insert_asset(&w, "photos/a.jpg");
        }
        let n: i64 = pool
            .with(|c| Ok(c.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn pool_connections_refuse_writes() {
        let dir = file_db();
        let pool = ReadPool::open(dir.path().join("catalog.db")).unwrap();
        let err: Result<()> = pool.with(|c| {
            c.execute(
                "INSERT INTO assets(imported_at, updated_at) VALUES (1,1)",
                [],
            )?;
            Ok(())
        });
        assert!(err.is_err(), "读池的连接必须拒绝写（query_only）");
    }

    #[test]
    fn respects_max_and_blocks_when_exhausted() {
        let dir = file_db();
        let pool = Arc::new(ReadPool::with_max(dir.path().join("catalog.db"), 1).unwrap());

        let held = pool.get().unwrap(); // 唯一的名额被占住
        let (tx, rx) = std::sync::mpsc::channel();
        let p2 = Arc::clone(&pool);
        let t = std::thread::spawn(move || {
            let start = Instant::now();
            let _conn = p2.get().unwrap();
            tx.send(start.elapsed()).unwrap();
        });

        // 立刻检查：第二个借用人应当还在等
        std::thread::sleep(Duration::from_millis(120));
        assert!(
            rx.try_recv().is_err(),
            "池满时必须等待，而不是无视上限继续开连接"
        );
        assert_eq!(pool.stats().1, 1, "等待期间不该多开连接");

        drop(held); // 释放名额
        let waited = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("应当拿到连接");
        assert!(waited >= Duration::from_millis(100), "确实等了：{waited:?}");
        t.join().unwrap();
        assert_eq!(pool.stats(), (1, 1));
    }

    #[test]
    fn concurrent_reads_work() {
        let dir = file_db();
        let path = dir.path().join("catalog.db");
        {
            let w = Connection::open(&path).unwrap();
            pragma::apply(&w, false).unwrap();
            for i in 0..20 {
                insert_asset(&w, &format!("photos/{i}.jpg"));
            }
        }
        let pool = Arc::new(ReadPool::open(&path).unwrap());
        let seen = Arc::new(AtomicUsize::new(0));
        std::thread::scope(|s| {
            for _ in 0..8 {
                let pool = Arc::clone(&pool);
                let seen = Arc::clone(&seen);
                s.spawn(move || {
                    for _ in 0..10 {
                        let n: i64 = pool
                            .with(|c| {
                                Ok(c.query_row("SELECT count(*) FROM asset_files", [], |r| {
                                    r.get(0)
                                })?)
                            })
                            .unwrap();
                        assert_eq!(n, 20);
                        seen.fetch_add(1, Ordering::Relaxed);
                    }
                });
            }
        });
        assert_eq!(seen.load(Ordering::Relaxed), 80);
        let (_, total) = pool.stats();
        assert!(total <= DEFAULT_MAX, "连接数不该超过上限：{total}");
    }

    #[test]
    fn connection_is_returned_even_after_error() {
        let dir = file_db();
        let pool = ReadPool::open(dir.path().join("catalog.db")).unwrap();
        let bad = pool.with(|c| {
            c.query_row("SELECT * FROM 不存在的表", [], |r| r.get::<_, i64>(0))
                .map_err(Error::from)
        });
        assert!(bad.is_err());
        assert_eq!(pool.stats(), (1, 1), "出错也要归还连接");
        // 还能继续用
        let n: i64 = pool
            .with(|c| Ok(c.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn opening_a_missing_path_fails_and_does_not_create_it() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("没有这个库.db");
        let err = ReadPool::open(&missing);
        assert!(err.is_err(), "路径不对必须立刻报错，而不是拖到第一次读");
        assert!(
            !missing.exists(),
            "读池**绝不能**顺手创建空库（路径打错会在错地方生出 catalog.db）"
        );
        // 错误要能说清是哪个路径（ReadPool 没有 Debug，所以用 .err()）
        let msg = err.err().expect("应当报错").to_string();
        assert!(msg.contains("没有这个库.db"), "错误信息里要带上路径：{msg}");
    }

    #[test]
    fn shared_memory_uri_works() {
        // 共享内存库：多个连接看到同一份数据（测试里很有用）
        let uri = "file:raybend_pool_test?mode=memory&cache=shared";
        let w = Connection::open_with_flags(
            uri,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                | rusqlite::OpenFlags::SQLITE_OPEN_URI
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .unwrap();
        pragma::apply(&w, false).unwrap();
        w.execute_batch("CREATE TABLE t(x INTEGER); INSERT INTO t VALUES (7);")
            .unwrap();

        let pool = ReadPool::open_uri(uri, 2).unwrap();
        let x: i64 = pool
            .with(|c| Ok(c.query_row("SELECT x FROM t", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(x, 7);
    }

    #[test]
    fn zero_max_is_clamped_to_one() {
        let dir = file_db();
        let pool = ReadPool::with_max(dir.path().join("catalog.db"), 0).unwrap();
        assert_eq!(pool.max(), 1);
        let _c = pool.get().unwrap();
    }
}
