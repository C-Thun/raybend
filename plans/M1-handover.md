# M1 交接说明（M1-5 → M1-6 → M1-7）

> **用途**：新会话从这一份接上，一口气跑到 M1 收尾。
> **现状**：M1-1 / M1-2 / M1-3 / M1-4 **已完成并提交**；剩余 **M1-5 / M1-6 / M1-7**。
> **本文档是一次性交接材料**：M1-5 开工后可以删掉，内容该进 `plans/M1-5.md` 与 `plans/M1.md`。
> 建立时间：2026-09-16 03:5x CST（M1-3 收尾后）

---

## 0. 新会话开场（按顺序读完这 5 件）

1. `AGENTS.md` —— 纪律：§2 硬约束、§5.1 设计稿先行、§5.2 实施记录、§5.4 一次只规划一个工作单元、
   §2.8 **冒烟归 Agent、E2E 归人类**
2. `ASSISTANCE.md` —— **只有「一、阻塞中」非空才必须停下来**（当前为空 → 不用停）
3. `plans/M1.md` —— 单元划分与状态（M1-1…M1-7），本文 §2 是它的展开
4. 本文 §1「已就绪资产」—— **别重造已有的东西**（规格、设计稿、代码、工具都在了）
5. 本文 §3 §4 —— 三个待定问题的默认值 + 两件必须先问用户的事

---

## 1. 已就绪的资产（直接用，不要重造）

### 1.1 规格（功能思路全在这里）

| 文档 | 覆盖什么 |
| --- | --- |
| `REPOSITORY.md` §2 | 库布局：库根 = `catalog.db` + `photos/`（无 `repo.json`）；库身份与多路径 |
| `REPOSITORY.md` §3 | **导入模版**（变量、前缀匹配）、序号计数（按目录+位数）、重名加 `_XX`、位图/RAW 分流、目录透传 |
| `REPOSITORY.md` §4 | **导入执行**流程、避免重复导入、**§4.4 进度弹窗规格**（阶段/百分比/计数/当前项/错误清单/暂停·继续·取消/结束摘要） |
| `REPOSITORY.md` §6 | 五个待确认（本文 §3 已给默认值） |
| `design/main.md` §3 | 导入工作区三列结构；§7 待决；§9 待同步画布清单；§6 画布 API 约束 |
| `DESIGN.md` | 视觉唯一事实来源（令牌、密度、状态色、`easy copy`/`easy destroy`、按时间分组、Tile 流换行算法） |
| `AGENTS.md` §11 | **术语表**（勾选 vs 选中、移除 vs 排除 vs 删除、`shortpath`、`easy copy`…）——写代码/文案前必看 |
| `BROWSE.md` | 浏览模式规格（**M2 的事，M1 不碰**，别顺手做） |

### 1.2 设计稿（Pencil）

* `design/main.pen` —— 导入工作区、外壳、状态速查、组件（`design/main.md` 有逐帧 id 索引）
* `design/browse.pen` —— 浏览七帧（M2 用）
* ⚠️ **Pencil 只能操作「当前激活」的那个 `.pen`**（`filePath` 参数被忽略）。开工画图前先请用户切文件。

### 1.3 代码（M1-2/M1-3 交付的底座，M1-5/M1-6 直接调）

```text
crates/raybend/src/
  store/       db.rs（AppDb / CatalogDb 门面：read / write / write_tx / flush）
               assets.rs（list_files / apply_diff / apply_exif / counts / is_online / insert_asset）
               tags.rs（ensure_tag / set_tags / tag_usage / 跨库词典对齐）
               repository.rs / migration.rs（Backups / DbKind{App,Catalog,Thumbs}）/ pool.rs / writer.rs
               file_id.rs（FileId::try_read → 改名不变的身份）/ path_semantics.rs（raw/normalized/folded）
  media/       kind.rs（扩展名/类型/垃圾文件）/ scan.rs（流式扫描 + Cancel）
               diff.rs（变更追踪：DiffPlan / Evidence::{Identity,Path,Heuristic}）/ exif.rs
  thumbnail/   render.rs（grid 384 / strip 192、两段式缩放、占位图）
               cache.rs（ThumbCache 键 / get / put / touch / gc / stats；**未入库的源文件也能用**）
               worker.rs（enqueue / claim_next / run_pending / produce / requeue_running / queue_stats）
src-tauri/src/db.rs   DbState（Mutex<Option<AppDb>>）+ 命令 app_paths / db_status / setting_get / setting_set
                      warm_up（启动预热，失败不阻止窗口）
```

