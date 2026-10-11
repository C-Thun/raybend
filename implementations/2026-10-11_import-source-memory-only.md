# import 只在内存 + 缩图收敛为两档（192 取消）

> 完成时间：**2026-10-11 20:09:58 +0800**
> 崔总口径（2026-10-11，逐条）：① import 里出 384 小图要**尽可能快**；② **不生成 strip(192)**；
> ③ **全局只剩两档缩图**（thumb 384 / preview 1920），192 整档取消；④ view 里不许用近似档当成品
> （`specs/view-true-image.md`），所以 **import 不需要 1920 这一档**，我上一轮说的「30 张」应当是**真身**；
> ⑤ 真身内存留 **10 张**；⑥ 重采样**先去 Lanczos3 精修**，libjpeg-turbo 的 DCT 缩放解码写进 future；⑦ 旧 strip 行清掉。
> 规格：`specs/thumbnail-tiers-and-import-memory.md`（本轮的验收清单在那儿）。

---

## 1. 改了什么

### 1.1 库外（import / 未入库）只在进程内存里

| 项 | 改后 |
| --- | --- |
| 存哪 | `crates/raybend/src/thumbnail/memory.rs`（新模块）→ Tauri `SourcesMemory` state，**退出即清** |
| 两档预算 | 缩略图（384）**按字节 256 MB**；**真身按条数 10 张**（≈ 90 MB）；两档各有各的池子、各自 LRU |
| 键 | 路径 + 尺度 + 渲染签名 + **文件身份（大小 + mtime）** → 换卡 / 覆盖保存自动不命中（无失效钩子） |
| 落盘 | **一行都不写**（`_sources/thumbs.db` 从此只服务库内文件）—— 实测见 §3.3 |
| 1920 | import **不产生也不请求**；真有人请求就现算，**不进内存**（`Class::of(Screen) = None`） |
| 渲染 | `worker::render_source`（纯渲染，JPEG q82，不碰任何缓存）取代原 `render_source_now` |

### 1.2 缩图收敛成两档

- `SizeClass::Strip` / `STRIP_LONG_EDGE` / `ImagePurpose::Strip` / `"strip"` 参数 **全链路删除**：
  `render`（含两个 `render_sig*` 分支、`all()`、`clamps_display_aspect`）、`display`、
  `worker`、`cache`、`memory`、`issues.rs`（`SNAPSHOT_ORDER` 三档 → **两档** Screen → Grid、
  定稿小图白名单、自愈判定）、`import.rs`（导入收尾从 grid+strip 两遍 → **只 grid**）、
  `develop.rs` / `thumbs.rs`（补图循环）、`export.rs`（去掉多余白名单）、前端 `ThumbSize` / `ImagePurpose` /
  `export.ts` / `issues.ts` 类型、editor 胶片带与定稿卡片取图（**改取 384**）。
- 收益：导入收尾与编辑补图都少渲一张；签名 / 缓存类别 / 规范各少一类。
  代价（实测级）：editor 胶片带队列字节 128 条 ≈ 1.3 MB → 3.5 MB，屏上解码位图每格 98 KB → 393 KB。

### 1.3 import 的看图不再要 1920

- `viewer/store.ts`：`loadScreen` 改成**可选**；不给时占位 = `loadThumb`（384），
  真身由 `loadFull` 补（位图 = 原文件字节；RAW = 完整解码）；真身也取不到且屏上无图才报 error。
- `ImportWorkspace`：**不给 `loadScreen`**（这一档在 import 里不存在）。
- 邻图预载本来就走 `loadFull ?? loadScreen` → import 自动变成「预载真身」，不用改。

### 1.4 重采样

- `render::resize_for_thumb` **去掉 Lanczos3 精修**，一条盒式快降（`imageops::thumbnail`）走到底。
  实测收益比预期大：单张 JPG 冷渲染 **202.7 ms → 125.3 ms**（见 §3.3）。

### 1.5 其它

- 探针 `examples/source-import-probe.rs` 修了一个**误导过读数**的 bug：④「4 线程争用」段以前写死
  `render_now`（库内口径），于是 `--source` 跑完也会往缓存库里写行（上一轮那份 12 行 `strip` 就是它）。
  现在按口径分派，标签也从 `Strip` 改成 `Screen`。
- 用户机器上 `%LOCALAPPDATA%\com.cthun.raybend\cache\_sources\thumbs.db` 里的旧 strip 行：
  **当前该文件不存在**（18:47 诊断时整份删掉了、之后没再重建）→ 没有可清的行。
  代码也不再产生 strip 行，所以不需要一次性迁移。

## 2. 与另一会话（展示尺寸生命周期）的口径对齐

按崔总要求读了 `specs/display-size-source.md` §2.1、`specs/display-size-lifecycle.md`、
`memory/FUNCTION-IMAGING.md` §2.1、`memory/PLAN.md` 问题清单 #10、
`implementations/2026-10-11_size-lifecycle-and-unification.md`（19:47:07）。结论：**没有逻辑冲突**
（那边管「尺寸从哪来 / 谁写 / 缺了怎么补」，本轮管「像素多久、存在哪」），但**四份文档里有 6 处表述与新口径打架**，已改：

