# M1-3 后半：EXIF、缩略图管线与吞吐实测

完成时间：2026-09-16 03:20:07 CST

对应计划：`plans/M1-3.md` 的步骤 **D / E / F / G**。前半（A schema v2 / B 扫描 / C 变更追踪）
见 `implementations/2026-09-16_M1-3_schema-v2-scan-diff.md`。
**本单元（M1-3）至此全部完成。**

---

## 1. 本次改动的范围

| # | 做了什么 | 提交 |
| --- | --- | --- |
| D | EXIF 抽取与拍摄时间来源判定（`media::exif` + `store::assets::apply_exif`） | `e549452` |
| E | 缩略图管线：`thumbnail::{render, cache, worker}` + 第三种库 `thumbs.db` | `806d59d` |
| F | 真实照片上的吞吐实测（`examples/thumb-bench.rs`）+ 据数据做的两段式缩放优化 | `9afc82b` |
| G | 本记录 + 勾 `plans/M1.md` | 见本文 |

---

## 2. 涉及文件

### 新增

| 文件 | 内容 |
| --- | --- |
| `crates/raybend/src/media/exif.rs` | EXIF 读取（容器 + 裸 TIFF）、拍摄时间来源判定、文件名时间识别、GPS、时区 |
| `crates/raybend/src/store/migrations/thumbs_0001_init.sql` | 缩略图缓存库 schema（`DbKind::Thumbs`） |
| `crates/raybend/src/thumbnail/render.rs` | 解码 → 按朝向摆正 → 两段式缩放 → JPEG q82；RAW 占位图 |
| `crates/raybend/src/thumbnail/cache.rs` | 缓存库门面 + 键 / 存取 / frecency GC / 统计 |
| `crates/raybend/src/thumbnail/worker.rs` | 任务队列 worker（认领 / 退避重试 / 崩溃恢复 / 取消） |
| `crates/raybend/examples/thumb-bench.rs` | 吞吐实测（分阶段耗时 + 缩放算法对比） |

### 修改

* `store/time.rs`：新增 `from_civil` / `from_civil_with_offset`（`civil` 的逆函数），
  并带**日历往返校验**（挡住 `2026-02-31` 这类「格式合法、日历非法」的值）
* `store/migration.rs`：新增 `DbKind::Thumbs` 与 `THUMBS_MIGRATIONS`
* `store/db.rs`：`migrate_file` 改为 `pub(crate)`（缓存库要复用它）
* `store/assets.rs`：`apply_exif`（只写「从文件推出来」的字段）
* `thumbnail/mod.rs`：从「只有说明」变成真正落地
* `Cargo.toml`：新增 `kamadak-exif 0.6`、`image 0.25`（只开 jpeg/png/tiff）

---

## 3. 关键决策与理由

### 3.1 EXIF：三条口径写进代码与迁移注释

1. **拍摄时间没有时区**（`DateTimeOriginal` 只是相机钟表读数）：
   有 `OffsetTimeOriginal` 就按它换算成 UTC 毫秒并记下偏移；没有就**把墙上时间原样当 UTC 存**、
   偏移留空，显示端遇空按「无偏移」渲染 —— 用户看到的数字与相机/其它软件一致。
2. **读不到 EXIF 不是错误**：退到文件名 → 文件修改时间，并把**来源**记进 `taken_at_source`。
3. **只写「从文件推出来」的字段**：用户写的作者/描述/评级/色标绝不被覆盖。

调研依据：RapidRAW 用同一个 `kamadak-exif` 读 EXIF（它另引 `little_exif` 只用于**写回**），
RAW 元数据走 `rawler::raw_metadata()` —— 本项目 RAW 元数据留到 `FUTURE.md` B8 那一步。

### 3.2 「先读开头 1 MiB，失败再整读」

EXIF 就在 JPEG 的 APP1 里，而 RAW 的 IFD 绝大多数也在开头。第一版实现直接 `fs::read` 整个文件，
实测 158 张照片用了 **7273 ms**（46 ms/张）；改成先读 1 MiB 后是 **1171 ms**（7.4 ms/张）——
**快 6 倍**，在 9p/SMB 这类慢盘上是数量级的差别。读不到（IFD 在文件尾部）才回退整读。