**前端已有的**：`src/components/ui/`（17 个基础组件）、`src/shell/`（三行外壳 + `flow.ts`/`store.ts`）、
`src/features/exif-strip/`、`src/api/{window,tauri-env}.ts`、`src/i18n/{zh-CN,en-US}.ts`、
`src/lib/{appearance,build-info,clipboard,easy-destroy,format,release-plan,tree}.ts`、`src/dev/KitchenSink.tsx`。
**还没有的**：导入工作区、库列表、与 Rust 侧的数据桥（M1-5 要新建 `src/api/db.ts` 一类）。

### 1.4 工具与命令（都已实测）

| 命令 | 说明 |
| --- | --- |
| `pnpm dev` / `build` / `typecheck` / `test` | 前端；`test` = `node --test`，212 用例 |
| `pnpm lint:colors` / `lint:arch` | 硬编码色值 / 分层依赖（features 之间禁止互相 import） |
| `pnpm shot` / `smoke:ui` | 截图 / UI 冒烟（页面 404、控制台报错） |
| `pnpm tauri dev` | WSL 侧窗口（经 WSLg） |
| `cargo test -p raybend` | **337 用例，约 3.5 秒**（重负载一律 `#[ignore]` 或进 examples） |
| `cargo clippy --workspace --all-targets` | 必须 0 警告 |
| **Windows 侧 check**（详见 `AGENTS.md` §5.3） | `export CARGO_TARGET_DIR='C:\rb-target\raybend'; export WSLENV='CARGO_TARGET_DIR'; cmd.exe /c 'pushd \\wsl.localhost\Ubuntu-24.04\home\andares\repos\c-thun\raybend & cargo check --workspace'` —— **Windows 侧比 WSL 多暴露 `cfg(windows)` 专属警告，改动核心 crate 后要跑一次** |
| `cargo run --example {scan-smoke,refresh-smoke,thumb-bench} -- /mnt/c/src/tmp/pic` | 真实样本上的扫描 / 全链路 / 吞吐实测 |
| 样本目录 | `/mnt/c/src/tmp/pic`（300 文件 = 158 JPG + 142 RW2，4.2 GB，142 组同名配对） |

---

## 2. 剩余三步

### 2.1 M1-5 导入工作区（**先写 `plans/M1-5.md` 并走 plannotator 评审**）

**交付**（详见 `design/main.md` §3 与 `design/main.pen`）：

| 块 | 内容 |
| --- | --- |
| 左列（三段 + 两个 `SplitHandle`） | `最近`（最近 50 个导入目录，圆形勾选框 + 行内移除）／ `来源`（驱动器 + 目录树）／ `已选目录`（横条列表） |
| 中列 | 照片网格（`Tile` 流换行，**按时间分组**模式：日组标题 + 片标题 + 「全选当天/此段」）＋ `GridControlBar`（计数 / 当前目录+文件名 / 按时间 / 缩放滑块） |
| 右列 | 库列表（库卡片 + 新建库 + 导入按钮 + 避免重复导入勾选项） |

**要用到的现成件**：`media::scan`（流式 `ScanEvent`，先喂树再喂网格）、
`thumbnail::cache::cache_key(identity, folded)` + `render`（**源目录里还没入库的照片也能出缩略图**）、
`components/ui/*`、`DESIGN.md` §12.7 按时间分组规则（阈值 1 小时可配）。

**要新做的**：`src/api/db.ts`（前端 ↔ Rust 桥，用 specta 风格手写也行）+ 若干 Tauri 命令
（列目录 / 扫描流 / 取缩略图 / 建库 / 登记库路径 / 列库）+ `src/features/import/*` 组件。

