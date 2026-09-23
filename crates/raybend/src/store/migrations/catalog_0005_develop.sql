-- catalog.db（每库一个）schema v5：**编辑栈**（M3-W3）
--
-- 这是「无损编辑」的真相源：一张照片的调整参数与曲线存这里，**原始文件永不被改写**
-- （`AGENTS.md` §6.4 的真相源规则；XMP 只是互操作通道，W3 只定契约不实现）。
--
-- # 本轮只有 `latest` 一个存储位（人类 2026-09-24 定）
--
-- 界面上的 issue 有三个来源，但只有一个是**存下来的**：
--   * `SOOC` —— **虚拟**：该资产有 JPG 文件时才有（相机直出，不可编辑）；
--   * `RAW`  —— **虚拟**：该资产有 RAW 文件时才有（完整解码，不可编辑）；
--   * `latest` —— **就是这张表**：当前编辑结果，唯一可编辑的那个。
-- 前两个是「文件本身」，不该也不能进库；`latest` 才是编辑栈。
--
-- `issue_kind` 这一列现在恒为 `'latest'`，是为 **M3-W6** 留的位：
-- 那一波会把编辑栈整成「多 issue + 变更集 + 调整后缓存快照」的形态，并给 issue
-- 数据结构打版本号（现阶段旧数据可直接丢弃、不写迁移 —— 见 `PLAN.md` §M3-W6）。
--
-- # 为什么参数只存**非默认值**
--
-- 表里没有的行 = 这一项没动过。于是「重置这一项」「重置全部」「这项动没动」
-- 三个问题共用同一套判断（`develop::params::DevelopParams` 的 `dirty` 就是它）。
-- 默认值本身**不进库**（`src/api/develop-params.json` 是唯一真相）——
-- 否则改一次默认值就要写一次数据迁移。

CREATE TABLE develop_stacks (
    asset_id   INTEGER PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    -- 局部调整（蒙版）的预留字段：`FUTURE.md` D4 明确要求建表时留位。
    -- 现在恒为 NULL（W3 不做局部调整），格式留待那一波定。
    masks      TEXT,
    -- issue 种类：本轮恒为 'latest'（见文件头；W6 会扩成多 issue）
    issue_kind TEXT    NOT NULL DEFAULT 'latest',
    -- **拍摄色温**（K）：色温拉杆的基线。它必须跟着 issue 一起存 ——
    -- 否则「同一个参数」在编辑器（有 as-shot）与缩略图（读不到元数据）会渲染出两种颜色。
    as_shot_k  REAL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- 调整参数（曝光 / 反差 / 色温…）：一行一项，只存非默认值
CREATE TABLE develop_params (
    asset_id INTEGER NOT NULL REFERENCES develop_stacks(asset_id) ON DELETE CASCADE,
    param_id TEXT    NOT NULL,          -- 与 develop-params.json 的 id 一致
    value    REAL    NOT NULL,
    PRIMARY KEY (asset_id, param_id)
);

-- 曲线：一行一个通道，控制点存 JSON（`[[x, y], …]`，归一化 0..1）
--
-- 为什么用 JSON 而不是一张点表：控制点总是**整体读写**（编辑器一次拖动就重排整条曲线），
-- 拆成行只会多出「顺序」这个需要维护的状态，没有任何查询会按单个点来查。
CREATE TABLE develop_curves (
    asset_id INTEGER NOT NULL REFERENCES develop_stacks(asset_id) ON DELETE CASCADE,
    channel  TEXT    NOT NULL,          -- rgb / r / g / b
    points   TEXT    NOT NULL,          -- JSON：[[x, y], …]
    PRIMARY KEY (asset_id, channel)
);

-- 「这张照片编辑过吗」是列表与网格都要问的问题（缩略图要不要走编辑管线）。
-- 只按主键查就够了，这里给的是「按更新时间找出最近编辑过的」那条路（将来用得上）。
CREATE INDEX idx_develop_stacks_updated ON develop_stacks(updated_at DESC);
