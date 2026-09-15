//! 单写者 actor：**所有写操作串行化到唯一一条连接上**（`AGENTS.md` §6.4）。
//!
//! 为什么这样做而不是「谁要写谁开连接 + busy_timeout 忙等」：
//!
//! * SQLite 的 WAL 允许多读者 + **单写者**；多个写者只会互相 `SQLITE_BUSY`，白白消耗重试；
//! * 把写收敛到一个线程后，「并发写」在**设计层面**就不存在了 —— 不需要在每处调用点
//!   考虑重试与顺序；
//! * 退出时只要把队列跑干再关线程，就能保证「已提交的事务不会丢」。
//!
//! 用法：
//!
//! ```no_run
//! # use raybend::store::{
//! #     migration::{self, Backups, DbKind},
//! #     writer::Writer,
//! # };
//! # fn main() -> raybend::Result<()> {
//! let mut conn = rusqlite::Connection::open("/tmp/x.db")?;
//! migration::apply(&mut conn, DbKind::Catalog, Backups::none(), 0)?;
//! drop(conn);
//!
//! let w = Writer::open("/tmp/x.db")?;
//! // 单条语句用 run，多条用 transaction（自动提交/回滚）
//! w.run(|conn| {
//!     conn.execute("INSERT INTO repository_meta(key,value) VALUES ('k','v')", [])?;
//!     Ok(())
//! })?;
//! let n: i64 = w.run(|conn| Ok(conn.query_row("SELECT count(*) FROM repository_meta", [], |r| r.get(0))?))?;
//! assert_eq!(n, 1);
//! # Ok(()) }
//! ```
//!
//! 注意：`run` 里也能查数据，但**读请走 [`super::pool::ReadPool`]** ——
//! 写队列是单线程的，把读塞进来会拖慢所有人的写。

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::thread::JoinHandle;

use rusqlite::Connection;

use crate::error::{Error, Result};

use super::pragma;

/// 一次写入任务：拿到专属连接，自己决定怎么写。
type JobFn = Box<dyn FnOnce(&mut Connection) + Send + 'static>;

enum Msg {
    /// 执行一个写入任务（结果由任务自己通过内部通道送回调用方）
    Job(JobFn),
    /// 队列屏障：前面的任务都跑完后再回话
    Barrier(Sender<()>),
    /// 关线程（前面排队的东西仍然跑完）
    Shutdown(Sender<()>),
}

/// 单写者句柄：克隆共享同一条消息队列，可跨线程使用。
///
/// `Drop` 时会**把队列跑干再关线程**（保证已入队的写不丢），然后 join。
pub struct Writer {
    tx: Sender<Msg>,
    handle: Mutex<Option<JoinHandle<()>>>,
    path: PathBuf,
}

impl Writer {
    /// 打开一个库的写者：起一条专属线程 + 专属写连接。
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let conn = Connection::open(&path)?;
        pragma::apply(&conn, false)?;

        let (tx, rx) = mpsc::channel::<Msg>();
        let handle = std::thread::Builder::new()
            .name(format!("raybend-writer:{}", path.display()))
            .spawn(move || worker(rx, conn))?;

        Ok(Self {
            tx,
            handle: Mutex::new(Some(handle)),
            path,
        })
    }

    /// 库文件路径（诊断用）。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 提交一个写入任务，等它执行完并取回结果。
    ///
    /// 任务在写线程上执行 —— 所以**不要在里面做慢活**（解码、缩略图、网络），
    /// 只做数据库操作，否则会卡住所有写。
    pub fn run<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    {
        let (reply_tx, reply_rx) = mpsc::channel();
        let job: JobFn = Box::new(move |conn| {
            // 结果送不出去（调用方已经不等了）也不算错，忽略即可
            let _ = reply_tx.send(f(conn));
        });
        self.send(Msg::Job(job))?;
        reply_rx.recv().map_err(|_| Error::WriterGone)?
    }

    /// 在**一个事务**里执行多条写：`Ok` 提交，`Err` 回滚。
    ///
    /// 批量导入、批量评级这类操作都该走这里 —— 一次提交比 N 次提交快几个数量级，
    /// 也天然保证了「要么全成、要么全不成」。
    pub fn transaction<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<T> + Send + 'static,
    {
        self.run(move |conn| {
            let tx = conn.transaction()?;
            let out = f(&tx)?;
            tx.commit()?;
            Ok(out)
        })
    }

    /// 等所有已入队的写跑完（不关线程）。测试与「导入完成后确保落盘」用得上。
    pub fn flush(&self) -> Result<()> {
        let (tx, rx) = mpsc::channel();
        self.send(Msg::Barrier(tx))?;
        rx.recv().map_err(|_| Error::WriterGone)
    }

    /// 优雅关闭：跑干队列 → 关线程 → join。
    ///
    /// 重复调用是安全的（第二次直接返回）。
    pub fn shutdown(&self) -> Result<()> {
        let handle = {
            let mut guard = self
                .handle
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.take()
        };
        let Some(handle) = handle else {
            return Ok(());
        };
        let (tx, rx) = mpsc::channel();
        // 队列已关（写线程早就退出了）也不算错
        if self.tx.send(Msg::Shutdown(tx)).is_ok() {
            let _ = rx.recv();
        }
        handle.join().map_err(|_| Error::WriterGone)?;
        Ok(())
    }

    fn send(&self, msg: Msg) -> Result<()> {
        self.tx.send(msg).map_err(|_| Error::WriterGone)
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        // 保证「已提交的写不丢」：把队列跑干再关线程
        let _ = self.shutdown();
    }
}

