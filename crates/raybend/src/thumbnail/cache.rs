//! 缩略图缓存：`<app data>/cache/<repository_id>/thumbs.db`。
//!
//! # 位置与形态（`AGENTS.md` §6.5）
//!
//! * 缓存**不在库根目录**里 —— 库根只放 `catalog.db` 与 `photos/`（`REPOSITORY.md` §2）。
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
        Ok(Self { pool, writer, path })
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

/// 删掉**不属于当前签名**的老条目（算法升级后的孤儿回收）。
///
/// # 为什么是**前缀匹配**而不是 `NOT IN`
///
/// 带编辑的渲染签名是**动态**的（`avif-q90-grid-v6+e<指纹>`，见
/// `render::render_sig_with_edit`）—— 一张编辑过的照片就有一个新签名，
/// 用 `NOT IN (几个静态签名)` 会把它们全当成孤儿删掉（每次 GC 都白删一遍）。
/// 改成「前缀命中就留着」之后：
///
/// * 静态签名（没编辑过）前缀就是它自己 ⇒ 行为与以前一致；
/// * 编辑过的签名前缀是基础签名 ⇒ 被保住；
/// * 真·老版本（`avif-q90-grid-v5+…`）前缀对不上 ⇒ 照旧回收。
pub fn drop_stale_signatures(conn: &Connection, sigs: &[&str]) -> Result<usize> {
    if sigs.is_empty() {
        return clear(conn);
    }
    let clauses = vec!["render_sig NOT LIKE ?"; sigs.len()].join(" AND ");
    let patterns: Vec<String> = sigs.iter().map(|sig| format!("{sig}%")).collect();
    let sql = format!("DELETE FROM thumbs WHERE {clauses}");
    Ok(conn.execute(&sql, rusqlite::params_from_iter(patterns.iter()))?)
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
    use crate::thumbnail::render::render_sig;

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
        assert!(!has(&conn, &key, SizeClass::Strip, render_sig(SizeClass::Strip)).unwrap());
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
        put_n(&conn, &key, SizeClass::Strip, 20, T0);
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
            get(&conn, &key, SizeClass::Strip, render_sig(SizeClass::Strip))
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
        put_n(&conn, &a, SizeClass::Strip, 8, T0);
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
    fn drop_stale_signatures_reclaims_old_pipeline_versions() {
        let conn = mem();
        let key = cache_key(None, "a.jpg");
        put(
            &conn,
            &key,
            SizeClass::Grid,
            "jpeg-q82-g1",
            &[1u8; 4],
            1,
            1,
            T0,
        )
        .unwrap();
        put(
            &conn,
            &key,
            SizeClass::Grid,
            "jpeg-q82-g2",
            &[2u8; 4],
            1,
            1,
            T0,
        )
        .unwrap();
        put(
            &conn,
            &key,
            SizeClass::Strip,
            "jpeg-q82-g1",
            &[3u8; 4],
            1,
            1,
            T0,
        )
        .unwrap();

        let removed = drop_stale_signatures(&conn, &["jpeg-q82-g1"]).unwrap();
        assert_eq!(removed, 1, "g2 是孤儿，收走");
        assert_eq!(stats(&conn).unwrap().entries, 2);
        /*
         * **带编辑的动态签名**（前缀 = 当前基础签名）必须被保住：
         * 编辑过的照片签名是 `avif-q90-grid-v7+e<指纹>`，用老的 `NOT IN` 判据会被
         * 每次 GC 都白删一遍（每次都要重渲染）。现在按前缀匹配 ⇒ 保住。
         */
        put(&conn, b"k", SizeClass::Grid, "jpeg-q82-g1+e0badc0de", b"x", 1, 1, 0).unwrap();
        let removed = drop_stale_signatures(&conn, &["jpeg-q82-g1"]).unwrap();
        assert_eq!(removed, 0, "编辑过的签名前缀命中，不许当孤儿删掉");
        assert_eq!(stats(&conn).unwrap().entries, 3);
        // 语法糖：不传任何签名 = 清空
        assert_eq!(drop_stale_signatures(&conn, &[]).unwrap(), 3);
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
        put_n(&conn, &k1, SizeClass::Strip, 25, T0);
        let s = stats(&conn).unwrap();
        assert_eq!((s.entries, s.bytes), (3, 175));
        assert_eq!(
            s.by_size,
            vec![("grid".to_string(), 2, 150), ("strip".to_string(), 1, 25)]
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
