# 导入侧写元数据 + 老库回填（「库里时间全是 NULL」的真根因）

完成时间：2026-09-18 16:23:48 CST

计划：`plans/M2-W2.md` 阶段 0.5（人类 2026-09-18 定「并入 W2 第一小步」）
相关：`plans/M2-W1-round3-fixes.md` 的 `#1-4`、`AGENTS.md` §6.4、`REPOSITORY.md` §4.3

---

## 一、根因：**导入根本不写 EXIF**

`import/sink.rs` 的 `register` 原本只做三件事：找/建资产、插 `asset_files` 行、记来源路径。
`store::assets::apply_exif`（唯一会写 `taken_at` / 器材 / 曝光 / 尺寸的入口）**只在测试里被调用过**。

于是那段时间导入的库：`taken_at`、`camera_*`、`lens`、`iso`、`width/height` 全是 NULL ——
浏览的时间分组、按时间筛选、右栏信息栏都因此是空的。人类报的「库里的时间全是 NULL」就是它。

（第三轮修的是**源目录扫描**那条路（导入网格显示的那个值），DB 这条从来没人走。）

## 二、改了什么

| 文件 | 改动 |
| --- | --- |
| `crates/raybend/src/import/sink.rs` | `register()` 里读**库内副本**的 EXIF（写事务之外），把 `ExifData` + `TakenAt` 塞进 `Op::Register`；`apply()` 里调 `assets::apply_exif` 落库 |
| `crates/raybend/src/import/sink.rs` | 新增 `keep_better_taken()`：**时间不许降级**（`EXIF > 同名位图继承 > 文件名 > mtime`） |
| `crates/raybend/src/media/exif.rs` | 新增 `source_rank()`（上面那条规则的唯一实现，sink 与 backfill 共用） |
| `crates/raybend/src/store/backfill.rs`（新） | 老库回填：候选 = `taken_at IS NULL` 的资产，重读 EXIF 并写回 |
| `crates/raybend/examples/backfill-metadata.rs`（新） | 命令行入口（`<库根> [--dry-run]`） |
| `crates/raybend/src/store/mod.rs` | 登记 `backfill` 模块 |

### 三条关键决定

1. **元数据只由位图写**，RAW 只在「这个资产没有位图」时才写。
   同一张照片的位图与 RAW 共用一个 `assets` 行，而 RAW 能读到的字段往往更少 ——
   让后登记的那个覆盖，会把位图读到的镜头名之类冲成空。
2. **时间不许降级**：登记顺序由规划决定、不保证；RAW 那侧只读到 mtime 而位图读到真 EXIF 时，
   「后写的赢」会把好时间冲掉。按来源定级，只允许往上换。
3. **兜底的 mtime 用源文件的**（`file.mtime_ms`）：库内副本的 mtime 是**复制时间**，
   拿它当拍摄时间是错的，而且与导入网格显示的值对不上。

### 回填的边界

* 只处理 `taken_at IS NULL` 的资产 —— 已经有时间的一行都不动；
* 写事务里**再确认一次**「这一行还是空的」，回填期间用户改了就不覆盖；
* 写的是 `apply_exif` 那一组「从文件推出来」的列，**绝不碰用户数据**（评分 / 标签 / 作者 / 描述 / 锁）；
* 文件不在磁盘上（离线盘 / 被删）→ 记账、原样不动；
* 一个资产多个文件时**以位图为准**（RAW 只在没有位图时用）。

## 三、验证

```text
cargo test -p raybend                       778 passed / 1 ignored（新增 8 条 backfill 单测）
cargo clippy --workspace --all-targets      0 warning
```

**真文件演练**（`/mnt/c/src/tmp/pic` 的 4 对 JPG+RW2，导进 `/tmp/rb-drill/lib`）：

```text
cargo run -p raybend --example import-smoke -- /tmp/rb-drill/lib /tmp/rb-drill/src
  → 4 条资产（每个资产 = 位图 + _RAW/ 里的同名 RAW），全部带上：
    taken_at=2026-09-12 16:54:28 exif off=+480 cam=Panasonic DC-G9 f=12 F=2.0 exp=16.7ms iso=500 3712x2784

把元数据清成「修复之前的样子」（模拟老库）后再跑回填：
cargo run -p raybend --example backfill-metadata -- /tmp/rb-drill/lib
  → 候选 4 / 补上 4 / 仍缺 0 / 文件不在磁盘 0；四条全部恢复

再跑一次（--dry-run）：0 张没有拍摄时间 —— 幂等
```

## 四、遗留

* **UI 入口没做**：回填现在只有命令行（`cargo run --example backfill-metadata`）。
  库设置里加一个「重新读取元数据」的按钮等真有需求时再接（IPC 命令 + 进度）。
  —— 计划里说的「走单写者 actor」已经满足：回填走的是 `CatalogDb::write_tx`。
* 老库里**已经有时间但来源是 mtime** 的行（如果将来出现）不在候选里 ——
  「要不要把 mtime 来源的也重读一遍」是一个独立的口径问题，需要时再定。
* 用户自己的老库（`测试库`）还没跑过回填 —— **要动用户数据，等他们点头**。
