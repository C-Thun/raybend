# import tiles：清单层 / 队列层 / 界面层（按时间分组的实时构建）

> 完成时间：**2026-10-12 01:00:42 +0800**
> 规格：`specs/source-time-inline.md`（崔总 2026-10-11 定方案 + 后续四条修正）。
> 这一轮把上一轮写歪的两处（「另开一条全目录补读流程」与「保留界面逐张拉」）推倒重做。

---

## 1. 做了什么（按方案的每一步）

### 第 0 步 · 清单层

* `SourceItem.taken_at` 的兜底次序改为 **文件名 → `min(创建时间, 修改时间)`**
  （`crates/raybend/src/media/source.rs::fallback_taken_at`）。
  为什么取小：复制过来的文件 `created` 是复制那天、`mtime` 才是拍摄那天 ——
  取小才与「按时间」的正确顺序一致，界面显示的日期也不会比排序看起来晚一大截（崔总原话场景）。
* 新增 `TakenAtSource::FileCreated`（含 `source_rank` 次序：exif 4 > sibling 3 > filename 2 >
  file_created 1 > mtime 0），DTO/前端联合类型/契约测试同步。
* 这一层**不打开任何文件**：`created` 与 `mtime` 都是 `read_dir` 顺手拿的
  （`media::scan` 早就读了 `created`，只是以前没用）。

### 第 1 步 · 队列层

* 前端缩略图队列：**固定 2 个消费者**（`photo-grid/store.ts` 的 `createThumbQueue({concurrency: 2})`）；
  进目录时**只把「缓存里没有的」推进队列**（`requestTimes` → `syncTimes` 返回 miss 名单 → `thumbs.enqueue`）；
  滚出屏幕**不再撤销**（`source.ts` 的 `cancelThumb` 变成空实现，注释写明为什么）。
* 屏幕只做一件事：把窗口内的提到队首（既有的 `prioritizeVisible`，未改）。

### 第 2 步 · 一次读拿全

* 新增 `media::exif::read_bundle_for(path)`：**一次开文件、读一段头**，同时解出
  `(ExifData, Vec<ExifField>)`（头一段不够才退回既有两个入口，语义不变）。
* `src-tauri/src/thumbs.rs` 的库外分支：**只有真渲染那一支**（`rendered` 标志）才做这一次读，
  把「拍摄时间 + 显示尺寸/方向 + **整块 EXIF**」写进该目录该张图的缓存
  （`MetaCache::remember`）。命中内存缓存时一次也不多花。
* 缓存条目 `CachedMeta` 现在携带 `Option<CachedExif>`（`ExifData` + 字段表）；
  新增 `MetaCache::exif_for`（按身份取，零 I/O）与 `forget_dir`。

### 第 3 步 · 界面层

* 分组结构改成**按天维护、局部增删**（`photo-grid/store.ts`）：
  `dayIds`（每天一张 id 列表）/ `dayOrder` / `unknownIds` + `groupingTick`；
  `placeTime(id, ms, source, offset)` 只做三件事：**建新分类 / 从旧分类移除（空了删分类）/ 加入新分类**，
  时间没变直接返回 false（什么都不写）。
* `grouping()` / `displayItems()` 变成**带缓存的派生**（按 `groupingTick` + 阈值失效），
  不再「每读一次就把 306 张重排一遍」；单天内部仍复用 `groupByTime`（规则唯一实现），
  分天键抽成 `lib/time-group.ts::dayKeyOf` 供增量维护与整表分组共用。
* 时间**不写回 `items`**（那是整表信号 → 会引发整表重渲染）：真相放在
  `timeOf` / 分组结构里；「只升不降」用它判断。
* 「随构建实时重排」的触发：`requestTimes`（攒批、串行、同一张不重复排队）由三处驱动 ——
  进目录 / 滚停（`noteVisible` → `collectFromRenders`）/ 300ms 节拍（只在浏览器里起，
  靠 `thumbs.readySeq` + `drainReady` 拿「刚出图的那几张」）。**每次都是零 I/O 的缓存查询**。

