# M1-6 前半程：导入执行与进度（核心层 + 数据桥 + 进度 store）

完成时间：2026-09-16 11:33:22 CST

## 本次改动的范围

`plans/M1-6.md` 的 **A 批与 B 批（步骤 1–10）** 全部完成并提交；
C 批（画布五帧、进度弹窗、库设置弹窗、导入后刷新）**未完成**，原因见文末「遗留」。

一句话总结：**导入这件事在 Rust 侧已经是完整的**（模版 → 规划 → 复制 → 登记 → 进度），
前端也已能收到进度事件；差的是把进度画出来（弹窗）与收尾。

## 涉及文件

#### 核心层（`crates/raybend/src/import/`，全新模块）

| 文件 | 内容 |
| --- | --- |
| `template.rs` | 模版解析 / 校验 / 渲染：最长前缀匹配、SEQ 宽度即身份、ISO 周、缺值占位 |
| `plan.rs` | 目标路径规划：目录透传、`_RAW/` 分流、重名 `_01…`、序号按「目录 + 宽度」、判重、`Reserved` 续跑 |
| `fsops.rs` | `FileOps` + `Scanner` 两个注入点；`RepoFs`（真）/ `MemoryFs`（测试用）；复制走 `.part` 中转；剩余空间 |
| `progress.rs` | 进度模型：阶段 / 状态 / 计数 / 当前项 / 错误清单截断 / 节流 / 批事务时机 / `BatchHandle` |
| `runner.rs` | 执行器：阶段机、暂停取消原子标志、逐文件 `pending`→复制→登记→`imported`、缩略图入队回调 |
| `sink.rs` | `catalog.db` 落库：`import_runs` / `import_items` / 资产登记（`_RAW/` 折算配对）/ 序号 / 判重集合 |
| `space.rs` | 开工前的空间预检（纯判断 + 轻量估算） |

#### 存储层

`store/migrations/catalog_0003_source_identity.sql`（源身份列 + 部分索引）、
`store/migration.rs`（登记 v3）、`store/assets.rs`（`find_asset_for_group` 公开 + `_RAW` 折算、`set_source`）。

#### 外壳与前端

`src-tauri/src/import.rs`（8 个命令 + `import://progress`）、`src-tauri/src/lib.rs`（注册）、
`src-tauri/src/contract.rs`（契约断言 + 守卫清单）、
`src/api/import.ts`、`src/api/types.ts`（8 个 DTO 镜像）、`src/api/dto-contract.json`（8 条）、
`src/api/dto-contract.test.ts`、`src/features/import/{store.ts,store.test.ts}`。

## 关键决策与理由

1. **每个源目录一个 run，一个 `batchId` 聚合**（`plans/M1-6.md` §3.4）：
   `import_runs.source_root` 只有一列；多目录聚合成一个批次显示，聚合在 **Rust 侧**算
   （前端再算一遍迟早会不一致）。
2. **`_RAW/` 配对要「折算」**：`assets::apply_diff` 的配对键是「同目录 + 同名主体」，
   而 `REPOSITORY.md` §4.1 下 RAW 与位图天生不同目录 —— 直接复用它会把一张照片
   拆成两条资产（网格里两个格子）。改成把 `_RAW/` 折算回位图目录再找资产，
   并且**大小写不敏感**（库里存的是折叠路径，这里踩过一次坑）。
3. **源身份单独两列**（迁移 v3）：现有的 `(volume_serial, file_id)` 是**库内副本**的身份
   （M1-3 的差分/缺失检测靠它），源身份写进去会让库内差分把每个文件当成「被替换过」。
   判重键用**源绝对路径**而不是相对路径（两个源根的同名文件不该互撞）。
4. **续跑 = `Reserved` + `pending` 行**：规划时把自己上次留下、且大小相符的路径当作
   「可用」，否则重名规则会给它加 `_01` —— 同一张照片变两份。
   这条是**测试逼出来的真 bug**（不是设计时想到的）。
5. **磁盘操作全部 `&self`**：执行器要同时拿 `ops` 与 `scanner`，而单测里它们是同一个
   内存假实现（原来的 `&mut self` 借不出来）。真实现本来就只需要 `&self`。
6. **进度百分比在扫描/规划阶段返回 `null`**：没有总数就不编数字，
   界面用不确定进度条 —— 硬凑一个百分比只会先快后慢、最后停在 90%。
7. **缩略图入队走注入回调**：`jobs` 表在 `app.db`，而它的单写者归外壳持有；
   核心 crate 不该认识 `app.db`。
8. **`begin_run` 立刻提交**（其余动作攒到 `commit()`）：不然崩溃之后根本不知道上次在导什么。

## 验证方式（Agent 侧冒烟）

| 项 | 结果 |
| --- | --- |
| `cargo test -p raybend` | **534 项全绿**（新增 134 项：模版 31 / 规划 40 / 文件操作 13 / 进度 10 / 执行器 19 / 落库 14 / 空间 7） |
| `cargo test -p raybend-desktop` | 19 项（含 8 条新 DTO 的契约断言 + 守卫清单） |
| `cargo clippy --workspace --all-targets` | **0 警告** |
| `pnpm test` | **373 项全绿**（新增 17 项：进度 store 16 + 契约 1） |
| `pnpm typecheck / lint:arch / lint:colors` | 全绿 |

单测的形态值得记一笔：**执行器那一批是「内存 FS + 内存 catalog 跑完整轮导入」**
（一个真文件都不碰，整批 19 项跑 0.11 秒），落库那一批跑在真实的临时 `catalog.db` 上。

## 遗留问题（C 批，未完成）

1. **画布五帧没画**（`plans/M1-6.md` §3.7）：`Dialog / 导入进度`（×3 态：进行中 / 错误清单 /
   结束）、`Dialog / 库设置`、`Dialog / 关于 About`。`AGENTS.md` §5.1 要求设计稿先行，
   所以进度弹窗**不该在帧补齐前写**。
   卡点：Pencil 的 `execute` 能跑 JS 片段（返回 `OK`），但**读不到返回值**
   （`console` / `print` 都不存在），我无法回读节点结构来核对；而 `execute.md` /
   `pen-schema.md` 这两份参考文档通过 `pencil_read_skill` 拿不到。
   下一步要么拿到那两份文档，要么由人指定「照现有弹窗改哪几处文字」。
2. **进度弹窗没写**（步骤 12）：`features/import/ImportProgressDialog.tsx` 未创建，
   导入按钮仍只显示「导入中…（M1-6 实现）」占位。
3. **库设置弹窗没写**（步骤 13）：右列的齿轮仍不渲染（M1-5 的 §9.5 待同步清单里记着）。
4. **导入后刷新与「在库中查看」没接**（步骤 14）。
5. **`pnpm smoke:ui` 没加导入断言**（步骤 15 的一半）。
6. **真实样本闭环没跑**：`plans/M1-6.md` §6 的「20~30 张真实照片导入 → 落盘断言」
   要在弹窗能跑之后做（现在只能靠 `cargo test` 的内存实现保证逻辑对）。
7. Windows 侧 `cargo check --workspace --all-targets` **本次未重跑**（M1-5 时是干净的；
   新增的 `src-tauri/src/import.rs` 只用了跨平台 API，但仍该在 Windows 目标上过一遍）。
