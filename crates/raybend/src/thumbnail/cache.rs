//! 缩略图缓存：`<app data>/cache/<repository_id>/thumbs.db`。
//!
//! # 位置与形态（`AGENTS.md` §6.5）
//!
//! * 缓存**不在库根目录**里 —— 库根只放 `catalog.db` 与 `photos/`（`memory/FUNCTION-REPOSITORY.md` §2）。
//! * 用**独立的小 SQLite 库**而不是散文件：一次事务写一张、读一张很快，
//!   而且 GC 就是几条 SQL（不用扫目录、不用管目录分片）。
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

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::Result;
use crate::store::db::migrate_file;
use crate::store::file_id::FileId;
use crate::store::migration::{Backups, DbKind};
use crate::store::pool::ReadPool;
use crate::store::writer::Writer;
use crate::thumbnail::render::SizeClass;

/// 缓存库文件名。
pub const THUMBS_FILE_NAME: &str = "thumbs.db";

/// 每库小图缓存默认上限：1 GiB（FUTURE G26 建议值；设置面板以后可覆盖）。
pub const DEFAULT_MAX_BYTES: i64 = 1 << 30;

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
}

impl ThumbsDb {
    /// 打开（必要时创建）缓存库。`dir` 是**该库的缓存目录**
    /// （通常是 `<app data>/cache/<repository_id>/`）。
    pub fn open(dir: impl AsRef<Path>, now_ms: i64) -> Result<Self> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir)?;
        let path = dir.join(THUMBS_FILE_NAME);

        migrate_file(&path, DbKind::Thumbs, Backups::none(), now_ms, true)?;

        let pool = ReadPool::open(&path)?;
        let writer = Writer::open(&path)?;
        Ok(Self { pool, writer, path, last_maintenance: Mutex::new(None) })
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
        self.write(move |conn| maintenance(conn, max_bytes, now_ms)).map(Some)
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

    // 一条 SQL 挑出「该先走的」：没钉住的里，命中少、最久没用过的排前面
    let mut stmt = conn.prepare(
        "SELECT cache_key, size_class, render_sig, bytes FROM thumbs
          WHERE pinned = 0
          ORDER BY hits ASC, last_used_at ASC
          LIMIT 2000",
    )?;
    let victims = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut remaining = total;
    for (key, size, sig, bytes) in victims {
        if remaining <= max_bytes {
            break;
        }
        let n = conn.execute(
            "DELETE FROM thumbs WHERE cache_key = ?1 AND size_class = ?2 AND render_sig = ?3",
            params![key, size, sig],
        )?;
        if n > 0 {
            out.removed += 1;
            out.bytes_freed += bytes;
            remaining -= bytes;
        }
    }
    out.remaining_bytes = remaining;
    Ok(out)
}