### 第 4 步 · 缓存生命周期

* 按目录建树（既有设计）；新增 `dir_meta_forget` 命令 + 前端 `dirMetaForget`，
  在「源不可用（拔盘/卸载）」时调用 —— 下次插回来重新读，不拿旧盘的信息当真相。

### 后续微调（同日）

* **两层都按升序（先到后）**（崔总 2026-10-12）：`groupByTime` 的日期层原来排的是**倒序**，
  我增量那边的两处也跟着倒序 —— 一并改成升序，并写进 `memory/DESIGN.md` §12.7 与规格：
  **增量维护与整表分组必须同一方向**。受影响的 4 个测试（`time-group` × 3、`photo-grid/rows` × 1）同步改口径。
* **export 左列「最近」列表的渐隐位置**（崔总同日报）：根因是包 `PathText` 的那层少了 `flex-1`
  —— 文字只按内容宽排，渐隐落在文字尾巴上、右侧空一大截。补上后文字吃满可用宽、渐隐贴住删除按钮
  （列表宽度可调时按钮位置会动，这样才跟得住）。

### 顺手砍掉的

* `source_times` 命令、`TIMES_BATCH`、前端 `readSourceTimes`、`SizeClass` 无关的旧钩子、
  每条查询里的「按窗口排序」（零 I/O 之后顺序无意义）、`timesRevision` 取消逻辑（改为「关掉即停」）。

---

## 2. 验证

| 项 | 结果 |
| --- | --- |
| `cargo test --workspace --offline` | ✅ **1513 + 134**（新增：`read_bundle_for` 相关、`cached_only`/`remember`/`exif_for`、兜底次序 `min(created,mtime)`、半条元信息不算尺寸命中） |
| `pnpm test` | ✅ **1296**（新增/改写：只查缓存不读文件、无变化不写信号、只升不降、一对 JPG/RAW 不拆块、关掉即停） |
| `pnpm typecheck` / `lint:colors` / `lint:arch` / `lint:i18n` / `pnpm build` | ✅ 全过 |
| 真机（Windows debug） | **未测** —— 按崔总要求 **E2E 归人类**；已重建 `C:\rb-target\raybend\debug\raybend-desktop.exe` 供测试 |

给 E2E 的测试点（规格 §4 同步）：

1. 同一目录冷启动，**按时间 ON 与 OFF 出图速度应接近**；
2. 按时间 ON：框架**立刻**出现（兜底时间），图片逐张出来时**分组跟着局部重排**、不顿；
3. **滚屏顺**（这条路上不该再有长命令；
   曾经的「滚不动」= 8 在飞 + 整目录一次性灌队列 + 整表重排，全部已改）；
4. 滚到任何位置都**不该再从无到有**（队列一直在吃，不只是屏幕内）；
5. 切走再回：时间**从缓存直接来**（框架直接是真时间）；
6. 拔盘 → 再插回来：该目录缓存忘掉、重新读；
7. 点/停到 tile 上：flowinfo 的 EXIF **立刻出现**（读的是出图时留下的那份缓存）。

---

## 3. 走过的弯路（同一天内，记下来免得再犯）

1. 上一轮把「把时间整合进读文件信息」做成了**另开一条全目录补读流程**（8 × 48 的
   `dir_meta_ensure`）—— 纯额外 I/O、和出图抢磁盘，真机实测 ON 比 OFF 慢一个数量级。
2. 上一轮还**保留了界面逐张拉**的投递方式，并且把整目录一次性压进队列 → 主线程被数百次
   往返与数百个响应淹没 = 「滚不动」。
3. 把缓存说成「只是加速、不是真相」：崔总指正 —— **缓存读了就是真相**，对账只看有没有，
   没有的才去处理。

三条都写进了规格 §5；这次的落点与它们逐条相反（搭车读 / 固定 2 消费者 / 只推未处理的）。
