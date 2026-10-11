# XMP 巡检工具（验收辅助）

**状态**：已批准实施（崔总 2026-10-09 问答第 4 题选「清单落 spec §10 + 我写巡检脚本」），本轮完成。
**关联**：`specs/xmp-sidecar.md`（被检查的规格）、`specs/xmp-write-toggle.md`（同期增补）。

---

## 1. 目标与非目标

* **目标**：给一个库根，一条命令把 **sidecar 的文件层**核对完 —— 把「人对着资源管理器数文件」这件事自动化，
  让真机验收只剩 GUI 动作（`AGENTS.md` §2.8：E2E 自动化须是可重复执行的测试代码，不临时手跑）。
* **非目标**：不改盘上任何文件（**只读工具**）；不做别家方言读取；不替代 GUI 验收（界面/交互仍归人类）。

## 2. 判定口径（逐资产）

对库内每个资产，先由核心的 `xmp::collect` 从 DB 组装「应写出什么」（与运行时写出口**同一份实现**），
再与盘上文件比对：

| 检查 | FAIL | WARN | INFO |
| --- | --- | --- | --- |
| 路径规则（`<主体名>.xmp`、`_RAW/` 折算、`_RAW/` 内不得有自家文件） | 应写位置不对 / `_RAW/` 下有自家 xmp | — | — |
| 存在性 | 有内容却没文件 | 没内容却有文件（陈旧副本） | — |
| 可解析 / 归属 | 解析失败、或「应是自家」的位置上不是自家文件 | — | — |
| `rb:latest` | 与 DB latest 的 `profile_hash` 或 `sourceBase` 不一致 | — | — |
| `rb:profiles` | DB 中某个定稿在文件里缺失或哈希不符 | 文件里多出 DB 没有的定稿 | — |
| 标准元数据（评级/色标/作者/说明/地点/关键词） | 任一不一致 | — | — |
| `rb:tagState` | 与 DB 快照（`photo_tags::snapshot`）不一致 | — | — |
| `crs:` 规则 | 该有（raw 基 latest 且有可映射项）却没有/值与 DB 推导不符；或该无（sooc / 无可映射）却出现 `crs:` | — | — |
| 整体文本 | — | 去掉 `xmp:MetadataDate`/`xmp:CreatorTool` 后仍与 DB 重新组合的结果不同（提示漂移，附首个差异位置） | — |
| 孤儿文件 | 自家 `.xmp` 找不到对应资产 | — | 外来 `.xmp`（不含 `rb:`，不动它） |
| 大小写折叠歧义 | 同一目标名折叠后 ≥2 个文件 | — | — |

判定实现复用现成函数，不重写：`xmp::parse` / `xmp::compose` / `xmp::resolve_sidecar` /
`issues::profile_hash` / `xmp::mapping::crs_block`（经 `packet::crs_fragments` 共用序列化）。

## 3. 形态与入口

* **核心**：`crates/raybend/src/xmp/audit.rs` —— `pub fn audit(conn: &Connection, root: &Path) -> Result<AuditReport>`；
  报告是纯数据（可被测试与将来的 UI 复用）。
* **外壳**：`crates/raybend/examples/xmp-audit.rs` —— 薄打印层：打开库（`CatalogDb::open`，`OpenOpts::unbacked_up`）
  → 调核心 → 打印 → 退出码。
* **入口**：`pnpm check:xmp "<库根>"`（WSL 内跑；Windows 盘库用 `/mnt/c/...`）。
* **限制（写进帮助与 README）**：工具会像应用一样打开库（**旧库会先迁移**，建议先用应用打开一次）；
  为免跨系统同时读写同一个 SQLite，**运行前建议关闭应用**（或先拷一份库）。

## 4. 输出与退出码

```text
XMP 巡检：<库根>
资产 128（应写 37，盘上 37）；磁盘 .xmp 41
[FAIL] photos/2026-08-15/MYP0007.xmp 元数据不一致：rating 盘上 3 / 库里 4
[WARN] photos/2026-08-15/MYP0011.xmp 没内容却有文件（陈旧副本）
[INFO] photos/2026-08-15/MYP0012.xmp 外来文件（无 rb:，不动）
汇总：FAIL 1 / WARN 1 / INFO 1
```

* 退出码：`0` 无 FAIL；`1` 有 FAIL；`2` 用法错误或库打不开。
* FAIL 逐条含「资产/文件 + 检查项 + 盘上值 vs 库里值」，便于直接复现到某张照片。
* 不写盘、不修文件；发现问题由人去应用里操作或回报。

## 5. 测试

* 核心模块单测（真实临时库 + 真实临时文件，复用 `xmp/adopt.rs` 的 in-memory catalog 夹具思路）：
  干净库全通过；删文件 → FAIL；改元数据 → FAIL；tamper `rb:latest` → FAIL；
  放一个外来 xmp → INFO；放一个孤儿自家 xmp → FAIL；`_RAW/` 下放自家 xmp → FAIL；
  同名折叠两份 → FAIL；sooc 基文件里出现 `crs:` → FAIL。
* 退出码/打印层不单测（薄层）；`cargo test -p raybend --lib -- xmp::audit` 秒级。

## 6. 纪律自查

* §2.15：是开发/诊断命令行入口，不是应用内用户功能，不进命令注册表。
* §2.12：全部检查复用运行时同一份实现（`collect` + `crs_fragments` + 既有 parse/hash），不写第二套规则。
* §2.8：这就是「可重复执行的验收代码」，真机 GUI 部分仍归人类。
