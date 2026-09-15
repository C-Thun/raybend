//! 缩略图 worker：消费 `app.db` 的 `jobs` 队列（`AGENTS.md` §6.4 的持久化任务队列）。
//!
//! # 为什么用任务队列而不是「当场算」
//!
//! 导入一万张照片时，缩略图是 CPU 大头。放进队列有三个好处：
//! 1. **导入不被拖住** —— 元数据先入库、网格立刻可用，缩略图随后逐步补齐；
//! 2. **崩溃可续** —— 任务是持久化的，重开软件接着跑；
//! 3. **可暂停/取消** —— 用户在进度弹窗里点一下就能停（`LIBRARY.md` §4.4）。
//!
//! # 一次任务的三个阶段（**刻意分成三次事务**）
//!
//! ```text
//! ① 认领（写 app.db）：pending → running，attempts += 1
//! ② 干活（不持有任何写锁）：读文件 → 渲染 → 写 thumbs.db
//! ③ 结账（写 app.db）：done / 失败则退回 pending，超过次数上限才算 failed
//! ```
//!
//! **绝不在 `app.db` 的写事务里做渲染** —— 那会把单写者堵住几秒钟，
//! 导入、评级、打标签全部卡住（`AGENTS.md` §6.4 的单写者纪律）。
//!
//! # 崩溃恢复
//!
//! 进程被杀时任务停在 `running`。启动时调 [`requeue_running`] 把它们退回 `pending`；
//! 因为「渲染一张缩略图」是**幂等**的，重复跑只是浪费一点 CPU，不会产生坏数据。

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::media::kind::{self, MediaKind};
use crate::media::scan::Cancel;
use crate::store::file_id::FileId;
use crate::store::path_semantics::PathForms;
use crate::thumbnail::cache::{self, ThumbsDb};
use crate::thumbnail::render::{self, SizeClass};

/// `jobs.kind` 里缩略图任务的取值。
pub const JOB_KIND: &str = "thumbnail";

/// 一条任务最多试几次（超过就标 failed，不再无限重试）。
pub const MAX_ATTEMPTS: i64 = 3;

/// 失败后的重试退避（毫秒）：退回 `pending` 时把 `updated_at` 设到未来，
/// `claim_next` 只认「`updated_at` 已到」的任务。
///
/// 没有这一步的话，一条**永久性失败**的任务（比如文件真的没了）会在同一轮里
/// 把重试次数一次烧完，还会让整批任务的进度条看着像卡住。
pub const RETRY_BACKOFF_MS: i64 = 30_000;

/// 缩略图任务的载荷（存 `jobs.payload` 的 JSON）。
///
/// **故意不带文件身份**：身份要在干活时现读（一个系统调用），放 JSON 里既要编解码
/// 又可能过期（文件被换掉时身份会变）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThumbJob {
    pub repository_id: String,
    /// 库内相对路径（原始大小写，`/` 分隔）。
    pub rel_path: String,
    /// 尺度名（[`SizeClass::as_str`]）。
    pub size: String,
}

impl ThumbJob {
    #[must_use]
    pub fn new(repository_id: &str, rel_path: &str, size: SizeClass) -> Self {
        Self {
            repository_id: repository_id.to_string(),
            rel_path: rel_path.to_string(),
            size: size.as_str().to_string(),
        }
    }

    /// 解析载荷；坏 JSON 返回 `None`（队列里的坏任务不该让 worker 崩）。
    #[must_use]
    pub fn parse(payload: &str) -> Option<Self> {
        serde_json::from_str(payload).ok()
    }
}

/// 从队列里认领到的一条任务。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claimed {
    pub job_id: i64,
    pub job: ThumbJob,
    pub attempts: i64,
}

