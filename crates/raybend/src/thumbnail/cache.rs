//! 缩略图缓存：`<库根>/cache/thumbs.db`（跟着库走，与 `cache/full/` 同级）。
//!
//! # 位置与形态（`AGENTS.md` §6.5）
//!
//! * 缓存**在库根的 `cache/` 里** —— 换机器/换盘只要库在，缩图就在；删掉它只降级、可重建。
//! * 用**独立的小 SQLite 库**而不是散文件：一次事务写一张、读一张很快，
//!   而且 GC 就是几条 SQL（不用扫目录、不用管目录分片）。15 万张的规模由
//!   每库 2 GiB 维护阈值 + 写时清兄弟签名 + LRU 兜住（`specs/thumbnail-cache-per-library.md`）。
//! * 它是**派生数据**：删掉整个目录 → 功能降级但完全可用。**不做快照备份。**
//!
//! # 缓存键
//!
//! `(cache_key, size_class, render_sig)`：
//!
//! * `cache_key` **优先用文件身份**（`volume_serial` + `file_id`）—— 文件改名、移动后
//!   缓存仍然命中；读不到身份（网络盘等）才退回**折叠路径**。首字节 `b'i'` / `b'p'`
//!   做判别，两种键不可能撞。
//! * `render_sig` 含管线版本（见 [`crate::thumbnail::render::render_sig`]）：
//!   算法一改，旧缓存自动成孤儿并被 GC 收走，**不需要写数据迁移**。
//!
//! # 回收（frecency）
//!
//! 「最近用过（recency）+ 用得多（frequency）」加权，而不是纯 LRU：
//! 一次性翻过的大量照片不该把常用的那几张挤掉。淘汰顺序
//! `pinned` → `hits` → `last_used_at`，`pinned = 1` 的永不淘汰。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{Error, Result};
use crate::store::db::migrate_file;
use crate::store::file_id::FileId;
use crate::store::migration::{Backups, DbKind};
use crate::store::pool::ReadPool;
use crate::store::writer::Writer;
use crate::thumbnail::render::SizeClass;

/// 缓存库文件名。
pub const THUMBS_FILE_NAME: &str = "thumbs.db";

/// 每库小图缓存维护阈值：2 GiB（崔总 2026-10-25 定的预期量级，不是硬限制）。
///
/// 按 384 档 AVIF q80 实测 12.75 KB/张，15 万张约 1.8 GiB；超过阈值时按
/// frecency 淘汰最久未用的条目，浏览时按需重建。单库达到 15 万张后界面会
/// 提示建议开新库，而不是在 2 GiB 处阻断。
pub const DEFAULT_MAX_BYTES: i64 = 2 << 30;

/// 自动维护的间隔（写缓存后触发，但最多这么久跑一次）。
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// 算一个缓存键。
///
/// * 有身份 → `b'i' || volume_serial(8, LE) || file_id(16)`（25 字节）
/// * 没有 → `b'p' || 折叠路径的 UTF-8 字节`（可变长）
#[must_use]
pub fn cache_key(identity: Option<&FileId>, rel_path_folded: &str) -> Vec<u8> {
    match identity.filter(|id| !id.is_zero()) {
        Some(id) => {
            let mut key = Vec::with_capacity(25);
            key.push(b'i');
            key.extend_from_slice(&id.volume_serial.to_le_bytes());
            key.extend_from_slice(&id.file_id);
            key
        }
        None => {
            let mut key = Vec::with_capacity(rel_path_folded.len() + 1);
            key.push(b'p');
            key.extend_from_slice(rel_path_folded.as_bytes());
            key
        }
    }
}

/// 一个缓存条目的元信息（不含图像数据）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ThumbMeta {
    pub width: u32,
    pub height: u32,
    pub bytes: u32,
    pub created_at: i64,
    pub last_used_at: i64,
    pub hits: i64,
    pub pinned: bool,
}

/// 缓存的总体统计（给设置里的「缓存」面板用）。
///
/// `rename_all = "camelCase"`：它直接从 Tauri 命令返回给前端，
/// 若不加就会序列化成 `by_size` —— 而前端镜像里写的是 `bySize`。
/// （这个问题真被契约测试抓到过一次，见 `src-tauri/src/contract.rs`。）
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    pub entries: i64,
    pub bytes: i64,
    pub pinned: i64,
    /// 每个尺度的 (条目数, 字节数)，按尺度名排序。
    pub by_size: Vec<(String, i64, i64)>,
}

/// 一次 GC 的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct GcOutcome {
    pub removed: i64,
    pub bytes_freed: i64,
    /// 淘汰后剩余的总字节数。
    pub remaining_bytes: i64,
}

/// 缓存库的门面（读池 + 单写者，与 `CatalogDb` 同一套模式）。
pub struct ThumbsDb {
    pool: ReadPool,
    writer: Writer,
    path: PathBuf,
    /// 上次维护时间（进程内节流；不落库）。
    last_maintenance: Mutex<Option<Instant>>,
    /// 命中后待批量刷新的 frecency（读路径不能写，先攒着）。
    touch_queue: Mutex<TouchState>,
}

/// 读命中攒下来的「用过」记录：同一行只留一条，30 秒或 200 条刷一次。
#[derive(Default)]
struct TouchState {
    pending: HashMap<(Vec<u8>, String, String), i64>,
    last_flush: Option<Instant>,
}