**要问用户的两件事**：

1. **虚拟化方案**（`AGENTS.md` §9：新增顶层框架或大型依赖要先讨论）——
   目前 `package.json` 里**没有任何虚拟化库**。候选：`@tanstack/solid-virtual`（Solid 官方示例常用）、
   自研固定行高虚拟列表（网格是均质 tile，`DESIGN.md` §12.6 已有列数算法）、`virtua`。
   建议：先问，别自己装。
2. 三个待定问题的答案（本文 §3）。

### 2.2 M1-6 导入执行与进度（**前置：进度弹窗的设计稿 + `REPOSITORY.md` §6 三问**）

**交付**：模版求值（含 `:SEQ000` 前缀匹配与位数独立计数）→ 目标路径计算 → 重名 `_XX` 兜底 →
位图/RAW 分流 → 目录透传 → 复制/移动（**第一阶段默认复制**）→ `import_runs` / `import_items` 落库 →
**进度弹窗**（`REPOSITORY.md` §4.4）+ 暂停/继续/取消 + 错误清单 + 结束摘要 + 避免重复导入（按文件身份）。

**复用**：`thumbnails::worker` 的任务队列模式（`jobs` 表已在 `app.db`）、单写者批量事务
（`CatalogDb::write_tx`）、`store::ids`、`seq_counters` 表（schema 已建好）。

**要新做的**：`media/import.rs`（或 `import/` 模块：模版求值 + 目标路径 + 冲突解决 + 执行）、
Tauri 命令（开始/暂停/取消 + 进度订阅）、`features/import/ImportProgressDialog.tsx`。

**⚠️ 设计稿先行**：进度弹窗的 `.pen` 帧**还没画**（`design/main.md` §7 #10）。
顺便一起做掉积压的三件：`PhotoThumb` 的 `$surface-track` → `$surface-main`、
新建 `Dialog / 关于 About` 帧、`States / 状态速查` 补「窗口关闭键悬停红」。
**这需要用户先把 Pencil 切到 `main.pen`。**

### 2.3 M1-7 M1 收尾

| 事项 | 做法 |
| --- | --- |
| 崩溃恢复演练 | 杀死进程 / 断开库盘 → 重开不崩、任务续跑、缺失标 `missing_since`（`requeue_running` 已实现） |
| 性能基线 | `thumb-bench` 的数字 + 导入 300 张的耗时，写进实施记录 |
| 实施记录 | `implementations/YYYY-MM-DD_*.md`，**首行精确到秒** |
| **人类验收清单 4 项** | 见 `PLAN.md`；到点**必须提醒用户**，不代替他确认 |
| 收尾 | **M1 完成后停下交付，绝不自行开始 M2**（浏览模式的规格/设计虽已就绪，也要等） |

**已知缺口**（M1-3 遗留，可顺手补）：worker 是**单线程**消费队列；
多线程池预期 4× 提升（队列本身已支持并发认领）。也算 M1-7 的性能项之一。

---

## 3. 三个待定问题（**默认值已定；用户未答就按默认，并写进实施记录**）

| # | 问题 | 默认 | 出处 |
| --- | --- | --- | --- |
| 1 | `:CDAY` / `:CWEEK` 的宽度 | **2 位**（`08`、`15`、`33`）—— 用户曾说 4 位，但 4 位会产生 `2026-0008-0015` 这种怪目录 | `REPOSITORY.md` §6.2 |
| 2 | 目录透传时模版的**作用范围** | **只作用于文件名**（透传目录原样保留，不在每层重套模版，避免嵌套日期目录） | §6.3 |
| 3 | 序号回绕后撞上已有文件 | **继续往后找空位**，仍冲突则加 `_XX`（用户说过「重名加尾号解决」） | §6.4 |

另两条已有倾向、无争议：库 ID 用**现有实现**（16 位可排序 base62，见 `store/ids.rs`；UUID v7 互操作登记在 `FUTURE.md` G15）；
`photos/` 目录名 M1 **固定**（可配置登记 `FUTURE.md` G14）。