/// 入队（同一条任务重复入队不做去重：渲染是幂等的，多跑一次只是浪费）。
pub fn enqueue(conn: &Connection, job: &ThumbJob, now_ms: i64) -> Result<i64> {
    let payload = serde_json::to_string(job)?;
    conn.execute(
        "INSERT INTO jobs (kind, payload, state, attempts, priority, created_at, updated_at)
         VALUES (?1, ?2, 'pending', 0, 0, ?3, ?3)",
        params![JOB_KIND, payload, now_ms],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 批量入队（一个事务里写多条，导入后一次性排上）。
pub fn enqueue_many(conn: &Connection, jobs: &[ThumbJob], now_ms: i64) -> Result<usize> {
    let mut n = 0;
    for job in jobs {
        enqueue(conn, job, now_ms)?;
        n += 1;
    }
    Ok(n)
}

/// 认领下一条待跑任务：`pending → running`，`attempts += 1`。
///
/// 用 `UPDATE ... RETURNING` 一条语句完成「挑 + 改」，避免自己拼两步。
pub fn claim_next(conn: &Connection, now_ms: i64) -> Result<Option<Claimed>> {
    let row: Option<(i64, String, i64)> = conn
        .query_row(
            "UPDATE jobs SET state = 'running', attempts = attempts + 1, updated_at = ?1
              WHERE id = (
                  SELECT id FROM jobs
                   WHERE kind = ?2 AND state = 'pending' AND updated_at <= ?1
                   ORDER BY priority DESC, id ASC LIMIT 1)
            RETURNING id, payload, attempts",
            params![now_ms, JOB_KIND],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;

    let Some((job_id, payload, attempts)) = row else {
        return Ok(None);
    };
    // 载荷坏了：直接结案（重试也没用），并留个原因给用户看
    let Some(job) = ThumbJob::parse(&payload) else {
        conn.execute(
            "UPDATE jobs SET state = 'failed', error = ?1, updated_at = ?2 WHERE id = ?3",
            params!["任务载荷不是合法 JSON", now_ms, job_id],
        )?;
        return claim_next(conn, now_ms); // 接着拿下一条
    };
    Ok(Some(Claimed {
        job_id,
        job,
        attempts,
    }))
}

/// 结账：成功。
pub fn complete(conn: &Connection, job_id: i64, now_ms: i64) -> Result<()> {
    conn.execute(
        "UPDATE jobs SET state = 'done', error = NULL, updated_at = ?1 WHERE id = ?2",
        params![now_ms, job_id],
    )?;
    Ok(())
}

/// 结账：失败。还有重试次数就退回 `pending`，否则标 `failed`。
///
/// 返回**是否还会重试**。
pub fn fail(conn: &Connection, job_id: i64, err: &str, now_ms: i64) -> Result<bool> {
    let attempts: Option<i64> = conn
        .query_row("SELECT attempts FROM jobs WHERE id = ?1", [job_id], |r| {
            r.get(0)
        })
        .optional()?;
    let Some(attempts) = attempts else {
        return Ok(false); // 任务已经不在了（被清理或取消）
    };
    let will_retry = attempts < MAX_ATTEMPTS;
    // 退避：退回 pending 时把「最早可重试时刻」写进 updated_at（兼作该语义）
    let due = if will_retry {
        now_ms + RETRY_BACKOFF_MS
    } else {
        now_ms
    };
    conn.execute(
        "UPDATE jobs SET state = ?1, error = ?2, updated_at = ?3 WHERE id = ?4",
        params![
            if will_retry { "pending" } else { "failed" },
            err,
            due,
            job_id
        ],
    )?;
    Ok(will_retry)
}

/// 把卡在 `running` 的任务退回 `pending`（**启动时调一次**，见模块文档）。
///
/// 返回退回了多少条。
pub fn requeue_running(conn: &Connection, now_ms: i64) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE jobs SET state = 'pending', updated_at = ?1
          WHERE kind = ?2 AND state = 'running'",
        params![now_ms, JOB_KIND],
    )?)
}

/// 队列里还有多少待跑 / 在跑 / 完成 / 失败。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct QueueStats {
    /// 待跑（含「退避中、还没到重试时刻」的）。
    pub pending: i64,
    pub running: i64,
    pub done: i64,
    pub failed: i64,
}

impl QueueStats {
    /// 还没结案的条数（进度条用）。
    #[must_use]
    pub fn outstanding(&self) -> i64 {
        self.pending + self.running
    }
}

pub fn queue_stats(conn: &Connection) -> Result<QueueStats> {
    let mut stmt =
        conn.prepare("SELECT state, count(*) FROM jobs WHERE kind = ?1 GROUP BY state")?;
    let mut out = QueueStats::default();
    let mut rows = stmt.query([JOB_KIND])?;
    while let Some(r) = rows.next()? {
        let state: String = r.get(0)?;
        let n: i64 = r.get(1)?;
        match state.as_str() {
            "pending" => out.pending = n,
            "running" => out.running = n,
            "done" => out.done = n,
            "failed" => out.failed = n,
            _ => {}
        }
    }
    Ok(out)
}

/// 一条任务干完的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// 新渲染出来并写进缓存。
    Rendered,
    /// 缓存已命中，什么都不用做。
    Cached,
    /// 文件不可解码（RAW）→ 存了占位图。
    Placeholder,
}

/// 一次 `run_pending` 的统计。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct RunStats {
    pub rendered: usize,
    pub cached: usize,
    pub placeholder: usize,
    pub failed: usize,
    pub cancelled: bool,
}