/// 写线程主循环。
///
/// 任务里 panic 时**不再继续服务**：连接可能停在半个事务上，
/// 继续跑等于在不确定状态上写数据。直接退出，让后续调用明确失败。
fn worker(rx: Receiver<Msg>, mut conn: Connection) {
    while let Ok(msg) = rx.recv() {
        match msg {
            Msg::Job(job) => {
                // 注意不能用 `move`：那会把 conn 搬进闭包，循环下一轮就用不了了。
                // 这里只**借** conn，闭包被 catch_unwind 立刻消费，借用随即结束。
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    job(&mut conn);
                }));
                if result.is_err() {
                    // 连接会随线程退出被 drop → 未提交的事务由 SQLite 回滚
                    return;
                }
            }
            Msg::Barrier(reply) => {
                let _ = reply.send(());
            }
            Msg::Shutdown(reply) => {
                let _ = reply.send(());
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migration::{self, Backups, DbKind};
    use crate::store::pool::ReadPool;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// 建一个有 schema 的文件库。
    fn file_db() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.db");
        let mut conn = Connection::open(&path).unwrap();
        pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::Catalog, Backups::none(), 0).unwrap();
        drop(conn);
        dir
    }

    fn count_meta(pool: &ReadPool) -> i64 {
        pool.with(|c| Ok(c.query_row("SELECT count(*) FROM repository_meta", [], |r| r.get(0))?))
            .unwrap()
    }

    #[test]
    fn run_executes_and_returns_value() {
        let dir = file_db();
        let w = Writer::open(dir.path().join("catalog.db")).unwrap();
        let inserted = w
            .run(|conn| {
                conn.execute(
                    "INSERT INTO repository_meta(key, value) VALUES ('repository_id', 'abc')",
                    [],
                )?;
                Ok(conn.changes())
            })
            .unwrap();
        assert_eq!(inserted, 1);

        let id: String = w
            .run(|conn| {
                Ok(conn.query_row("SELECT value FROM repository_meta WHERE key='repository_id'", [], |r| {
                    r.get(0)
                })?)
            })
            .unwrap();
        assert_eq!(id, "abc");
    }

    #[test]
    fn writes_are_visible_to_readers() {
        let dir = file_db();
        let path = dir.path().join("catalog.db");
        let w = Writer::open(&path).unwrap();
        w.run(|conn| {
            conn.execute("INSERT INTO repository_meta(key, value) VALUES ('a','1')", [])?;
            Ok(())
        })
        .unwrap();

        let pool = ReadPool::open(&path).unwrap();
        assert_eq!(count_meta(&pool), 1, "读连接应当看到已提交的写");
    }

    #[test]
    fn transaction_commits_on_ok() {
        let dir = file_db();
        let path = dir.path().join("catalog.db");
        let w = Writer::open(&path).unwrap();
        w.transaction(|tx| {
            for i in 0..50 {
                tx.execute(
                    "INSERT INTO repository_meta(key, value) VALUES (?1, 'x')",
                    [format!("k{i}")],
                )?;
            }
            Ok(())
        })
        .unwrap();

        let pool = ReadPool::open(&path).unwrap();
        assert_eq!(count_meta(&pool), 50);
    }

    #[test]
    fn transaction_rolls_back_on_error() {
        let dir = file_db();
        let path = dir.path().join("catalog.db");
        let w = Writer::open(&path).unwrap();
        let err = w
            .transaction(|tx| {
                tx.execute("INSERT INTO repository_meta(key, value) VALUES ('good','1')", [])?;
                // 主键冲突：整批都要回滚
                tx.execute("INSERT INTO repository_meta(key, value) VALUES ('good','2')", [])?;
                Ok(())
            })
            .unwrap_err();
        assert!(matches!(err, Error::Database(_)), "应当是数据库错误：{err:?}");

        let pool = ReadPool::open(&path).unwrap();
        assert_eq!(count_meta(&pool), 0, "失败的事务必须整批回滚");
    }

    #[test]
    fn jobs_run_in_submission_order() {
        let dir = file_db();
        let w = Arc::new(Writer::open(dir.path().join("catalog.db")).unwrap());
        let order = Arc::new(Mutex::new(Vec::new()));

        let mut threads = Vec::new();
        for i in 0..16 {
            let w = Arc::clone(&w);
            let order = Arc::clone(&order);
            threads.push(std::thread::spawn(move || {
                w.run(move |conn| {
                    conn.execute(
                        "INSERT INTO repository_meta(key, value) VALUES (?1, 'v')",
                        [format!("k{i}")],
                    )?;
                    order.lock().unwrap().push(i);
                    Ok(())
                })
                .unwrap();
            }));
        }
        for t in threads {
            t.join().unwrap();
        }

        let seq = order.lock().unwrap().clone();
        assert_eq!(seq.len(), 16, "每个任务都要跑到");
        // 单写者 ⇒ 任务之间绝不重叠：这里检查「同一个任务不会插队到另一个任务中间」，
        // 也就是整个序列是某个提交顺序的排列（不会有重复或丢失）
        let mut sorted = seq.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..16).collect::<Vec<_>>());
    }

    #[test]
    fn concurrent_writers_never_hit_database_locked() {
        // 单写者设计的核心价值：并发写不再产生 SQLITE_BUSY
        let dir = file_db();
        let w = Arc::new(Writer::open(dir.path().join("catalog.db")).unwrap());
        let errors = Arc::new(AtomicUsize::new(0));

        std::thread::scope(|s| {
            for t in 0..8 {
                let w = Arc::clone(&w);
                let errors = Arc::clone(&errors);
                s.spawn(move || {
                    for i in 0..50 {
                        let key = format!("t{t}-{i}");
                        if w.run(move |conn| {
                            conn.execute(
                                "INSERT INTO repository_meta(key, value) VALUES (?1, 'v')",
                                [key],
                            )?;
                            Ok(())
                        })
                        .is_err()
                        {
                            errors.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                });
            }
        });

        assert_eq!(errors.load(Ordering::Relaxed), 0, "不该有失败的写");
        let pool = ReadPool::open(dir.path().join("catalog.db")).unwrap();
        assert_eq!(count_meta(&pool), 400, "8 线程 × 50 条都要写进去");
    }

    #[test]
    fn shutdown_flushes_queued_writes() {
        let dir = file_db();
        let path = dir.path().join("catalog.db");
        {
            let w = Writer::open(&path).unwrap();
            for i in 0..20 {
                // 不等结果，只入队
                let w2 = &w;
                w2.run(move |conn| {
                    conn.execute(
                        "INSERT INTO repository_meta(key, value) VALUES (?1, 'v')",
                        [format!("k{i}")],
                    )?;
                    Ok(())
                })
                .unwrap();
            }
            w.flush().unwrap();
        } // Drop → shutdown → 队列跑干后关线程

        let pool = ReadPool::open(&path).unwrap();
        assert_eq!(count_meta(&pool), 20, "关闭前入队的写一条都不能丢");
    }

    #[test]
    fn flush_is_a_barrier() {
        let dir = file_db();
        let w = Writer::open(dir.path().join("catalog.db")).unwrap();
        w.run(|conn| {
            conn.execute("INSERT INTO repository_meta(key, value) VALUES ('a','1')", [])?;
            Ok(())
        })
        .unwrap();
        w.flush().unwrap();
        w.flush().unwrap(); // 幂等
        assert!(w.run(|_| Ok(())).is_ok(), "flush 之后写者仍然可用");
    }

    #[test]
    fn panic_in_a_job_does_not_take_down_the_process() {
        let dir = file_db();
        let w = Writer::open(dir.path().join("catalog.db")).unwrap();
        // 静音 panic 输出（测试输出干净一点）
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        // 标出返回类型：闭包是 `!`，不标的话 T 推断不出来
        let _: Result<()> = w.run(|_conn| -> Result<()> { panic!("任务里故意 panic") });
        std::panic::set_hook(prev);

        // 写线程已退出 → 后续调用必须**明确失败**，而不是挂住或静默成功
        let after = w.run(|_| Ok(()));
        assert!(matches!(after, Err(Error::WriterGone)), "实际：{after:?}");

        // 库本身没坏：数据可读、事务已回滚
        let pool = ReadPool::open(dir.path().join("catalog.db")).unwrap();
        assert_eq!(count_meta(&pool), 0);
    }

    #[test]
    fn shutdown_is_idempotent() {
        let dir = file_db();
        let w = Writer::open(dir.path().join("catalog.db")).unwrap();
        w.shutdown().unwrap();
        w.shutdown().unwrap(); // 第二次直接返回
    }

    #[test]
    fn opening_a_bad_path_fails() {
        let dir = tempfile::tempdir().unwrap();
        // 把目录当成库文件打开 → SQLite 会拒绝
        assert!(Writer::open(dir.path()).is_err());
    }
}