---

## 4. 本轮踩过的坑（别再踩）

| 坑 | 后果 | 记住 |
| --- | --- | --- |
| Pencil **只能操作当前激活的 `.pen`** | 跨文件读写静默无效 | 画图前先请用户切文件 |
| Pencil 新建画布**先建节点后建变量** | `$surface-*` 被静默烤成 `#000000`（看着像「背景纯黑」的 bug） | **先 `SetVariables` 再建节点** |
| `rusqlite` 没有 `u64` 的 `ToSql`/`FromSql` | 编译不过 | 按位解释成 `i64` 存（`volume_serial` 就是这么办的） |
| 写者闭包要 `Send + 'static` | 借用数据编译不过 | 闭包里 `move` 所有权（`Vec`/`PathBuf` 克隆一份） |
| **读池连接是 `query_only`** | 在读路径里写会报 `readonly database` | 任何写都走 `write`/`write_tx`；`cache::get` 因此是纯读、frecency 用 `touch` |
| `cargo clippy`（WSL）看不到 `cfg(windows)` 的死代码警告 | Windows 侧 7 条警告 | 改核心 crate 后跑一次 Windows check |
| EXIF `DateTimeOriginal` **没有时区** | 直接存 UTC 会被本机时区篡改 | 已定口径：有 `OffsetTime*` 就换算，没有就原样墙上时间 + `taken_at_offset_min = NULL` |
| FTS5 `trigram` 分词器 **< 3 字符搜不到** | 中文两字词搜不到 | 短词必须 `LIKE` 兜底（已有测试与注释） |
| 直接 Lanczos3 缩 6000px → 384px | 205 ms/张 | 用 `render::resize_for_thumb`（两段式，77 ms） |
| 大图整读文件取 EXIF | 46 ms/张 | `media::exif::read_file` 已改成「先读 1 MiB，失败再整读」（7.4 ms） |
| `web_search` 工具会弹交互窗口卡死会话 | —— | **禁用**；联网用 Firecrawl / `searxng-search` skill / `fetch_context` |
| `.pi-lens.json` 里编辑器格式已关（`format.enabled: false`） | —— | 别打开，否则污染 diff |

---

## 5. 验证与收尾纪律（每次都一样）

1. `cargo test -p raybend` 全绿且**保持秒级**；单元测试与实现一起交付（含边界：空/单元素/上下限/非法值/Unicode/超长/大小写/并发/溢出）
2. `cargo clippy --workspace --all-targets` **0 警告** + Windows 侧 `cargo check` **0 警告**
3. `pnpm typecheck` / `test` / `lint:colors` / `lint:arch` 全过；改了前端要 `pnpm build`（dist 是编译期嵌入的）
4. 真实样本上跑一遍冒烟（examples），数字记进实施记录
5. **实施记录**：`implementations/YYYY-MM-DD_<简述>.md`，首行精确到秒
6. 报告里**分清**「已验证（冒烟）」与「未经人类验证」——GUI 观感、多显示器/DPI、真实照片库、
   性能体感、色彩正确性一律归人类
7. 提交信息用中文 `<type>: <subject>`；**`git push`/打 tag/发布永远由人做**

---

## 6. 交接时的仓库状态

* 分支与工作区：见 `git log --oneline -1` 与 `git status`（交接时 HEAD 为 M1-3 收尾提交，工作区干净）
* M1-3 相关提交：`77b1711`（改名+色标）→ `c548cc5`（schema v2 + tags）→ `1cf2882`（扫描）→
  `850fad1`（变更追踪）→ `e549452`（EXIF）→ `806d59d`（缩略图）→ `9afc82b`（实测+两段式）→
  `6352b62`（记录+勾选）→ `77bf0c7`（Windows 警告清理）
* 实施记录：`implementations/2026-09-16_M1-3_schema-v2-scan-diff.md`、
  `implementations/2026-09-16_M1-3_exif-thumbnails-bench.md`
* 计划：`plans/M1-3.md`（7/7 完成）、`plans/M1.md`（M1-3 行已标完成）