impl RunStats {
    #[must_use]
    pub fn processed(&self) -> usize {
        self.rendered + self.cached + self.placeholder + self.failed
    }
}

/// 缓存键的**唯一算法**（`produce` 与 `render_now` 共用，避免两处各算一遍算歪）。
///
/// `key_material` 是「这个文件在缓存里的身份字符串」：入库文件传**库内相对路径**，
/// 未入库的源文件传**绝对路径**（否则不同目录里的同名文件会撞键）。
#[must_use]
pub fn cache_key_for(abs_path: &Path, key_material: &str) -> Vec<u8> {
    let forms = PathForms::new(key_material);
    let identity = FileId::try_read(abs_path);
    cache::cache_key(identity.as_ref(), forms.folded())
}

/// 干一条活：读文件 → 渲染（或占位图）→ 写缓存。
///
/// **纯函数式的一次调用**：不碰 `jobs` 表，方便单测与复用（导入现场也可以直接调）。
///
/// ⚠️ 传进来的连接**必须可写**：`produce` 会把渲染结果写进 `thumbs`。
/// `ThumbsDb::read` 给的连接是 `query_only` 的，所以实际使用时要把 `produce`
/// 放进 `ThumbsDb::write` / `write_tx` 里跑（见 [`render_pending`]）。
#[allow(clippy::needless_pass_by_value)]
pub fn produce(conn: &Connection, root: &Path, job: &ThumbJob, now_ms: i64) -> Result<Outcome> {
    let Some(size) = SizeClass::parse(&job.size) else {
        return Err(crate::error::Error::Unsupported(format!(
            "未知的缩略图尺度：{}",
            job.size
        )));
    };
    let abs = root.join(&job.rel_path);
    let key = cache_key_for(&abs, &job.rel_path);
    let sig = render::render_sig(size);

    // 命中就不干活（渲染是这里最贵的部分）
    if cache::has(conn, &key, size, sig)? {
        return Ok(Outcome::Cached);
    }

    let file_name = job.rel_path.rsplit('/').next().unwrap_or(&job.rel_path);
    let kind = kind::kind_of_file(file_name);
    let (thumb, outcome) = match render::render_file(&abs, size)? {
        Some(t) => (t, Outcome::Rendered),
        None => {
            // 不可解码（RAW）：先用占位图兜住，之后有真实解码后端时再替换
            let p = render::placeholder(kind, size)?;
            (p, Outcome::Placeholder)
        }
    };

    cache::put(
        conn,
        &key,
        size,
        sig,
        &thumb.data,
        thumb.width,
        thumb.height,
        now_ms,
    )?;
    Ok(outcome)
}

/// 「**现在就给我一张缩略图**」：导入工作区滚动源目录时走这条路。
///
/// 与队列那条路的区别只有「谁来等」：这里同步等结果，队列是后台慢慢跑。
/// **缓存键与渲染逻辑完全共用** —— 同一个文件以后入库再生成缩略图时，直接命中这里写下的那条。
///
/// 调用方负责控制并发（前端只同时发 4 个请求）；这里不做限流，超时/取消也不属于它。
///
/// 文件不存在 / 读不了 → 报错（界面显示破图占位）；文件能读但解不了码（RAW）→ 占位图。
pub fn render_now(
    thumbs: &ThumbsDb,
    abs_path: &Path,
    size: SizeClass,
    now_ms: i64,
) -> Result<Vec<u8>> {
    // 源文件还没入库，「身份字符串」就是**绝对路径**（见 `cache_key_for`）
    let material = abs_path.to_string_lossy().into_owned();
    let key = cache_key_for(abs_path, &material);
    let sig = render::render_sig(size);

    // 读池的连接是 `query_only`，且闭包要 `Send + 'static` —— 键得自己持一份
    let read_key = key.clone();
    if let Some(bytes) = thumbs.read(move |conn| cache::get(conn, &read_key, size, sig))? {
        return Ok(bytes);
    }

    let file_name = abs_path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let kind = kind::kind_of_file(&file_name);
    let thumb = match render::render_file(abs_path, size)? {
        Some(t) => t,
        // 不可解码（RAW）：先用占位图兜住（与队列那条路同一取舍）
        None => render::placeholder(kind, size)?,
    };
    let (data, width, height) = (thumb.data, thumb.width, thumb.height);

    thumbs.write(move |conn| cache::put(conn, &key, size, sig, &data, width, height, now_ms))?;
    // 写进缓存的那份已由闭包持有，这里再取一次（一次 BLOB 读，微不足道）
    let read_key = abs_path.to_string_lossy().into_owned();
    let key = cache_key_for(abs_path, &read_key);
    thumbs
        .read(move |conn| cache::get(conn, &key, size, sig))?
        .ok_or_else(|| {
            crate::error::Error::Unsupported("缩略图刚写进缓存却读不回来".to_string())
        })
}

