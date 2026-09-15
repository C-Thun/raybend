//! SQLite 连接的统一设置。
//!
//! 来源：`AGENTS.md` §6.4 —— 读写连接**都要**套这一套：
//!
//! | PRAGMA | 值 | 为什么 |
//! | --- | --- | --- |
//! | `journal_mode` | `WAL` | 多读者 + 单写者；写不阻塞读 |
//! | `synchronous` | `NORMAL` | WAL 下的常规选择：崩溃不丢已提交事务（可能丢最后几次 checkpoint） |
//! | `busy_timeout` | `5000` | 单写者方案下本不该有锁等待，这是**兜底**（外部工具也在开同一个库时） |
//! | `foreign_keys` | `ON` | 外键约束默认是关的，必须显式打开（每个连接都要） |
//!
//! 注意：`journal_mode` 是**持久化**在库文件里的（一次设置，后续连接继承），
//! 而 `foreign_keys` 与 `busy_timeout` 是**每连接**的。

use crate::Result;
use rusqlite::Connection;

/// 忙碌等待上限（毫秒）。
pub const BUSY_TIMEOUT_MS: u32 = 5_000;

/// 给一个连接套上统一设置。读写连接都该调用。
///
/// `read_only` 为真时**不**尝试设置 `journal_mode`（只读连接改不了它，
/// 且 WAL 已经写在文件头里了）。
pub fn apply(conn: &Connection, read_only: bool) -> Result<()> {
    if !read_only {
        // 返回的是生效后的 journal_mode。**内存库不支持 WAL**（会回落到 'memory'），
        // 所以这里不校验结果，只确保语句执行到（文件库的 WAL 由测试单独验证）。
        let _mode: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    }
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "busy_timeout", BUSY_TIMEOUT_MS)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

/// 只读连接的额外设置：避免任何写入。
pub fn apply_read_only(conn: &Connection) -> Result<()> {
    apply(conn, true)?;
    conn.pragma_update(None, "query_only", "ON")?;
    Ok(())
}

/// 读当前 `journal_mode`（诊断/测试用）。
pub fn journal_mode(conn: &Connection) -> Result<String> {
    Ok(conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wal_is_enabled_on_a_file_database() {
        // WAL 是**持久化**在库文件里的设置，内存库验证不了，必须用真实文件
        let dir = tempfile::tempdir().unwrap();
        let conn = Connection::open(dir.path().join("t.db")).unwrap();
        apply(&conn, false).unwrap();
        assert_eq!(journal_mode(&conn).unwrap().to_lowercase(), "wal");
        // 另一个连接打开同一个库，应当继承 WAL
        let conn2 = Connection::open(dir.path().join("t.db")).unwrap();
        apply(&conn2, false).unwrap();
        assert_eq!(journal_mode(&conn2).unwrap().to_lowercase(), "wal");
    }

    #[test]
    fn applies_connection_settings() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn, false).unwrap();

        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        let bt: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
            .unwrap();
        assert_eq!(bt, i64::from(BUSY_TIMEOUT_MS));
        let sync: i64 = conn
            .query_row("PRAGMA synchronous", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sync, 1, "NORMAL == 1");
    }

    #[test]
    fn read_only_rejects_writes() {
        let conn = Connection::open_in_memory().unwrap();
        apply_read_only(&conn).unwrap();
        let err = conn.execute_batch("CREATE TABLE t(x)");
        assert!(err.is_err(), "query_only 之后写入必须失败");
    }

    #[test]
    fn foreign_keys_are_actually_enforced() {
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn, false).unwrap();
        conn.execute_batch(
            "CREATE TABLE parent(id INTEGER PRIMARY KEY);
             CREATE TABLE child(id INTEGER PRIMARY KEY, pid INTEGER REFERENCES parent(id));",
        )
        .unwrap();
        let ok = conn.execute("INSERT INTO child(id, pid) VALUES (1, 999)", []);
        assert!(ok.is_err(), "外键必须真的生效");
    }

    #[test]
    fn read_only_mode_skips_journal_mode_but_sets_the_rest() {
        // 只读连接不该因为设置 journal_mode 而失败
        let conn = Connection::open_in_memory().unwrap();
        apply(&conn, true).unwrap();
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);
    }
}