/// 把一个库的缓存目录算出来（`<app data>/cache/<repository_id>/`）。
///
/// 缓存位置将来可按库配置（`AGENTS.md` §6.5 允许「随外接盘」），所以**不要**
/// 在各处硬拼这个路径，一律走这里。
#[must_use]
pub fn cache_dir(app_data: &Path, repository_id: &str) -> PathBuf {
    app_data.join("cache").join(repository_id)
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
            get_with_size(&conn, &key, SizeClass::Screen, render_sig(SizeClass::Screen))
                .unwrap()
                .is_none(),
            "未命中是 None，不是错误"
        );
        put_n(&conn, &key, SizeClass::Screen, 16, T0);
        let (bytes, width, height) =
            get_with_size(&conn, &key, SizeClass::Screen, render_sig(SizeClass::Screen))
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
    fn sizes_and_signatures_are_independent_rows() {
        let conn = mem();
        let key = cache_key(None, "a.jpg");
        put_n(&conn, &key, SizeClass::Grid, 10, T0);
        put_n(&conn, &key, SizeClass::Screen, 20, T0);
        // 老签名（算法升级前）
        put(
            &conn,
            &key,
            SizeClass::Grid,
            "jpeg-q50-g0",
            &[1u8; 5],
            10,
            10,
            T0,
        )
        .unwrap();

        assert_eq!(stats(&conn).unwrap().entries, 3);
        assert!(
            get(&conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid))
                .unwrap()
                .is_some()
        );
        assert!(
            get(&conn, &key, SizeClass::Screen, render_sig(SizeClass::Screen))
                .unwrap()
                .is_some()
        );
        assert!(
            get(&conn, &key, SizeClass::Grid, "jpeg-q50-g0")
                .unwrap()
                .is_some()
        );
    }

    // ---------- 删除 ----------

    #[test]
    fn remove_variants() {
        let conn = mem();
        let a = cache_key(None, "a.jpg");
        let b = cache_key(None, "b.jpg");
        put_n(&conn, &a, SizeClass::Grid, 8, T0);
        put_n(&conn, &a, SizeClass::Screen, 8, T0);
        put_n(&conn, &b, SizeClass::Grid, 8, T0);

        assert_eq!(remove(&conn, &a, SizeClass::Grid).unwrap(), 1);
        assert_eq!(
            remove(&conn, &a, SizeClass::Grid).unwrap(),
            0,
            "再删就没有了"
        );
        assert_eq!(remove_key(&conn, &a).unwrap(), 1, "剩下那个尺度");
        assert_eq!(stats(&conn).unwrap().entries, 1);
        assert_eq!(clear(&conn).unwrap(), 1, "一键清理");
        assert_eq!(stats(&conn).unwrap().entries, 0);
    }

    #[test]
    fn stale_reclaim_only_drops_other_pipeline_versions() {
        let conn = mem();
        let key = cache_key(None, "a.jpg");
        let current = render_sig(SizeClass::Grid);
        let edited = format!("{}+e0badc0de", render_sig(SizeClass::Screen));
        let issue = format!("issue-v{PIPELINE_VERSION}-abc-srcdef");
        let raw = format!("raw-original-v{PIPELINE_VERSION}-srcdef");
        let old_grid = "avif-q90-grid-v7+e0badc0de".to_string();
        let old_issue = "issue-v7-abc-srcdef".to_string();
        let unversioned = "legacy-signature-without-version".to_string();
        for (index, sig) in [
            current.to_string(),
            edited.clone(),
            issue.clone(),
            raw.clone(),
            old_grid.clone(),
            old_issue.clone(),
            unversioned.clone(),
        ]
        .iter()
        .enumerate()
        {
            put(&conn, &key, SizeClass::Grid, sig, &[index as u8; 4], 1, 1, T0).unwrap();
        }

        let removed = drop_other_pipeline_versions(&conn, PIPELINE_VERSION).unwrap();
        assert_eq!(removed, 2, "只收走旧版本的两条");
        assert!(get(&conn, &key, SizeClass::Grid, current).unwrap().is_some());
        assert!(get(&conn, &key, SizeClass::Grid, &edited).unwrap().is_some(), "带编辑的动态签名要保住");
        assert!(get(&conn, &key, SizeClass::Grid, &issue).unwrap().is_some());
        assert!(get(&conn, &key, SizeClass::Grid, &raw).unwrap().is_some());
        assert!(get(&conn, &key, SizeClass::Grid, &unversioned).unwrap().is_some(), "不带版本片段的一律保留（宁可漏收）");
        assert!(get(&conn, &key, SizeClass::Grid, &old_grid).unwrap().is_none());
        assert!(get(&conn, &key, SizeClass::Grid, &old_issue).unwrap().is_none());
    }

    #[test]
    fn maintenance_reclaims_old_signatures_but_keeps_current_families() {
        let conn = mem();
        let key = cache_key(None, "a.jpg");
        let issue = format!("issue-v{PIPELINE_VERSION}-abc-srcdef");
        let raw = format!("raw-original-v{PIPELINE_VERSION}-srcdef");
        for (size, sig) in [
            (SizeClass::Grid, render_sig(SizeClass::Grid).to_string()),
            (SizeClass::Screen, format!("{}+e1234", render_sig(SizeClass::Screen))),
            (SizeClass::Grid, issue.clone()),
            (SizeClass::Grid, raw.clone()),
            (SizeClass::Grid, "avif-q90-strip-v7".to_string()),
        ] {
            put(&conn, &key, size, &sig, &[1u8; 4], 1, 1, T0).unwrap();
        }
        let (outcome, stale) = maintenance(&conn, DEFAULT_MAX_BYTES, T0 + 1).unwrap();
        assert_eq!(stale, 1, "只该收走旧签名那一条");
        assert_eq!(outcome.removed, 0, "没超限不应按容量淘汰");
        assert_eq!(stats(&conn).unwrap().entries, 4);
        assert!(get(&conn, &key, SizeClass::Grid, render_sig(SizeClass::Grid)).unwrap().is_some());
        assert!(get(&conn, &key, SizeClass::Screen, &format!("{}+e1234", render_sig(SizeClass::Screen))).unwrap().is_some());
        assert!(get(&conn, &key, SizeClass::Grid, &issue).unwrap().is_some());
        assert!(get(&conn, &key, SizeClass::Grid, &raw).unwrap().is_some());
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
    fn stats_group_by_size_class() {
        let conn = mem();
        let k1 = cache_key(None, "a.jpg");
        let k2 = cache_key(None, "b.jpg");
        put_n(&conn, &k1, SizeClass::Grid, 100, T0);
        put_n(&conn, &k2, SizeClass::Grid, 50, T0);
        put_n(&conn, &k1, SizeClass::Screen, 25, T0);
        let s = stats(&conn).unwrap();
        assert_eq!((s.entries, s.bytes), (3, 175));
        assert_eq!(
            s.by_size,
            vec![("grid".to_string(), 2, 150), ("screen".to_string(), 1, 25)]
        );
    }

    // ---------- 门面（真实文件）----------

    #[test]
    fn thumbs_db_opens_persists_and_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = cache_dir(dir.path(), "0AbCdEfGhIjKlMn");
        std::fs::create_dir_all(&cache_dir).unwrap();

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
        let cache_dir = cache_dir(dir.path(), "0AbCdEfGhIjKlMn");
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
    fn cache_dir_is_per_repository() {
        let a = cache_dir(Path::new("/app"), "repoA");
        let b = cache_dir(Path::new("/app"), "repoB");
        assert_ne!(a, b);
        assert!(a.ends_with("repoA"));
        assert!(a.to_string_lossy().contains("cache"));
    }
}