/// 一条「真的干过活」的任务的完整结果（给访问用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessReport {
    pub job_id: i64,
    pub outcome: Outcome,
    pub kind: MediaKind,
}

/// 跑一轮：反复「认领 → 干活 → 结账」直到队列空或取消。
///
/// `app_write` 是写 `app.db` 的入口（`AppDb::write` / `write_tx`），
/// `thumbs` 是缓存库。任务计数器 `max_jobs` 用来限制一次跑多少条（0 = 不限）。
pub fn run_pending<W>(
    app: &Connection,
    thumbs: &ThumbsDb,
    root: &Path,
    cancel: &Cancel,
    max_jobs: usize,
    now_ms: i64,
    mut app_write: W,
) -> Result<RunStats>
where
    W: FnMut(&Connection, i64, bool, &str, i64) -> Result<()>,
{
    let _ = thumbs;
    let mut stats = RunStats::default();
    let mut done = 0usize;

    loop {
        if cancel.is_cancelled() {
            stats.cancelled = true;
            break;
        }
        if max_jobs > 0 && done >= max_jobs {
            break; // 到了这一轮的限额
        }
        let Some(claimed) = claim_next(app, now_ms)? else {
            break; // 队列空了
        };
        done += 1;
        let job = claimed.job.clone();

        // ② 干活：**不持有 app.db 的写锁**（渲染是慢活），但写缓存要走 thumbs 的写者。
        //    （thumbs.db 是独立的库，堵它不影响 app.db 的写入。）
        //    闭包要送到写线程 → 根路径与任务都取自己的一份所有权。
        let root_owned = root.to_path_buf();
        let result = thumbs.write(move |conn| produce(conn, &root_owned, &job, now_ms));

        match result {
            Ok(outcome) => {
                match outcome {
                    Outcome::Rendered => stats.rendered += 1,
                    Outcome::Cached => stats.cached += 1,
                    Outcome::Placeholder => stats.placeholder += 1,
                }
                app_write(app, claimed.job_id, true, "", now_ms)?;
            }
            Err(e) => {
                stats.failed += 1;
                app_write(app, claimed.job_id, false, &e.to_string(), now_ms)?;
            }
        }
    }
    Ok(stats)
}