impl ThumbsDb {
    /// 打开（必要时创建）缓存库。`dir` 是**该库的缓存目录**（`<库根>/cache/`），
    /// 文件固定叫 [`THUMBS_FILE_NAME`]。
    pub fn open(dir: impl AsRef<Path>, now_ms: i64) -> Result<Self> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir)?;
        let path = dir.join(THUMBS_FILE_NAME);

        migrate_file(&path, DbKind::Thumbs, Backups::none(), now_ms, true)?;

        let pool = ReadPool::open(&path)?;
        let writer = Writer::open(&path)?;
        Ok(Self {
            pool,
            writer,
            path,
            last_maintenance: Mutex::new(None),
            touch_queue: Mutex::new(TouchState::default()),
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 读（走连接池）。
    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        self.pool.with(f)
    }

    /// 写一条（单写者）。闭包会被送到写线程，所以要 `Send + 'static`。
    pub fn write<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    {
        self.writer.run(f)
    }

    /// 写一个事务（批量）。
    pub fn write_tx<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&rusqlite::Transaction<'_>) -> Result<T> + Send + 'static,
    {
        self.writer.transaction(f)
    }

    /// 有节制地跑一次维护：旧签名孤儿回收 + 容量回收。
    ///
    /// 调用方在**写缓存之后**顺手调一下即可；内部按 [`MAINTENANCE_INTERVAL`] 节流，
    /// 所以不会每次写都扫全表。返回 `None` = 这次没到时间。
    ///
    /// # Errors
    /// 数据库读写失败。
    pub fn maybe_maintain(&self, max_bytes: i64, now_ms: i64) -> Result<Option<(GcOutcome, usize)>> {
        let mut last = self
            .last_maintenance
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if last.is_some_and(|at| at.elapsed() < MAINTENANCE_INTERVAL) {
            return Ok(None);
        }
        *last = Some(Instant::now());
        drop(last);
        /*
         * **一次事务**跑完整维护：`gc` 一次最多删 5 万行，逐条自动提交会变成
         * 5 万次提交（慢盘上可能几秒），而它占用的是写线程 —— 期间所有缩图写入
         * 都会排队。包成一笔事务后只有一次提交成本。
         */
        self.write_tx(move |tx| maintenance(tx, max_bytes, now_ms))
            .map(Some)
    }

    /// 记一次读命中（frecency 的原料）。
    ///
    /// 读连接是 `query_only`，不能直接写；这里只把「这一行被用过」攒进内存，
    /// 每 30 秒或 200 条批量刷一次。同一行只留一条，刷的时候还带
    /// `last_used_at < now - 1h` 的条件 —— 一小时内反复看同一张只写一次。
    pub fn note_use(&self, key: &[u8], size: SizeClass, sig: &str, now_ms: i64) {
        let flush = {
            let mut state = match self.touch_queue.lock() {
                Ok(state) => state,
                Err(_) => return,
            };
            state
                .pending
                .insert((key.to_vec(), size.as_str().to_string(), sig.to_string()), now_ms);
            state.pending.len() >= 200
                || state
                    .last_flush
                    .is_none_or(|at| at.elapsed() >= Duration::from_secs(30))
        };
        if flush {
            self.flush_touches(now_ms);
        }
    }

    /// 把攒下的 touch 一次性写掉（失败只记日志：frecency 坏掉不该影响看图）。
    pub fn flush_touches(&self, now_ms: i64) {
        let pending = {
            let mut state = match self.touch_queue.lock() {
                Ok(state) => state,
                Err(_) => return,
            };
            state.last_flush = Some(Instant::now());
            std::mem::take(&mut state.pending)
        };
        if pending.is_empty() {
            return;
        }
        let cutoff = now_ms.saturating_sub(60 * 60 * 1000);
        let result = self.write_tx(move |tx| {
            for ((key, size, sig), used_at) in pending {
                tx.execute(
                    "UPDATE thumbs SET hits = hits + 1, last_used_at = ?1
                      WHERE cache_key = ?2 AND size_class = ?3 AND render_sig = ?4
                        AND last_used_at < ?5",
                    params![used_at, key, size, sig, cutoff],
                )?;
            }
            Ok(())
        });
        if let Err(error) = result {
            log::warn!(target: "thumbs", "frecency 批量刷新失败（不影响看图）：{error}");
        }
    }

    /// 把队列跑干（`Drop` 时也会跑，这里是显式版）。
    pub fn flush(&self) -> Result<()> {
        self.writer.flush()
    }
}

/// 写一条缩略图（同键覆盖）。
///
/// 参数多是有意的：它是**缓存最底层的一次写**，调用点都在 worker / 导入流程里，
/// 包成结构体只会让调用处更啰嗦（而且这些字段本来就一起出现）。
#[allow(clippy::too_many_arguments)]
pub fn put(
    conn: &Connection,
    key: &[u8],
    size: SizeClass,
    sig: &str,
    data: &[u8],
    width: u32,
    height: u32,
    now_ms: i64,
) -> Result<()> {
    if !matches!(size, SizeClass::Grid) {
        return Err(Error::Unsupported(format!(
            "thumbs.db 只存 384 小图，收到 {}",
            size.as_str()
        )));
    }
    conn.execute(
        "INSERT INTO thumbs
            (cache_key, size_class, render_sig, width, height, bytes, data,
             created_at, last_used_at, hits, pinned)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 0, 0)
         ON CONFLICT(cache_key, size_class, render_sig) DO UPDATE SET
            width = excluded.width, height = excluded.height, bytes = excluded.bytes,
            data = excluded.data, last_used_at = excluded.last_used_at",
        params![
            key,
            size.as_str(),
            sig,
            width,
            height,
            data.len() as i64,
            data,
            now_ms
        ],
    )?;
    /*
     * 同一个 (文件, 尺度) 只有**最新**那一份是活的：
     * * 编辑确认后旧 `+e<指纹>` 立即作废；
     * * 重置编辑后 base 又成为活的那份。
     * 不在这里删，编辑历史会一直攒到容量 GC（15 万张的 2 GiB 预算会被吃掉）。
     * `pinned` 的条目不动（用户明确要求留着的东西）。
     */
    conn.execute(
        "DELETE FROM thumbs
          WHERE cache_key = ?1 AND size_class = ?2 AND render_sig <> ?3 AND pinned = 0",
        params![key, size.as_str(), sig],
    )?;
    Ok(())
}

