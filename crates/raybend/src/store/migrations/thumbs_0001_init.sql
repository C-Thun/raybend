-- thumbs.db（每库一个，放在 <app data>/cache/<repository_id>/）schema v1
--
-- 这是**派生数据**，不是真相源：整个文件删掉，功能降级但完全可用
-- （AGENTS.md §6.5 的红线）。所以它：
--   * 不做快照备份（升级前不需要保命）；
--   * 不与 catalog.db 做任何外键关联（缓存库可以随时消失）。
--
-- 缓存键 = (cache_key, size_class, render_sig)：
--   * cache_key  —— 文件身份优先（改名/移动后仍然命中），读不到身份时退回折叠路径；
--     首字节做判别：'i' = 身份（8 字节卷序列号 + 16 字节 file_id），'p' = 路径字节
--   * size_class —— grid（网格，长边 384）/ strip（胶片带，长边 192）
--   * render_sig —— 渲染签名（编码格式 + 质量 + 管线版本）。算法升级后改这个串，
--     旧缓存自动变成孤儿并被 GC 收走，**不需要写数据迁移**。

CREATE TABLE thumbs (
    cache_key    BLOB    NOT NULL,
    size_class   TEXT    NOT NULL,
    render_sig   TEXT    NOT NULL,
    width        INTEGER NOT NULL,
    height       INTEGER NOT NULL,
    bytes        INTEGER NOT NULL,
    data         BLOB    NOT NULL,
    created_at   INTEGER NOT NULL,
    last_used_at INTEGER NOT NULL,
    hits         INTEGER NOT NULL DEFAULT 0,   -- 命中次数（frecency 的「f」）
    pinned       INTEGER NOT NULL DEFAULT 0,   -- 钉住：GC 永不淘汰（用户手动保留）
    PRIMARY KEY (cache_key, size_class, render_sig)
) WITHOUT ROWID;

-- 按「使用价值」回收：先淘汰没钉住的、命中少的、最久没用的
CREATE INDEX idx_thumbs_gc ON thumbs(pinned, hits, last_used_at);
CREATE INDEX idx_thumbs_class ON thumbs(size_class, bytes);
