# import tiles 提速（库外 JPEG 口径 + 窗口优先队列）

> 完成时间：**2026-10-11 18:45:17 +0800**
> 需求：崔总 2026-10-11「import 出图要快：只读必要的、窗口优先、别重复算、进程内有缓存；
> 相片进库才用 AVIF，import 是外面的东西当然用 JPEG；别在来源目录上折腾缓存」。
> 诊断与实测见同日 `implementations/2026-10-11_import-tiles-diagnosis.md`；纪律见 `AGENTS.md` §2.24。

---

## 1. 改了什么（按「减法优先」排序）

| # | 改动 | 为什么 |
| --- | --- | --- |
| 1 | **来源目录（未入库）小图改 JPEG q82**；库内仍 AVIF q90 / 4:4:4 | 同一张 384px 图：AVIF 编码 118–166 ms → JPEG 3–5 ms（体积 25→27 KB）。两套**签名分开**（`render_sig_source`），同一张图先后被 import / browse 看不互相冒充 |
| 2 | **内嵌预览先比尺寸、再编码**（`prepare_pixels` / `encode_pixels` 拆开） | 以前先编码再发现 160×120 不够 384，白烧一次编码（实测 ~118 ms/张） |
| 3 | **窗口优先**：滚动 / 缩放停 0.3 s 后，把窗口内区间提到队首（`ThumbQueue.prioritize`） | import 不做载入画面，就得按「眼前这几屏」排队；请求仍只由可见格子发，滚出去即撤 |
| 4 | **队列并发按核数 2–8**（`min(8, max(2, 核数−1))`） | 以前写死 4；缩略图是 CPU 活，留一核给 UI |
| 5 | **换目录不再清内存缓存**（只有 `reload` / 磁盘变化才清） | 来回切 A/B 目录时瞬间出图，不重生成；内存靠队列自己的 LRU（600 条）封顶 |
| 6 | **按时间读 EXIF 一批 512**（以前 8 张 × 39 次串行 IPC） | 306 张实测读 EXIF 只要 ~140 ms（Rust 内 4 线程）；贵的是往返次数与整表合并 |
| 7 | **选中项 EXIF 加进程内 LRU（32 条）** | 实测读 EXIF 0.5–1 ms/张（便宜，仍放「选中时拿」），但 RAW 补镜头名要问 worker，来回切同一张不该重问 |
| 8 | **RAW 补镜头名走 worker `Low` 档** | 以前直接抢 `shared()` 锁 —— 「选中才拿」的信息把排队的缩略图顶住了 |
| 9 | **进 view 的 screen 1920：来源文件走 JPEG** | 实测 AVIF 4.2–5.6 s → JPEG 1.3–2.2 s（同机同图）；库内不变 |

**没做**（明确不在本轮，见 §5）：库内 AVIF 的多线程编码、sources 缓存的失效策略与 GC、任何新缓存层 / 预热。

## 2. 涉及文件

#### Rust 核心
- `crates/raybend/src/thumbnail/render.rs`：`EncodeFormat`（Avif/Jpeg）+ `JPEG_THUMB_QUALITY`；
  `encode_with_lut` 拆成 `prepare_pixels`（方向→3:1 夹取→缩放→编辑栈）+ `encode_pixels`（唯一编码出口）；
  位图内嵌快路径改成「先出像素、比过尺寸再编码」；新增 `render_sig_source`；格式参数沿 `render_file_with_profiles` /
  `render_bytes_with_edit_and_lut` / `render_raw_file` / `encode_with_format` 贯穿。
- `crates/raybend/src/thumbnail/worker.rs`：新增 `render_source_now`（JPEG + source 签名）与
  `render_cached_with_signature`（库内签名 vs 库外签名的分派点）；`render_now*` 保持库内（AVIF）。
- `crates/raybend/src/display/mod.rs`：`ImageRequest.format` + `ImageRequest::source()`；
  `rendered()` 按请求格式编码；`cached_image` 按格式选签名、命中时 MIME 不再一律写 Jpeg。
- `src-tauri/src/thumbs.rs`：`thumb_get` 的**口径分派点**（`resolve_asset` 有 ⇒ 库内 AVIF；无 ⇒ 库外 JPEG）；
  `view_image` 同口径分派；`edited_source` 改成接收已解析资产（避免重复反查）。