### 3.3 缩略图缓存：位置、键、回收

* **位置** `<app data>/cache/<repository_id>/thumbs.db` —— 库根只放 `catalog.db` + `photos/`。
* **键** `(cache_key, size_class, render_sig)`：
  * `cache_key` **优先用文件身份**（改名/移动后仍然命中），读不到身份才退回折叠路径，
    首字节 `'i'`/`'p'` 判别 —— **未入库的源文件也能共用同一套缓存**（M1-5 要用）。
  * `render_sig` 含管线版本：改算法只改版本号，旧缓存自动成孤儿被 GC 收走，**不需要数据迁移**。
* **回收**用 frecency（`pinned → hits → last_used_at`），**钉住的永不淘汰**。
* **不做快照备份**：派生数据，最坏就是重算（红线：删掉整个缓存目录后功能降级但完全可用，
  已有测试守着这条）。

### 3.4 两段式缩放（本次最有价值的一条实测结论）

实测（30 张 6000×4000 JPG，release）：

| 做法 | 每张 |
| --- | --- |
| 直接 Lanczos3 | **205 ms** |
| 直接三角滤波 | 73 ms |
| 直接盒式 | 46 ms |
| **两段式（盒式 → 2× 目标 → Lanczos3）** | **77 ms** |

把 6000px 直接缩到 384px 时，Lanczos3 的核要跨 15 个像素取样，代价极高。
先用盒式（整数快路径）降到 2× 目标，再让 Lanczos3 精修最后一段 ——
观感与直接 Lanczos3 几乎一致（大幅缩小本就丢高频），速度快 2.7 倍，
且与三角滤波同速但质量更好。整条管线因此从 82 秒降到 60 秒。

**结论：不需要上 SIMD 缩放库**（计划里留的「要不要 `fast_image_resize`」现在有答案了）。

### 3.5 RAW 走占位图（本阶段有意为之）

占位图是**程序画的**，不依赖任何字体文件（手写 5×7 点阵 R/A/W），
而且与原图走**同一条编码路径** → UI 不用分情况处理。
实测占位图 **1.8 ms/张**，对比解码一张位图 ~200 ms —— 这就是推迟 RAW 解码的价值。
用户样本里 142 张 RAW **全部**有同名 JPG（`scan-smoke` 实测 0 个「只有 RAW」），
所以这个取舍在当前样本上零观感损失。

### 3.6 worker 的三个阶段刻意分成三次事务

```text
① 认领（写 app.db）：pending → running，attempts += 1
② 干活：不持有 app.db 写锁（渲染是慢活），只写 thumbs.db
③ 结账（写 app.db）：done / 失败退回 pending
```

在 `app.db` 的写事务里渲染会把单写者堵住几秒，导入、评级、打标签全部卡住 ——
这是 `AGENTS.md` §6.4 单写者纪律的直接推论。

**失败退避 30 秒**：没有它，一条永久失败的任务（文件真的没了）会在同一轮里把重试次数
一次烧完，还会让整批任务的进度看着像卡住。退避期间 `claim_next` 不认领它。

### 3.7 两条被测试逼出来的修正

* **`cache::get` 必须是纯读**：读连接是 `query_only` 的（`store::pool` 的纪律），
  在里面更新命中计数会直接报 `readonly database`；而且浏览时一秒滚过几十张，
  「看一眼写一次库」也太重。→ frecency 拆成独立的 `touch`，由 UI 侧批量刷。
* **`produce` 要写缓存**，所以不能挂在读池上 → `run_pending` 走 `ThumbsDb::write`。

---

## 4. 验证方式

### 已跑的命令与结果