/// 取一条缩略图。
///
/// **取不到就是 `None`，不是错误** —— 缓存未命中是常态。
///
/// ⚠️ **这是纯读**：不更新 frecency 计数。原因有两条：
/// 1. 读连接是 `query_only` 的（`store::pool` 的纪律），在这里写会直接报错；
/// 2. 「看一眼就写一次数据库」也太重了 —— 浏览时一秒能滚过几十张。
///    frecency 由 [`touch`] 单独更新，UI 侧按批（或退出时）刷一次即可。
pub fn get(conn: &Connection, key: &[u8], size: SizeClass, sig: &str) -> Result<Option<Vec<u8>>> {
    Ok(conn
        .query_row(
            "SELECT data FROM thumbs
              WHERE cache_key = ?1 AND size_class = ?2 AND render_sig = ?3",
            params![key, size.as_str(), sig],
            |r| r.get(0),
        )
        .optional()?)
}

/// 取一条缩略图**连同尺寸**（一次查询）。
///
/// 给「统一取图口」的缓存路径用：命中时要把尺寸一起报回 `DisplayImage.size`，
/// 分两次查（`get` + `meta`）没必要。其余语义与 [`get`] 完全相同（纯读、不碰 frecency）。
pub fn get_with_size(
    conn: &Connection,
    key: &[u8],
    size: SizeClass,
    sig: &str,
) -> Result<Option<(Vec<u8>, u32, u32)>> {
    Ok(conn
        .query_row(
            "SELECT data, width, height FROM thumbs
              WHERE cache_key = ?1 AND size_class = ?2 AND render_sig = ?3",
            params![key, size.as_str(), sig],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?)
}

/// 记一次「用过」（frecency 的原料）：`hits += 1`、`last_used_at = now`。
///
/// 批量调用更划算（一次事务里 `touch` 一批），见模块文档的回收策略。
pub fn touch(conn: &Connection, key: &[u8], size: SizeClass, sig: &str, now_ms: i64) -> Result<()> {
    conn.execute(
        "UPDATE thumbs SET hits = hits + 1, last_used_at = ?1
          WHERE cache_key = ?2 AND size_class = ?3 AND render_sig = ?4",
        params![now_ms, key, size.as_str(), sig],
    )?;
    Ok(())
}

/// 不看数据，只问「有没有」（用于「已缓存 N/M」这类进度显示）。
pub fn has(conn: &Connection, key: &[u8], size: SizeClass, sig: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM thumbs
          WHERE cache_key = ?1 AND size_class = ?2 AND render_sig = ?3",
        params![key, size.as_str(), sig],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// 取元信息（不含数据）。
pub fn meta(
    conn: &Connection,
    key: &[u8],
    size: SizeClass,
    sig: &str,
) -> Result<Option<ThumbMeta>> {
    Ok(conn
        .query_row(
            "SELECT width, height, bytes, created_at, last_used_at, hits, pinned
               FROM thumbs WHERE cache_key = ?1 AND size_class = ?2 AND render_sig = ?3",
            params![key, size.as_str(), sig],
            |r| {
                Ok(ThumbMeta {
                    width: r.get(0)?,
                    height: r.get(1)?,
                    bytes: r.get(2)?,
                    created_at: r.get(3)?,
                    last_used_at: r.get(4)?,
                    hits: r.get(5)?,
                    pinned: r.get::<_, i64>(6)? != 0,
                })
            },
        )
        .optional()?)
}

/// 钉住/取消钉住（钉住的条目 GC 永不淘汰）。
pub fn set_pinned(
    conn: &Connection,
    key: &[u8],
    size: SizeClass,
    sig: &str,
    pinned: bool,
) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE thumbs SET pinned = ?1
          WHERE cache_key = ?2 AND size_class = ?3 AND render_sig = ?4",
        params![i64::from(pinned), key, size.as_str(), sig],
    )?)
}

/// 删除某个键的**所有**尺度与签名（源文件没了、或内容变了要重做时用）。
pub fn remove_key(conn: &Connection, key: &[u8]) -> Result<usize> {
    Ok(conn.execute("DELETE FROM thumbs WHERE cache_key = ?1", [key])?)
}

/// 删除某个键的某个尺度。
pub fn remove(conn: &Connection, key: &[u8], size: SizeClass) -> Result<usize> {
    Ok(conn.execute(
        "DELETE FROM thumbs WHERE cache_key = ?1 AND size_class = ?2",
        params![key, size.as_str()],
    )?)
}

/// 清空整个缓存（设置里的「一键清理」）。返回删掉多少条。
pub fn clear(conn: &Connection) -> Result<usize> {
    Ok(conn.execute("DELETE FROM thumbs", [])?)
}