| 文件 | 改动 |
| --- | --- |
| `specs/view-true-image.md` | 逐路径表：`fullscreen` 与 **import** 拆成两行（import 无 1920，占位 384）；网格/胶片带/右栏预览的 `grid / strip` → `grid`（384 一档）；「载入中」那行补「库内 1920 / import 384」；验收清单里 `screen` 语义那句去掉 strip |
| `specs/display-size-source.md` | 范围边界「不改三档图规格（grid 384 / strip 192 / screen 1920）」→ **两档**（grid 384 / screen 1920），并指向本轮规格 |
| `memory/FUNCTION-IMAGING.md` | §2 总表（三种图 → 两种缩图 + 真身）、上表归属段（库外只在内存、没有 1920）、§2.1 首句、§6 速查、§9 审计补记、定稿自愈两档…… 共 12 处 |
| `memory/DESIGN.md` / `memory/FUNCTION-BROWSE.md` / `memory/ARCHITECTURE.md` / `memory/FUNCTION-REPOSITORY.md` / `memory/FINISHED.md` / `memory/FUTURE.md` / `AGENTS.md` | 同上口径收敛；`FUTURE.md` 新增 **G25**（libjpeg-turbo 的 1/8 DCT 缩放解码，含网址与代价） |

## 3. 验证

### 3.1 质量门（Agent 侧）

| 命令 | 结果 |
| --- | --- |
| `cargo test --workspace --lib --offline` | ✅ `raybend` **1517** / `raybend-desktop` **134**（新增：内存缓存 8 条、库外不落盘 2 条、1920 不进内存、真身档预算、库外不请求 1920 的 viewer 测试、换卡失效） |
| `pnpm typecheck` / `pnpm test` | ✅ **1289** 通过 |
| `pnpm lint:colors` / `lint:arch` / `lint:i18n` / `pnpm build` | ✅ 全过 |
| `pnpm check:import` | ✅（无头 Chrome 走完「勾选目录 → 选库 → 确认窗 → 开工」） |

### 3.2 真机（Windows release 探针，`C:\src\tmp\pic` 306 张 = 161 JPG + 145 RAW）

| 指标 | 最初（诊断时） | 第一轮（JPEG + 窗口优先） | **本轮** |
| --- | --- | --- | --- |
| 单张冷渲染 JPG | 448–494 ms | 202.7 ms | **125.3 ms** |
| 单张冷渲染 RAW | 200–700 ms | 120.9 ms | **111.3 ms** |
| 一屏 24 张（4 路并发、全冷）JPG | 2.97–3.87 s | 1.53 s | **979 ms** |
| 一屏 24 张（4 路并发、全冷）RAW | 3.93–4.32 s | 1.90 s | **1.44 s** |
| 内存命中 | — | — | **0.12–0.14 ms/张** |
| `source_scan` | 1–4 ms | 1.2–3.0 ms | **1.2 ms / 306 张** |
| 按时间读 EXIF（306 张） | 143–330 ms | 143 ms | **166 ms** |

### 3.3 「库外不落盘」的硬证据

`--source` 跑完（12 JPG + 12 RAW 冷渲染 + 24 次 Screen 渲染 + 全部重取）之后直接开那份缓存库：

```
db 存在？ True        # 探针为库内那半边的对照接口开了库
行数 0 | 分组 []      # ← 一行都没有
```

（同一探针在本轮**之前**跑，会留下 12 行 `strip` —— 这就是那个探针 bug，见 §1.5。）

### 3.4 未验证 / 需人类确认

- **观感**：JPEG q82 + 盒式重采样（去 Lanczos3）后的 tile 画质；胶片带用 384 缩小显示是否够看。
- **import 看图的过渡**：384 占位 → 真身（位图秒出；**RAW 要完整解码几秒**，期间只有一个 384 放大图顶着）。
- **真身内存 10 张的体感**：`←/→` 翻回上一张应当秒出；超过 10 张会重解码。
- 内存占用体感：缩略图档上限 256 MB（真到上限只会在看几千张的大目录时）。
- `pnpm check:browse` 仍红：失败断言全在另一会话正在改的 `Tile.tsx` / `FilterBar.tsx` 一带，与本轮无关。

## 4. 遗留

1. **库内 AVIF 仍是单线程编码**（`encode_avif` 的 `with_num_threads(Some(1))`）：实测 4 路并发吞吐 20.7 → 44.3 张/s，影响 browse / 编辑 / 导出，本轮范围外。
2. **`_sources/thumbs.db` 没有 GC**（`cache::gc` / `drop_stale_signatures` 有实现、无调用点）：库内小图会一直涨；
   旧口径的 AVIF 行在改口径后也成了孤儿 —— 已登记 `memory/FUTURE.md` **G26**（接回收器 + 设置里的清缓存入口）。
3. **JPEG 全尺寸解码**（`image` crate 无 DCT 缩放）：125 ms 里约 80–100 ms 是解码 + 缩放 —— 这一档要动就得引 C 依赖，已登记 `memory/FUTURE.md` **G25**（libjpeg-turbo / mozjpeg，预期 30–60 ms/张）。
4. 探针 `examples/source-import-probe.rs` 是**未提交**的诊断文件（复测用），去留请崔总定。