| 命令 | 结果 |
| --- | --- |
| `cargo test -p raybend` | **337 passed / 0 failed**（约 3.5 秒；另有 2 个文档测试） |
| `cargo clippy --workspace --all-targets` | **0 警告** |
| `cargo fmt --all` | 无残留 |
| `examples/scan-smoke` / `refresh-smoke` / `thumb-bench` | 均在真实样本上跑通（下节） |

### 真实样本实测（`/mnt/c/src/tmp/pic`，300 文件 = 158 JPG + 142 RW2，4.2 GB）

```text
扫描        300 文件 · 0 跳过 · 142 组位图↔RAW 配对 · 0 个「只有 RAW」
完整链路    建库 16ms → 首轮 158 资产/300 文件 787ms → 次轮变化 0 条（幂等）
EXIF        158 个资产全部读到 · 时间来源 100% exif · 7273ms → 1171ms（1 MiB 探测）
缩略图      300 张 60 秒（5.0 张/秒 · P50 174ms · P95 310ms）
            缓存命中 3.5 ms/张 · 缓存体量 5.8 MB（19 KB/张）
```

⚠️ WSL 下 `/mnt/c` 走 9p，绝对耗时只能当**上界**参考（读文件 45 ms、解码 114 ms
在本地 NTFS 上会低得多）；本次结论主要建立在「阶段占比」与「算法之间的相对差距」上。

### 边界覆盖（单测）

* EXIF：裸 TIFF（RW2 家族）与容器两种路径、无 EXIF、损坏文件、缺文件、
  相机没设时间（全 0）、烂日期（含 `2026-02-31`）、时区（有/无/格式错）、GPS（南纬/西经/0,0/半条）、
  文件名时间各种写法（紧凑/分隔/毫秒后缀/前缀数字干扰）、探测长度 0/8/64/256 的回退路径；
* 时间：`from_civil` 与 `civil` 往返、闰年（2024/2000/1900）、月日越界、漏斗秒、1970 前后；
* 缩略图：JPEG/PNG 解码、保持比例、不放大、非图像返回 `None`、朝向 1..8（含缩放前的顺序）、
  占位图是合法 JPEG 且真的画出了字、两段式与直接缩放形状一致；
* 缓存：身份键 vs 路径键不互撞、全 0 身份退回路径键、同键覆盖、尺度与签名各自独立、
  `get` 纯读不改计数、frecency GC 先淘汰「命中少 + 最久没用」、钉住的永不淘汰、
  老签名回收、统计分组、真实文件路径（迁移 + 写者 + 读池）打开/重开、**删缓存目录只剩重算**；
* worker：载荷往返与坏载荷（判失败不无限重试）、优先级、失败退避与上限、
  **崩溃恢复**（running → pending）、不抢别的工种、取消、单轮限额、
  真实 `ThumbsDb` 写者路径。

---

## 5. 遗留问题（交给后续单元或波次）

1. **worker 目前是单线程消费队列**（多线程池 `min(4, cores)` 未做）。
   队列本身已支持并发认领（`claim_next` 是单条 `UPDATE ... RETURNING`），
   只差「起 N 个线程各跑 `run_pending`」这一层。**这是下一步最值得做的性能项**：
   实测单线程 5 张/秒，四线程在本地盘上预期能到 ~15 张/秒。
2. **RAW 真实解码**仍不做（用户批准），登记在 `FUTURE.md` B8；
   截至今天「只有 RAW」的照片会显示占位图。
3. **`SCREEN` / `FULL` 两个尺度**未做（视口分辨率与 1:1 预览），属于浏览里程碑之后。
4. **缓存的容量上限与按库覆盖路径**（`AGENTS.md` §6.5 的 10GB 策略、随外接盘）未接线：
   `gc()` 已实现，缺的是「谁来调、阈值从哪来」（设置面板）。
5. **缩略图任务尚未与导入流程接线**（M1-6 负责：导入后批量 `enqueue` + 进度弹窗显示）。
6. **`store::assets::apply_exif` 尚未接进「刷新」流程**：目前 `refresh-smoke` 演示里手工调；
   正式的「扫描 → 差分 → EXIF → 缩略图」编排属于 M1-5/M1-6。