/// 收掉**其它管线版本**留下的孤儿行（算法升级后的缓存回收）。
///
/// 判据是签名里的版本片段 `-v<数字>`：
///
/// * 当前家族的签名都带 `-v<PIPELINE_VERSION>`（`avif-q90-grid-v16…`、`issue-v16-…`、
///   `raw-original-v16-…`）；带编辑的动态签名 `…+e<指纹>` 也带 ⇒ **保住**；
/// * 老版本（`…-v7+e…`、`…-v5`）不含当前版本 ⇒ **收走**；
/// * **完全不带 `-v<数字>` 片段的签名一律保留** —— 宁可漏收，也不能让一次命名变化把
///   还在用的缓存当成孤儿每次维护都扫一遍（那等于反复清空重渲染）。
///
/// 为什么用 `GLOB '*-v[0-9]*'` 而不是 `LIKE '%-v%'`：后者会被 `-version` 这类
/// 普通单词误命中（2026-10-25 评审时单测当场发现）。
///
/// 为什么不按「当前签名白名单」收（2026-10-25 评审改）：那要求每加一种缓存东西就回来
/// 补一次名单，漏了就会让那些行每 6 小时被删一次；按版本片段判是**自维护**的，
/// 且判错的失效方向是「多留」而不是「误删」。
///
/// 旧版本的行一律收（含 `pinned`）：它们已经不可读取，留着只是永久占字节。
pub fn drop_other_pipeline_versions(conn: &Connection, version: u32) -> Result<usize> {
    let current = format!("%-v{version}%");
    Ok(conn.execute(
        "DELETE FROM thumbs WHERE render_sig GLOB '*-v[0-9]*' AND render_sig NOT LIKE ?1",
        [current],
    )?)
}

/// 一次维护：先收其它管线版本的孤儿，再按容量回收。
pub fn maintenance(conn: &Connection, max_bytes: i64, now_ms: i64) -> Result<(GcOutcome, usize)> {
    let stale = drop_other_pipeline_versions(conn, crate::thumbnail::render::PIPELINE_VERSION)?;
    let outcome = gc(conn, max_bytes, now_ms)?;
    Ok((outcome, stale))
}