/// 便捷入口：把 `app_write` 适配成直接写 `app.db` 连接的闭包。
///
/// 返回值忽略即可；调用方通常包在 `AppDb::write_tx` 里跑。
pub fn apply_outcome(
    conn: &Connection,
    job_id: i64,
    ok: bool,
    err: &str,
    now_ms: i64,
) -> Result<()> {
    if ok {
        complete(conn, job_id, now_ms)
    } else {
        fail(conn, job_id, err, now_ms).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::db::{OpenOpts, migrate_file};
    use crate::store::migration::{self, Backups, DbKind};
    use crate::store::pragma;
    use image::{Rgb, RgbImage};

    const T0: i64 = 1_789_516_800_000;

    fn app() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::App, Backups::none(), T0).unwrap();
        conn
    }

    fn cache_conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::Thumbs, Backups::none(), T0).unwrap();
        conn
    }

    fn write_jpeg(path: &Path, w: u32, h: u32) {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).unwrap();
        }
        let img = RgbImage::from_pixel(w, h, Rgb([120, 80, 40]));
        let mut buf = Vec::new();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 90);
        enc.encode_image(&img).unwrap();
        std::fs::write(path, buf).unwrap();
    }

    fn thumbs_db(dir: &Path) -> ThumbsDb {
        ThumbsDb::open(dir.join("cache"), T0).unwrap()
    }

    /// 某条缓存被写入/刷新时记下的时间（测试用；取不到则 panic）。
    fn last_used_at(thumbs: &ThumbsDb, key: &[u8], size: &str) -> i64 {
        let key = key.to_vec();
        let size = size.to_string();
        thumbs
            .read(move |conn| {
                Ok(conn.query_row(
                    "SELECT last_used_at FROM thumbs WHERE cache_key = ?1 AND size_class = ?2",
                    params![&key, &size],
                    |r| r.get::<_, i64>(0),
                )?)
            })
            .unwrap()
    }

    // ---------- 载荷 ----------

    #[test]
    fn job_payload_roundtrip_and_garbage() {
        let job = ThumbJob::new("repo123", "photos/2026/a.jpg", SizeClass::Grid);
        let json = serde_json::to_string(&job).unwrap();
        assert_eq!(ThumbJob::parse(&json).unwrap(), job);
        assert!(ThumbJob::parse("").is_none());
        assert!(ThumbJob::parse("{}").is_none(), "缺字段的载荷要判为坏");
        assert!(ThumbJob::parse("not json").is_none());
        assert_eq!(job.size, "grid");
    }

    // ---------- 队列 ----------

    #[test]
    fn enqueue_claim_complete() {
        let conn = app();
        let job = ThumbJob::new("r", "a.jpg", SizeClass::Grid);
        let id = enqueue(&conn, &job, T0).unwrap();
        assert_eq!(queue_stats(&conn).unwrap().pending, 1);

        let claimed = claim_next(&conn, T0 + 1).unwrap().unwrap();
        assert_eq!(claimed.job_id, id);
        assert_eq!(claimed.attempts, 1, "认领时 attempts 加一");
        assert_eq!(queue_stats(&conn).unwrap().running, 1);

        complete(&conn, id, T0 + 2).unwrap();
        let s = queue_stats(&conn).unwrap();
        assert_eq!((s.done, s.pending, s.running), (1, 0, 0));
        assert_eq!(claim_next(&conn, T0 + 3).unwrap(), None, "没有任务了");
    }

    #[test]
    fn claim_picks_highest_priority_then_lowest_id() {
        let conn = app();
        let a = enqueue(&conn, &ThumbJob::new("r", "a.jpg", SizeClass::Grid), T0).unwrap();
        let b = enqueue(&conn, &ThumbJob::new("r", "b.jpg", SizeClass::Grid), T0).unwrap();
        conn.execute("UPDATE jobs SET priority = 5 WHERE id = ?1", [b])
            .unwrap();
        assert_eq!(
            claim_next(&conn, T0).unwrap().unwrap().job_id,
            b,
            "优先级高的先跑"
        );
        assert_eq!(
            claim_next(&conn, T0).unwrap().unwrap().job_id,
            a,
            "同优先级按入队顺序"
        );
    }

    #[test]
    fn fail_retries_then_gives_up() {
        let conn = app();
        let id = enqueue(&conn, &ThumbJob::new("r", "a.jpg", SizeClass::Grid), T0).unwrap();

        let mut clock = T0;
        for attempt in 1..=MAX_ATTEMPTS {
            let claimed = claim_next(&conn, clock).unwrap().unwrap();
            assert_eq!(claimed.attempts, attempt);
            let will_retry = fail(&conn, id, "文件读不了", clock).unwrap();
            if will_retry {
                // 退避期间不该被再认领
                assert_eq!(
                    claim_next(&conn, clock).unwrap(),
                    None,
                    "退避期内不应重试（否则会在一轮里烧完重试次数）"
                );
                clock += RETRY_BACKOFF_MS; // 等到可重试时刻
            }
            assert_eq!(
                will_retry,
                attempt < MAX_ATTEMPTS,
                "第 {attempt} 次失败后的重试判断不对"
            );
        }
        let s = queue_stats(&conn).unwrap();
        assert_eq!((s.failed, s.pending), (1, 0), "超过上限就不该再排队");
        let err: String = conn
            .query_row("SELECT error FROM jobs WHERE id = ?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(err, "文件读不了", "要给用户留原因");
    }

    #[test]
    fn requeue_running_is_the_crash_recovery() {
        let conn = app();
        let id = enqueue(&conn, &ThumbJob::new("r", "a.jpg", SizeClass::Grid), T0).unwrap();
        // 模拟「进程被杀」：任务停在 running
        claim_next(&conn, T0).unwrap();
        assert_eq!(queue_stats(&conn).unwrap().running, 1);

        assert_eq!(requeue_running(&conn, T0 + 5).unwrap(), 1);
        let s = queue_stats(&conn).unwrap();
        assert_eq!((s.pending, s.running), (1, 0));
        let claimed = claim_next(&conn, T0 + 6).unwrap().unwrap();
        assert_eq!(claimed.job_id, id);
        assert_eq!(claimed.attempts, 2, "重跑要算一次尝试");
    }

    #[test]
    fn corrupt_payload_is_failed_not_retried_forever() {
        let conn = app();
        conn.execute(
            "INSERT INTO jobs (kind, payload, state, attempts, priority, created_at, updated_at)
             VALUES (?1, '{{{', 'pending', 0, 0, ?2, ?2)",
            params![JOB_KIND, T0],
        )
        .unwrap();
        let good = enqueue(&conn, &ThumbJob::new("r", "a.jpg", SizeClass::Grid), T0).unwrap();

        // 坏任务被跳过并标 failed，好任务照常被认领
        let claimed = claim_next(&conn, T0 + 1).unwrap().unwrap();
        assert_eq!(claimed.job_id, good);
        let s = queue_stats(&conn).unwrap();
        assert_eq!(s.failed, 1, "坏载荷直接判失败，不无限重试");
    }

    #[test]
    fn other_job_kinds_are_left_alone() {
        let conn = app();
        conn.execute(
            "INSERT INTO jobs (kind, payload, state, attempts, priority, created_at, updated_at)
             VALUES ('import', '{}', 'pending', 0, 0, ?1, ?1)",
            [T0],
        )
        .unwrap();
        assert_eq!(
            claim_next(&conn, T0).unwrap(),
            None,
            "别的工种的活不该被缩略图 worker 抢"
        );
        assert_eq!(requeue_running(&conn, T0).unwrap(), 0);
    }

    // ---------- 干活 ----------

    #[test]
    fn produce_renders_then_hits_cache() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("lib");
        write_jpeg(&root.join("photos/a.jpg"), 1200, 800);
        let conn = cache_conn();
        let job = ThumbJob::new("r", "photos/a.jpg", SizeClass::Grid);

        assert_eq!(produce(&conn, &root, &job, T0).unwrap(), Outcome::Rendered);
        let s = cache::stats(&conn).unwrap();
        assert_eq!(s.entries, 1);

        // 第二次：命中（不重复渲染）
        assert_eq!(
            produce(&conn, &root, &job, T0 + 1).unwrap(),
            Outcome::Cached
        );
        assert_eq!(cache::stats(&conn).unwrap().entries, 1);
    }

    #[test]
    fn produce_makes_a_placeholder_for_raw() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("lib");
        std::fs::create_dir_all(root.join("photos")).unwrap();
        // 假 RAW 字节（II*\0 开头，但内容不足以解码）
        std::fs::write(
            root.join("photos/a.rw2"),
            [0x49, 0x49, 0x2a, 0x00, 0, 0, 0, 0],
        )
        .unwrap();

        let conn = cache_conn();
        let job = ThumbJob::new("r", "photos/a.rw2", SizeClass::Grid);
        assert_eq!(
            produce(&conn, &root, &job, T0).unwrap(),
            Outcome::Placeholder
        );

        let key = cache::cache_key(
            FileId::try_read(root.join("photos/a.rw2")).as_ref(),
            "photos/a.rw2",
        );
        let bytes = cache::get(
            &conn,
            &key,
            SizeClass::Grid,
            render::render_sig(SizeClass::Grid),
        )
        .unwrap()
        .unwrap();
        assert!(
            render::is_valid_jpeg(&bytes),
            "占位图也该是合法 JPEG，UI 不分情况"
        );
    }

    #[test]
    fn produce_reports_missing_file_as_error() {
        let dir = tempfile::tempdir().unwrap();
        let conn = cache_conn();
        let job = ThumbJob::new("r", "photos/gone.jpg", SizeClass::Grid);
        let err = produce(&conn, dir.path(), &job, T0);
        assert!(err.is_err(), "文件没了要报错（好让任务重试/标失败）");
    }

    #[test]
    fn produce_rejects_unknown_size_class() {
        let dir = tempfile::tempdir().unwrap();
        let conn = cache_conn();
        let job = ThumbJob::new("r", "a.jpg", SizeClass::Grid);
        let bad = ThumbJob {
            size: "screen".into(),
            ..job
        };
        assert!(produce(&conn, dir.path(), &bad, T0).is_err());
    }

    #[test]
    fn both_size_classes_are_independent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("lib");
        write_jpeg(&root.join("a.jpg"), 1000, 1000);
        let conn = cache_conn();

        produce(
            &conn,
            &root,
            &ThumbJob::new("r", "a.jpg", SizeClass::Grid),
            T0,
        )
        .unwrap();
        produce(
            &conn,
            &root,
            &ThumbJob::new("r", "a.jpg", SizeClass::Strip),
            T0,
        )
        .unwrap();
        let s = cache::stats(&conn).unwrap();
        assert_eq!(s.entries, 2, "两个尺度各自一条");
        assert_eq!(
            s.by_size,
            vec![
                ("grid".to_string(), 1, s.by_size[0].2),
                ("strip".to_string(), 1, s.by_size[1].2)
            ]
            .into_iter()
            .map(|(a, b, _)| (a, b, 0))
            .collect::<Vec<_>>()
            .into_iter()
            .zip(s.by_size.iter())
            .map(|((a, b, _), (a2, b2, c2))| {
                assert_eq!((&a, b), (a2, *b2));
                (a2.clone(), *b2, *c2)
            })
            .collect::<Vec<_>>()
        );
    }

    // ---------- 跑一轮 ----------

    #[test]
    fn run_pending_processes_everything_and_stops() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("lib");
        for i in 0..5 {
            write_jpeg(&root.join(format!("photos/{i}.jpg")), 800, 600);
        }
        let thumbs = thumbs_db(dir.path());
        let app_conn = app();
        let mut jobs = Vec::new();
        for i in 0..5 {
            jobs.push(ThumbJob::new(
                "r",
                &format!("photos/{i}.jpg"),
                SizeClass::Grid,
            ));
        }
        enqueue_many(&app_conn, &jobs, T0).unwrap();

        let apply = |conn: &Connection, id: i64, ok: bool, err: &str, now: i64| {
            apply_outcome(conn, id, ok, err, now)
        };
        let stats = run_pending(
            &app_conn,
            &thumbs,
            &root,
            &Cancel::new(),
            0,
            T0,
            |conn, id, ok, err, now| apply(conn, id, ok, err, now),
        )
        .unwrap();
        assert_eq!(stats.rendered, 5);
        assert_eq!(stats.failed, 0);
        assert_eq!(queue_stats(&app_conn).unwrap().done, 5);
        assert_eq!(thumbs.read(cache::stats).unwrap().entries, 5);

        // 再跑一轮：队列已空，什么都不做
        let again = run_pending(
            &app_conn,
            &thumbs,
            &root,
            &Cancel::new(),
            0,
            T0 + 1,
            |conn, id, ok, err, now| apply(conn, id, ok, err, now),
        )
        .unwrap();
        assert_eq!(again.processed(), 0);
    }

    #[test]
    fn run_pending_counts_failures_and_keeps_going() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("lib");
        write_jpeg(&root.join("ok.jpg"), 400, 300);
        let thumbs = thumbs_db(dir.path());
        let app_conn = app();
        enqueue(
            &app_conn,
            &ThumbJob::new("r", "ok.jpg", SizeClass::Grid),
            T0,
        )
        .unwrap();
        enqueue(
            &app_conn,
            &ThumbJob::new("r", "gone.jpg", SizeClass::Grid),
            T0,
        )
        .unwrap();

        let stats = run_pending(
            &app_conn,
            &thumbs,
            &root,
            &Cancel::new(),
            0,
            T0,
            apply_outcome,
        )
        .unwrap();
        assert_eq!(
            (stats.rendered, stats.failed),
            (1, 1),
            "一条失败不拖垮另一条"
        );
        let s = queue_stats(&app_conn).unwrap();
        assert_eq!(s.done, 1);
        assert_eq!(s.pending, 1, "失败的那条退回 pending 等重试");
    }

    #[test]
    fn run_pending_respects_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("lib");
        for i in 0..4 {
            write_jpeg(&root.join(format!("{i}.jpg")), 300, 200);
        }
        let thumbs = thumbs_db(dir.path());
        let app_conn = app();
        for i in 0..4 {
            enqueue(
                &app_conn,
                &ThumbJob::new("r", &format!("{i}.jpg"), SizeClass::Grid),
                T0,
            )
            .unwrap();
        }
        let cancel = Cancel::new();
        cancel.cancel();

        let stats = run_pending(&app_conn, &thumbs, &root, &cancel, 0, T0, apply_outcome).unwrap();
        assert!(stats.cancelled);
        assert_eq!(stats.processed(), 0, "一开始就取消 → 一条都不跑");
        assert_eq!(queue_stats(&app_conn).unwrap().pending, 4, "任务还在队列里");
    }

    #[test]
    fn run_pending_honours_max_jobs_budget() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("lib");
        for i in 0..3 {
            write_jpeg(&root.join(format!("{i}.jpg")), 200, 150);
        }
        let thumbs = thumbs_db(dir.path());
        let app_conn = app();
        for i in 0..3 {
            enqueue(
                &app_conn,
                &ThumbJob::new("r", &format!("{i}.jpg"), SizeClass::Grid),
                T0,
            )
            .unwrap();
        }
        let stats = run_pending(
            &app_conn,
            &thumbs,
            &root,
            &Cancel::new(),
            2,
            T0,
            apply_outcome,
        )
        .unwrap();
        assert_eq!(stats.processed(), 2, "限额生效");
        assert_eq!(queue_stats(&app_conn).unwrap().pending, 1);
    }

    #[test]
    fn produce_via_the_real_thumbs_db_writer() {
        // 真实路径：ThumbsDb（文件 + 读池 + 写者）而不是内存连接
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("lib");
        write_jpeg(&root.join("a.jpg"), 500, 400);
        let thumbs = ThumbsDb::open(dir.path().join("cache"), T0).unwrap();
        let job = ThumbJob::new("r", "a.jpg", SizeClass::Grid);
        let out = thumbs
            .write(move |conn| produce(conn, &root, &job, T0))
            .unwrap();
        assert_eq!(out, Outcome::Rendered);
        assert_eq!(thumbs.read(cache::stats).unwrap().entries, 1);
    }

    #[test]
    fn migrator_accepts_a_real_thumbs_file() {
        // 走一遍真实文件路径（不是内存库）：确认迁移 + 写者 + 读池三件套在磁盘上能用
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("thumbs.db");
        migrate_file(&path, DbKind::Thumbs, Backups::none(), T0, true).unwrap();
        assert!(path.is_file());

        let thumbs = ThumbsDb::open(dir.path(), T0).unwrap();
        let _ = OpenOpts::new(None, T0); // 顺手确认门面类型可用
        assert_eq!(
            thumbs
                .read(crate::store::migration::schema_version)
                .unwrap(),
            migration::supported_version(DbKind::Thumbs)
        );
    }

    // ---------- render_now（导入工作区的滚动加载）----------

    #[test]
    fn render_now_returns_bytes_and_then_hits_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("源");
        write_jpeg(&photo.join("a.jpg"), 600, 400);
        let thumbs = thumbs_db(dir.path());

        let abs = photo.join("a.jpg");
        let first = render_now(&thumbs, &abs, SizeClass::Grid, T0).unwrap();
        assert!(render::is_valid_jpeg(&first), "应当是一张真 JPEG");

        // 记下写入时间：第二次若又走渲染，`put` 会把 last_used_at 推到新值
        let key = cache_key_for(&abs, &abs.to_string_lossy());
        let after_first = last_used_at(&thumbs, &key, "grid");

        let second = render_now(&thumbs, &abs, SizeClass::Grid, T0 + 999).unwrap();
        assert_eq!(first, second, "同样的输入要拿到同样的字节");
        assert_eq!(
            last_used_at(&thumbs, &key, "grid"),
            after_first,
            "第二次必须命中缓存，不再重写"
        );
    }

    #[test]
    fn render_now_uses_a_placeholder_for_undecodable_files() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("源");
        std::fs::create_dir_all(&photo).unwrap();
        // RW2 在本阶段就是「读得出来但解不了码」那一类
        std::fs::write(photo.join("a.rw2"), vec![1u8, 2, 3, 4, 5]).unwrap();
        let thumbs = thumbs_db(dir.path());

        let bytes = render_now(&thumbs, &photo.join("a.rw2"), SizeClass::Strip, T0).unwrap();
        assert!(render::is_valid_jpeg(&bytes), "占位图也是合法 JPEG");
        assert_eq!(thumbs.read(cache::stats).unwrap().entries, 1, "占位图也入缓存");
    }

    #[test]
    fn render_now_reports_a_missing_file_instead_of_faking_an_image() {
        let dir = tempfile::tempdir().unwrap();
        let thumbs = thumbs_db(dir.path());
        let missing = dir.path().join("gone.jpg");
        assert!(render_now(&thumbs, &missing, SizeClass::Grid, T0).is_err());
    }

    #[test]
    fn render_now_keeps_different_directories_apart_without_identity() {
        // 两个目录里的**同名**文件必须各占一条缓存 ——
        // 否则（读不到文件身份时）缓存会把它们当成同一张图
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        write_jpeg(&a.join("same.jpg"), 100, 80);
        write_jpeg(&b.join("same.jpg"), 300, 240);
        let thumbs = thumbs_db(dir.path());

        let a_bytes = render_now(&thumbs, &a.join("same.jpg"), SizeClass::Grid, T0).unwrap();
        let b_bytes = render_now(&thumbs, &b.join("same.jpg"), SizeClass::Grid, T0).unwrap();
        assert_ne!(a_bytes, b_bytes, "不同目录的同名文件不能撞缓存");
    }

    #[test]
    fn render_now_respects_size_classes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("big.jpg");
        write_jpeg(&file, 1200, 800);
        let thumbs = thumbs_db(dir.path());

        render_now(&thumbs, &file, SizeClass::Grid, T0).unwrap();
        render_now(&thumbs, &file, SizeClass::Strip, T0).unwrap();
        assert_eq!(
            thumbs.read(cache::stats).unwrap().entries,
            2,
            "两个尺度各存一条"
        );
    }
}