- `src-tauri/src/source.rs`：`file_exif` 的 RAW 镜头名探测 `acquire(DecodePriority::Low)`。
- `crates/raybend/examples/source-import-probe.rs`（**新增，未提交**）：诊断探针
  （`--screen` / `--source` / `--encode-bench` / `--exif` / `--view` / `--raw-concurrency` / `--hits`），复测用。
- `crates/raybend/examples/full-tier-probe.rs`：调用点补格式参数。

#### 前端
- `src/components/ui/thumb-queue.ts`（+ `.test.ts`）：`prioritize(paths)` 批量置顶（顺序即优先级，
  ready/在飞跳过、**不补请求**）；`defaultThumbConcurrency()` 按核数取 2–8。
- `src/components/ui/tiles/source.ts`：契约加 `prioritizeVisible?(from, to)` 与 `requestThumb(path, priority?)`。
- `src/features/photo-grid/PhotoGrid.tsx`：`onVisibleRange` 带 0.3 s 防抖 → `source.prioritizeVisible`；
  可见格子请求带 `priority`。
- `src/features/photo-grid/source.ts` / `src/features/browse/grid-source.ts`：`prioritizeVisible` 实现
  （显示序 → 路径，交给同一条队列）。
- `src/features/photo-grid/store.ts`（+ `.test.ts`）：换目录不再 `thumbs.clear()`；`loadTimes` 一批 512 + `Set` 查重。
- `src/features/exif-strip/selected.ts`（+ `.test.ts`）：32 条进程内 LRU（只缓存成功）。
- `scripts/check-import-boot.mjs`：修假后端缺口（`organization_reconcile` / `migration_snapshot` 返回结构，
  以前返回 null 让前端模块崩在冒烟的第一屏）+ 钉死界面语言为 `zh-CN`。

#### 文档
- `AGENTS.md` §2.24（新增硬约束）、§6.5 / §11.4 / §11.5 口径收窄；`memory/FUNCTION-IMAGING.md`
  （§2 附注、§9 更正「每批 8 张」、新增 §10 实现表）；`memory/ARCHITECTURE.md` §6.4、`memory/FUNCTION-REPOSITORY.md` §4.1。
- `implementations/2026-10-11_import-tiles-diagnosis.md`（诊断）。

## 3. 关键决策与理由

- **格式由场景决定，落在「唯一的编码出口」上**（`encode_pixels`），而不是到处 `if import`：
  库内 / 库外各一个签名、各一条缓存，调用点只传 `EncodeFormat`。
- **签名分开**（`jpeg-q82-grid-v16-embedded-v1` vs `avif-q90-grid-v16-embedded-v1`）：
  同一个文件先进 import 再进库，两份图本来就该各存一条；也保证以后改口径不会拿错图。
- **不补请求**：`prioritize` 只重排**已经在队里**的项；「谁该出图」只有可见格子一个来源
  （虚拟列表保证），否则会出现「窗口外也在生成」。
- **不换目录清缓存**：换来的是「切回来秒出图」；失效点收在两个明确入口（`reload` / 磁盘变化）。
- **EXIF 不进队列**：实测读 EXIF 0.5–1 ms（便宜），进队列反而要改缩略图的 IPC 载荷；真正的坑是
  RAW 镜头名（worker 调用）—— 那条走 Low 档 + 前端 LRU 就够。

## 4. 变化点清单（选中 / 进 view，`AGENTS.md` §2.24 要求先列）

| 触发 | 会变的元素 | 取数路径 | 会不会阻塞 | 过渡 |
| --- | --- | --- | --- | --- |
| 点一张 / 方向键 | tiles 选中态 | 纯前端（selection 信号） | 不会 | — |
| 同上 | `flowinfo`（名称 / 相机 / 镜头 / 曝光 / 尺寸 / 类型） | `file_exif`（~0.5–1 ms）+ 32 条 LRU；RAW 无镜头名时问 worker（**Low 档**） | 不阻塞 UI（异步 + revision 丢弃迟到结果）；worker 侧排队在缩略图之后 | 未到时保持上一次的值 / 空白 |
| 同上 | tiles statusbar（张数 / 选中数） | 纯前端 | 不会 | — |
| 进 view（Enter / 双击） | 底图 384 → screen 1920 | 网格小图已在内存/缓存；1920 走 `view_image`（来源文件 JPEG） | 不阻塞 UI（IPC 异步）；1920 冷渲染 1.3–2.2 s（RAW 2.2 s） | 先出网格小图 → 真图到位后**原地替换**（既有机制） |
| 进 view / 对比 | 参与对比的 2–4 张真实尺寸 | `ensureNatural`（读 EXIF 头，~1 ms/张） | 不会 | 先用缩略图尺寸显示 |
| 进 view | 胶片带 | 与网格共用同一条队列（已缓存即刻可用） | 不会 | 滚出窗口的带内请求被撤 |