/// 统计。
pub fn stats(conn: &Connection) -> Result<CacheStats> {
    let (entries, bytes, pinned): (i64, i64, i64) = conn.query_row(
        "SELECT count(*), coalesce(sum(bytes), 0), coalesce(sum(pinned), 0) FROM thumbs",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let mut stmt = conn.prepare(
        "SELECT size_class, count(*), coalesce(sum(bytes), 0)
           FROM thumbs GROUP BY size_class ORDER BY size_class",
    )?;
    let by_size = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(CacheStats {
        entries,
        bytes,
        pinned,
        by_size,
    })
}

/// 按容量上限回收（frecency）。`max_bytes <= 0` 表示不做容量限制（只回收孤儿）。
///
/// 淘汰顺序：`pinned` 优先保留 → `hits` 少的先走 → `last_used_at` 最久远的先走。
/// **钉住的条目永不淘汰**（哪怕超出上限 —— 用户明确要求留着的东西不该被偷偷删掉）。
pub fn gc(conn: &Connection, max_bytes: i64, now_ms: i64) -> Result<GcOutcome> {
    let _ = now_ms;
    let total: i64 = conn.query_row("SELECT coalesce(sum(bytes), 0) FROM thumbs", [], |r| {
        r.get(0)
    })?;
    let mut out = GcOutcome {
        remaining_bytes: total,
        ..GcOutcome::default()
    };
    if max_bytes <= 0 || total <= max_bytes {
        return Ok(out);
    }

    let mut remaining = total;
    let mut removed = 0i64;
    let mut freed = 0i64;
    // 单次维护的删除预算：一次跑干通常远小于它；给个上限防病态数据卡住维护。
    let mut budget = 50_000usize;
    while remaining > max_bytes && budget > 0 {
        // 一条 SQL 挑出「该先走的」：没钉住的里，命中少、最久没用过的排前面
        let victims = {
            let mut stmt = conn.prepare(
                "SELECT cache_key, size_class, render_sig, bytes FROM thumbs
                  WHERE pinned = 0
                  ORDER BY hits ASC, last_used_at ASC
                  LIMIT 2000",
            )?;

            stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, Vec<u8>>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        if victims.is_empty() {
            break;
        }
        for (key, size, sig, bytes) in victims {
            if remaining <= max_bytes || budget == 0 {
                break;
            }
            budget -= 1;
            let n = conn.execute(
                "DELETE FROM thumbs WHERE cache_key = ?1 AND size_class = ?2 AND render_sig = ?3",
                params![key, size, sig],
            )?;
            if n > 0 {
                removed += 1;
                freed += bytes;
                remaining -= bytes;
            }
        }
    }
    out.removed = removed;
    out.bytes_freed = freed;
    out.remaining_bytes = remaining;
    Ok(out)
}

/// 按库持有小图缓存（`<库根>/cache/thumbs.db`）。
///
/// 库会话（`BrowseState`）持有这个映射：库打开时懒打开、库释放/换位置时
/// 从映射里移除（后台任务手里的 `Arc` 会自然活到任务结束）。
/// 缓存**跟着库走**：换机器/换盘只要库在，缩图就在；删库目录时缩图跟着消失。
#[derive(Default)]
pub struct LibraryThumbs {
    inner: Mutex<HashMap<String, Arc<ThumbsDb>>>,
}

impl LibraryThumbs {
    /// 取（必要时打开）某个库的缓存库。`root` = 库根。
    ///
    /// # Errors
    /// 目录创建 / SQLite 打开失败。调用方应**降级为不缓存**，而不是让看图失败。
    pub fn get(&self, repository_id: &str, root: &Path, now_ms: i64) -> Result<Arc<ThumbsDb>> {
        {
            let guard = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(db) = guard.get(repository_id) {
                return Ok(Arc::clone(db));
            }
        }
        let dir = root.join("cache");
        let opened = Arc::new(ThumbsDb::open(&dir, now_ms)?);
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // 两个线程同时打开同一个库：先到的那份说了算，后来这份直接丢掉。
        if let Some(existing) = guard.get(repository_id) {
            return Ok(Arc::clone(existing));
        }
        guard.insert(repository_id.to_string(), Arc::clone(&opened));
        Ok(opened)
    }

    /// 库释放 / 换位置时调用；之后再有任务拿旧库会重新打开新路径。
    pub fn release(&self, repository_id: &str) {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.remove(repository_id);
    }

    /// 当前进程里已经打开的库（诊断/测试用）。
    #[must_use]
    pub fn is_open(&self, repository_id: &str) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(repository_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::migration;
    use crate::store::pragma;
    use crate::thumbnail::render::{PIPELINE_VERSION, render_sig};

    const T0: i64 = 1_789_516_800_000;

    fn mem() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        pragma::apply(&conn, false).unwrap();
        migration::apply(&mut conn, DbKind::Thumbs, Backups::none(), T0).unwrap();
        conn
    }

    fn fid(n: u8) -> FileId {
        let mut b = [0u8; 16];
        b[0] = n;
        FileId {
            volume_serial: 42,
            file_id: b,
        }
    }

    fn put_n(conn: &Connection, key: &[u8], size: SizeClass, bytes: usize, now: i64) {
        put(
            conn,
            key,
            size,
            render_sig(size),
            &vec![7u8; bytes],
            300,
            200,
            now,
        )
        .unwrap();
    }

    // ---------- 缓存键 ----------

    #[test]
    fn identity_key_is_stable_and_distinct_from_path_key() {
        let by_id = cache_key(Some(&fid(1)), "photos/a.jpg");
        let again = cache_key(Some(&fid(1)), "photos/别的名字.jpg");
        assert_eq!(by_id, again, "身份优先：路径变了键也不变（改名仍然命中）");
        assert_eq!(by_id.len(), 25);
        assert_eq!(by_id[0], b'i');

        let by_path = cache_key(None, "photos/a.jpg");
        assert_eq!(by_path[0], b'p');
        assert_ne!(by_id, by_path, "两种键不可能撞");

        // 全 0 身份（卷序列号与 file_id 都是 0）视为「读不到身份」
        let all_zero = FileId {
            volume_serial: 0,
            file_id: [0u8; 16],
        };
        assert!(all_zero.is_zero());
        assert_eq!(
            cache_key(Some(&all_zero), "photos/a.jpg"),
            by_path,
            "全 0 身份要退回路径键"
        );
    }

    #[test]
    fn path_keys_differ_per_path() {
        assert_ne!(cache_key(None, "a.jpg"), cache_key(None, "b.jpg"));
        assert_eq!(cache_key(None, "a.jpg"), cache_key(None, "a.jpg"));
        // 中文路径也能用
        let k = cache_key(None, "照片/海边.jpg");
        assert!(k.len() > 1);
    }

    // ---------- 存取 ----------

    #[test]
    fn put_then_get_roundtrip() {
        let conn = mem();
        let key = cache_key(None, "a.jpg");
        assert!(
            get(&conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid))
                .unwrap()
                .is_none()
        );

        put_n(&conn, &key, SizeClass::Grid, 16, T0);
        let got = get(&conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid))
            .unwrap()
            .unwrap();
        assert_eq!(got.len(), 16);
        assert!(has(&conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid)).unwrap());
        assert!(!has(&conn, &key, SizeClass::Screen, render_sig(SizeClass::Screen)).unwrap());
    }

    #[test]
    fn get_with_size_returns_bytes_and_dimensions_in_one_read() {
        let conn = mem();
        let key = cache_key(None, "a.jpg");
        assert!(
            get_with_size(&conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid))
                .unwrap()
                .is_none(),
            "未命中是 None，不是错误"
        );
        put_n(&conn, &key, SizeClass::Grid, 16, T0);
        let (bytes, width, height) =
            get_with_size(&conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid))
                .unwrap()
                .unwrap();
        assert_eq!(bytes.len(), 16);
        assert_eq!((width, height), (300, 200), "尺寸与 put 写入的一致");
    }

    #[test]
    fn touch_updates_frecency_counters_and_get_is_read_only() {
        let conn = mem();
        let key = cache_key(None, "a.jpg");
        put_n(&conn, &key, SizeClass::Grid, 8, T0);
        let m0 = meta(&conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid))
            .unwrap()
            .unwrap();
        assert_eq!((m0.hits, m0.last_used_at), (0, T0));

        touch(
            &conn,
            &key,
            SizeClass::Grid,
            render_sig(SizeClass::Grid),
            T0 + 500,
        )
        .unwrap();
        touch(
            &conn,
            &key,
            SizeClass::Grid,
            render_sig(SizeClass::Grid),
            T0 + 900,
        )
        .unwrap();
        let m1 = meta(&conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid))
            .unwrap()
            .unwrap();
        assert_eq!(m1.hits, 2, "命中次数要累加（frecency 的 f）");
        assert_eq!(
            m1.last_used_at,
            T0 + 900,
            "最近使用时间要更新（frecency 的 r）"
        );

        // 反复 get **不该**改计数（它是纯读，读连接是 query_only 的）
        let before = m1;
        for _ in 0..3 {
            get(&conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid)).unwrap();
        }
        let after = meta(&conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid))
            .unwrap()
            .unwrap();
        assert_eq!(
            (after.hits, after.last_used_at),
            (before.hits, before.last_used_at)
        );
    }

    #[test]
    fn put_is_an_upsert() {
        let conn = mem();
        let key = cache_key(None, "a.jpg");
        put_n(&conn, &key, SizeClass::Grid, 8, T0);
        put_n(&conn, &key, SizeClass::Grid, 32, T0 + 1);
        assert_eq!(stats(&conn).unwrap().entries, 1, "同键只该有一条");
        assert_eq!(stats(&conn).unwrap().bytes, 32);
        let got = get(&conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid)).unwrap();
        assert_eq!(got.unwrap().len(), 32);
    }

    #[test]
    fn put_prunes_older_signatures_for_the_same_key() {
        let conn = mem();
        let key = cache_key(None, "a.jpg");
        put_n(&conn, &key, SizeClass::Grid, 10, T0);
        // 换一个签名（编辑确认 / 重置）—— 同一 (文件, 尺度) 只留最新那份
        put(
            &conn,
            &key,
            SizeClass::Grid,
            "avif-q80-grid-v17+e000000000000001",
            &[1u8; 20],
            10,
            10,
            T0 + 1,
        )
        .unwrap();
        assert_eq!(stats(&conn).unwrap().entries, 1);
        assert!(
            get(
                &conn,
                &key,
                SizeClass::Grid,
                "avif-q80-grid-v17+e000000000000001"
            )
            .unwrap()
            .is_some()
        );
        assert!(
            get(&conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid))
                .unwrap()
                .is_none(),
            "旧签名要当场删掉，不能等容量 GC"
        );

        // 写回 base 也一样：只留最新的
        put_n(&conn, &key, SizeClass::Grid, 8, T0 + 2);
        assert_eq!(stats(&conn).unwrap().entries, 1);
        assert!(
            get(&conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn put_rejects_non_grid_sizes() {
        let conn = mem();
        let key = cache_key(None, "a.jpg");
        for size in [SizeClass::Screen, SizeClass::Full] {
            assert!(
                put(&conn, &key, size, render_sig(size), b"x", 1, 1, T0).is_err(),
                "thumbs.db 只存 384：{size:?} 必须被拒"
            );
        }
        assert_eq!(stats(&conn).unwrap().entries, 0);
    }

    // ---------- 删除 ----------

    #[test]
    fn remove_variants() {
        let conn = mem();
        let a = cache_key(None, "a.jpg");
        let b = cache_key(None, "b.jpg");
        put_n(&conn, &a, SizeClass::Grid, 8, T0);
        put_n(&conn, &b, SizeClass::Grid, 8, T0);

        assert_eq!(remove(&conn, &a, SizeClass::Grid).unwrap(), 1);
        assert_eq!(
            remove(&conn, &a, SizeClass::Grid).unwrap(),
            0,
            "再删就没有了"
        );
        assert_eq!(remove_key(&conn, &a).unwrap(), 0, "已经空了");
        put_n(&conn, &a, SizeClass::Grid, 8, T0);
        assert_eq!(remove_key(&conn, &a).unwrap(), 1, "删掉这个键的所有行");
        assert_eq!(stats(&conn).unwrap().entries, 1);
        assert_eq!(clear(&conn).unwrap(), 1, "一键清理");
        assert_eq!(stats(&conn).unwrap().entries, 0);
    }

    #[test]
    fn stale_reclaim_only_drops_other_pipeline_versions() {
        let conn = mem();
        let current = render_sig(SizeClass::Grid);
        let edited = format!("{}+e0badc0de", render_sig(SizeClass::Grid));
        let issue = format!("issue-v{PIPELINE_VERSION}-abc-srcdef");
        let raw = format!("raw-original-v{PIPELINE_VERSION}-srcdef");
        let old_grid = "avif-q90-grid-v7+e0badc0de".to_string();
        let old_issue = "issue-v7-abc-srcdef".to_string();
        let unversioned = "legacy-signature-without-version".to_string();
        // 不同家族用不同键（现实里 base/编辑同一键、定稿与 raw-original 各有自己的键）
        for (index, (name, sig)) in [
            ("current", current),
            ("edited", edited.as_str()),
            ("issue", issue.as_str()),
            ("raw", raw.as_str()),
            ("old-grid", old_grid.as_str()),
            ("old-issue", old_issue.as_str()),
            ("unversioned", unversioned.as_str()),
        ]
        .into_iter()
        .enumerate()
        {
            let key = cache_key(None, name);
            put(&conn, &key, SizeClass::Grid, sig, &[index as u8; 4], 1, 1, T0).unwrap();
        }

        let removed = drop_other_pipeline_versions(&conn, PIPELINE_VERSION).unwrap();
        assert_eq!(removed, 2, "只收走旧版本的两条");
        for (name, sig) in [
            ("current", current),
            ("edited", edited.as_str()),
            ("issue", issue.as_str()),
            ("raw", raw.as_str()),
            ("unversioned", unversioned.as_str()),
        ] {
            let key = cache_key(None, name);
            assert!(
                get(&conn, &key, SizeClass::Grid, sig).unwrap().is_some(),
                "{name} 要保住"
            );
        }
        assert!(
            get(&conn, &cache_key(None, "old-grid"), SizeClass::Grid, &old_grid)
                .unwrap()
                .is_none()
        );
        assert!(
            get(&conn, &cache_key(None, "old-issue"), SizeClass::Grid, &old_issue)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn maintenance_reclaims_old_signatures_but_keeps_current_families() {
        let conn = mem();
        let issue = format!("issue-v{PIPELINE_VERSION}-abc-srcdef");
        let raw = format!("raw-original-v{PIPELINE_VERSION}-srcdef");
        for (name, sig) in [
            ("base", render_sig(SizeClass::Grid).to_string()),
            ("edited", format!("{}+e1234", render_sig(SizeClass::Grid))),
            ("issue", issue.clone()),
            ("raw", raw.clone()),
            ("old", "avif-q90-strip-v7".to_string()),
        ] {
            let key = cache_key(None, name);
            put(&conn, &key, SizeClass::Grid, &sig, &[1u8; 4], 1, 1, T0).unwrap();
        }
        let (outcome, stale) = maintenance(&conn, DEFAULT_MAX_BYTES, T0 + 1).unwrap();
        assert_eq!(stale, 1, "只该收走旧签名那一条");
        assert_eq!(outcome.removed, 0, "没超限不应按容量淘汰");
        assert_eq!(stats(&conn).unwrap().entries, 4);
        assert!(get(&conn, &cache_key(None, "base"), SizeClass::Grid, render_sig(SizeClass::Grid)).unwrap().is_some());
        assert!(get(&conn, &cache_key(None, "edited"), SizeClass::Grid, &format!("{}+e1234", render_sig(SizeClass::Grid))).unwrap().is_some());
        assert!(get(&conn, &cache_key(None, "issue"), SizeClass::Grid, &issue).unwrap().is_some());
        assert!(get(&conn, &cache_key(None, "raw"), SizeClass::Grid, &raw).unwrap().is_some());
    }

    #[test]
    fn maintenance_enforces_capacity_after_stale_cleanup() {
        let conn = mem();
        for i in 0..5 {
            put_n(&conn, &cache_key(None, &format!("bulk{i}.jpg")), SizeClass::Grid, 100, T0 + i);
        }
        let (outcome, stale) = maintenance(&conn, 250, T0 + 10).unwrap();
        assert_eq!(stale, 0);
        assert!(outcome.removed >= 3, "应当删到上限以下");
        assert!(stats(&conn).unwrap().bytes <= 250);
    }

    #[test]
    fn pinned_survives_gc() {
        let conn = mem();
        let pinned_key = cache_key(None, "keeper.jpg");
        put_n(&conn, &pinned_key, SizeClass::Grid, 100, T0);
        set_pinned(
            &conn,
            &pinned_key,
            SizeClass::Grid,
            render_sig(SizeClass::Grid),
            true,
        )
        .unwrap();

        // 再塞一批「大且没人用」的
        for i in 0..5 {
            let k = cache_key(None, &format!("bulk{i}.jpg"));
            put_n(&conn, &k, SizeClass::Grid, 100, T0 + i);
        }
        let out = gc(&conn, 150, T0 + 1000).unwrap();
        assert!(out.removed > 0, "超上限要淘汰");
        assert!(
            has(
                &conn,
                &pinned_key,
                SizeClass::Grid,
                render_sig(SizeClass::Grid)
            )
            .unwrap(),
            "钉住的绝不能被淘汰"
        );
    }

    // ---------- GC ----------

    #[test]
    fn gc_does_nothing_when_under_limit() {
        let conn = mem();
        put_n(&conn, &cache_key(None, "a.jpg"), SizeClass::Grid, 10, T0);
        let out = gc(&conn, 1000, T0).unwrap();
        assert_eq!(out.removed, 0);
        assert_eq!(out.remaining_bytes, 10);
    }

    #[test]
    fn gc_evicts_least_valuable_first() {
        let conn = mem();
        let cold = cache_key(None, "cold.jpg");
        let warm = cache_key(None, "warm.jpg");
        let hot = cache_key(None, "hot.jpg");
        for (k, hits) in [(&cold, 0), (&warm, 5), (&hot, 50)] {
            put_n(&conn, k, SizeClass::Grid, 100, T0);
            for _ in 0..hits {
                touch(
                    &conn,
                    k,
                    SizeClass::Grid,
                    render_sig(SizeClass::Grid),
                    T0 + 1,
                )
                .unwrap();
            }
        }
        // 上限 250：必须至少淘汰一条，且先走「命中少 + 最久没用」的 cold
        let out = gc(&conn, 250, T0 + 100).unwrap();
        assert!(out.removed >= 1);
        assert!(!has(&conn, &cold, SizeClass::Grid, render_sig(SizeClass::Grid)).unwrap());
        assert!(has(&conn, &hot, SizeClass::Grid, render_sig(SizeClass::Grid)).unwrap());
    }

    #[test]
    fn gc_with_zero_limit_is_a_noop() {
        let conn = mem();
        put_n(&conn, &cache_key(None, "a.jpg"), SizeClass::Grid, 100, T0);
        let out = gc(&conn, 0, T0).unwrap();
        assert_eq!(out.removed, 0, "0 = 不做容量限制");
        assert_eq!(out.remaining_bytes, 100);
    }

    // ---------- 统计 ----------

    #[test]
    fn stats_counts_entries_and_bytes() {
        let conn = mem();
        let k1 = cache_key(None, "a.jpg");
        let k2 = cache_key(None, "b.jpg");
        put_n(&conn, &k1, SizeClass::Grid, 100, T0);
        put_n(&conn, &k2, SizeClass::Grid, 50, T0);
        let s = stats(&conn).unwrap();
        assert_eq!((s.entries, s.bytes), (2, 150));
        assert_eq!(s.by_size, vec![("grid".to_string(), 2, 150)]);
    }

    #[test]
    fn gc_converges_in_one_run_beyond_the_batch_limit() {
        let conn = mem();
        for i in 0..5_000 {
            put_n(
                &conn,
                &cache_key(None, &format!("bulk{i}.jpg")),
                SizeClass::Grid,
                100,
                T0 + i,
            );
        }
        let out = gc(&conn, 50_000, T0 + 10_000).unwrap();
        assert!(out.removed >= 4_500, "一次维护要能删到上限以下");
        assert!(stats(&conn).unwrap().bytes <= 50_000);
    }

    #[test]
    fn note_use_batches_frecency_and_ignores_repeats_within_an_hour() {
        let dir = tempfile::tempdir().unwrap();
        let db = ThumbsDb::open(dir.path().join("cache"), T0).unwrap();
        let key = cache_key(None, "a.jpg");
        {
            let key = key.clone();
            db.write(move |conn| {
                put(
                    conn,
                    &key,
                    SizeClass::Grid,
                    render_sig(SizeClass::Grid),
                    b"x",
                    1,
                    1,
                    T0,
                )
            })
            .unwrap();
        }
        // 行是 T0 写的；超过一小时后才算一次「用过」
        db.note_use(&key, SizeClass::Grid, render_sig(SizeClass::Grid), T0 + 4_000_000);
        db.flush_touches(T0 + 4_000_000);
        let m = db
            .read(|conn| meta(conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid)))
            .unwrap()
            .unwrap();
        assert_eq!(m.hits, 1);

        // 一小时内重复看同一张：不重复写
        db.note_use(&key, SizeClass::Grid, render_sig(SizeClass::Grid), T0 + 5_000_000);
        db.flush_touches(T0 + 5_000_000);
        let m = db
            .read(|conn| meta(conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid)))
            .unwrap()
            .unwrap();
        assert_eq!(m.hits, 1, "一小时内的重复命中只算一次");

        // 超过一小时：再算一次
        db.note_use(&key, SizeClass::Grid, render_sig(SizeClass::Grid), T0 + 8_000_000);
        db.flush_touches(T0 + 8_000_000);
        let m = db
            .read(|conn| meta(conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid)))
            .unwrap()
            .unwrap();
        assert_eq!(m.hits, 2);
    }

    // ---------- 门面（真实文件）----------

    #[test]
    fn thumbs_db_opens_persists_and_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");

        let key = cache_key(None, "photos/a.jpg");
        let key_for_write = key.clone(); // 闭包要 'static，只能把副本 move 进去
        {
            let db = ThumbsDb::open(&cache_dir, T0).unwrap();
            assert!(db.path().ends_with(THUMBS_FILE_NAME));
            db.write(move |conn| {
                put(
                    conn,
                    &key_for_write,
                    SizeClass::Grid,
                    render_sig(SizeClass::Grid),
                    b"jpeg-bytes",
                    300,
                    200,
                    T0,
                )
            })
            .unwrap();
        } // Drop：写线程把队列跑干

        let db = ThumbsDb::open(&cache_dir, T0 + 1).unwrap();
        let got = db
            .read(|conn| get(conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid)))
            .unwrap()
            .unwrap();
        assert_eq!(got, b"jpeg-bytes");
        assert_eq!(db.read(stats).unwrap().entries, 1);
    }

    #[test]
    fn deleting_the_cache_dir_only_costs_regeneration() {
        // 红线（AGENTS.md §6.5）：整个缓存目录删掉 → 功能降级但可用
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("cache");
        let key = cache_key(None, "a.jpg");
        let key_for_write = key.clone();
        {
            let db = ThumbsDb::open(&cache_dir, T0).unwrap();
            db.write(move |conn| {
                put(
                    conn,
                    &key_for_write,
                    SizeClass::Grid,
                    render_sig(SizeClass::Grid),
                    b"x",
                    1,
                    1,
                    T0,
                )
            })
            .unwrap();
        }
        std::fs::remove_dir_all(&cache_dir).unwrap();

        let db = ThumbsDb::open(&cache_dir, T0 + 1).unwrap();
        assert_eq!(db.read(stats).unwrap().entries, 0, "缓存空空如也");
        assert!(
            db.read(|conn| get(conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid)))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn library_thumbs_are_isolated_per_repository() {
        let dir = tempfile::tempdir().unwrap();
        let root_a = dir.path().join("repoA");
        let root_b = dir.path().join("repoB");
        std::fs::create_dir_all(&root_a).unwrap();
        std::fs::create_dir_all(&root_b).unwrap();
        let map = LibraryThumbs::default();
        let a = map.get("repoA", &root_a, T0).unwrap();
        let b = map.get("repoB", &root_b, T0).unwrap();
        assert!(a.path().starts_with(&root_a), "A 的缓存要在 A 的库根下");
        assert!(b.path().starts_with(&root_b), "B 的缓存要在 B 的库根下");

        let key = cache_key(None, "same.jpg");
        {
            let key = key.clone();
            a.write(move |conn| {
                put(
                    conn,
                    &key,
                    SizeClass::Grid,
                    render_sig(SizeClass::Grid),
                    b"a",
                    1,
                    1,
                    T0,
                )
            })
            .unwrap();
        }
        assert!(
            b.read(|conn| get(conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid)))
                .unwrap()
                .is_none(),
            "库 B 不能读到库 A 的缓存"
        );
        assert!(map.is_open("repoA"));
        drop(a);
        map.release("repoA");
        assert!(!map.is_open("repoA"));
        let reopened = map.get("repoA", &root_a, T0 + 1).unwrap();
        assert_eq!(
            reopened.read(stats).unwrap().entries,
            1,
            "释放后再打开仍读得到原库缓存"
        );
    }
}