## 5. 验证

#### Agent 冒烟（全部本地跑过）

| 命令 | 结果 |
| --- | --- |
| `cargo test --workspace --lib --offline` | ✅ `raybend` 1502 通过 / 11 ignored；`raybend-desktop` 134 通过 / 1 ignored（含本轮新增两条：`render_source_now_is_jpeg_and_shares_the_cache_without_claiming_it`、`source_and_library_signatures_are_distinct_but_share_the_pipeline_version`） |
| `pnpm typecheck` | ✅ |
| `pnpm test` | ✅ 1287 通过（新增：队列「窗口优先」两条 + 默认并发一条；EXIF LRU 一条；「按时间一批 512」改写） |
| `pnpm lint:colors` / `lint:arch` / `lint:i18n` | ✅ |
| `pnpm build` | ✅ |
| `pnpm check:import` | ✅（修了假后端两处缺口与语言后） |

#### 真机实测（Windows release + 探针，`C:\src\tmp\pic` 306 张 = 161 JPG + 145 RAW）

| 指标 | 改前 | 改后 |
| --- | --- | --- |
| 单张冷渲染（JPG） | 448–494 ms | **202 ms** |
| 一屏 24 张（4 路并发、全冷） | JPG 2.97–3.87 s / RAW 3.93–4.32 s | **JPG 1.53 s / RAW 1.90 s** |
| 命中缓存 | 0.55–0.76 ms | 0.59–1.06 ms（不变） |
| 进 view 的 screen 1920 | JPG AVIF 4.15 s / RAW 5.62 s | **JPG 1.30 s / RAW 2.20 s** |
| 按时间读 EXIF（306 张） | 143–330 ms | 143 ms（未变，但往返 39 → 1） |
| 选中读 EXIF | 0.88 ms（头）/ 0.77 ms（全标签） | 同左 + 32 条 LRU；RAW 镜头名 861 ms（首次含 worker 启动）走 Low 档 |

#### 未验证 / 需人类确认
- GUI 观感：tile 画质（JPEG q82 vs AVIF q90 —— 体积只差 7%，观感请真机看一眼）、
  窗口优先的体感（滚动停下 0.3 s 内出图）、进 view 的过渡是否够顺。
- RAW 的 `Low` 档镜头名探测在真实慢盘上的排队时间（是否会等很久才补上镜头名）。
- `pnpm check:browse` 当前是红的，但失败的断言全部落在**另一个会话正在改**的
  `Tile.tsx` / `FilterBar.tsx` / `filter.ts` / `PhotoMarks.tsx`（mtime 17:05，本轮进行中），
  与本次改动无关 —— 未作为本轮凭据，建议合并后跑一次。
- 外置 HDD / 网络盘的真实体感（本轮只保证了「不再有全目录预扫」与单张成本下降）。

## 6. 遗留

1. **库内 AVIF 仍是单线程编码**（`encode_avif` 的 `with_num_threads(Some(1))`，与它自己文档里
   「线程是必须的」矛盾）：4 路并发吞吐 20.7 → 44.3 张/s（实测），但影响 browse/编辑/导出，
   本轮范围外，建议下一轮单独一次改动。
2. **sources 缓存 151 MB / 4179 行全是旧签名尸体**（`drop_stale_signatures` / `gc` 全仓无调用点）；
   改口径后旧 AVIF 行更是纯废物。要么下轮接 GC，要么手动删 `cache/_sources/thumbs.db`（可重建缓存）。
3. **JPEG 解码仍是全尺寸**（`image` crate 无 DCT 缩放）：202 ms 里约 120 ms 是解码 + 缩放。
   要再快一档需要 libjpeg-turbo 的 1/8 缩放解码（新增 C 依赖，按 `AGENTS.md` §2.9 得先讨论）。
4. 探针 `crates/raybend/examples/source-import-probe.rs` 是**未提交**的诊断文件，留作复测；
   要不要长期保留（并入 `thumb-bench`）请崔总定。
