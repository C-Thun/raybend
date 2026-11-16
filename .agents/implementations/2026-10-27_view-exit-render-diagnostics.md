# 看图链路分段诊断日志（退出编辑后「看大图卡住」排障）

完成时间：2026-10-27 03:49:37

## 现象（崔总 2026-10-27 报）

进 editor 编辑一会儿 → 退出 → browse 双击看大图：图片区空的、顶部「正在载入原图…」一直挂着；
compare 同样；切 import 也看不了大图；GPU 占用一直高，进 view 后几乎把应用拖死，退回 tiles 也难显示。
（高质量降噪**没开**，所以不是降噪那条已知问题。）

## 已经拿到的现场事实

**① 崩溃/日志没有报错**（`%LOCALAPPDATA%\com.cthun.raybend\logs\raybend.6.log`，崔总那一场 16:47–17:15 UTC）：

- 退出编辑（17:07:23 最后一次 tao 事件）之后**一行渲染日志都没有**，直到 17:13:57 才有新一轮 tao 事件；
  17:15:54 记 `正常退出` —— 进程没崩，是界面被拖到只能关掉。
- 编辑期间「真帧」管线时间从 **800ms 一路涨到 2200ms**（同一张 5186×3457），
  退出编辑时后台还在跑 `develop_preview_refresh`（每次退出/换图都会排一次）。

**② 真机探针复现（`pnpm perf:win --launch` 的 CDP 通道，controller 侧驱动）**：

- 竞态脚本（改一次曝光 → 立刻退出编辑 → 立刻双击）：`editor_unbind_renderer` **挂了 5.5s**，
  其后 `view_image` 最长 **18.3s**、`thumb_get` 最长 **12.9s**；期间界面一直显示「正在载入原图…」，
  tiles 也一批批慢慢冒 —— 与崔总描述的观感一致。
- 冷渲染实测（编辑过的 RAW，MYP1000036）：
  `thumb_get`(384) 38ms（命中）／`view_image`(1920) **9.5s**／`view_image`(full) **10.7s**；
  缓存回填之后同一批请求 **55–90ms**。
- 同一条栈在**编辑器里**的「真帧」只要 2.4s 管线 —— 说明浏览/看图这条路把同一份重活
  又完整跑了一遍，而且和「退出编辑时的 preview 刷新」「十来张 tile 的缩略图」
  全挤在**单例 RAW 解码管道 + WorkQueue 两条工作线程**上排队。

结论（待日志确认细节）：**不是死锁，是取图链路的排队 + 每张冷渲染的秒级代价**；
「像死掉」是 10–20 秒的串行等待叠加用户继续点（每次都再排一个 full 渲染）造成的。
GPU 一直高：这段时间显影管线一直在跑，没有空闲。

## 这次改了什么（只加诊断，不改行为）

| 文件 | 加了什么 |
| --- | --- |
| `crates/raybend/src/thumbnail/render.rs` | `RenderClock` + `log_render_stages`；RAW 编辑档（full / 缩小）、色彩管理档各一条 DEBUG 分段行：**解码 / 缩放 / 显影 / 编码 / 合计 / 输出尺寸** |
| `src-tauri/src/thumbs.rs` | `view_image`：进/出各一行（用途 + 文件 + 毫秒 + 字节），并单记「库许可已取」耗时；`render_profile_cached` / `render_full_cached`：缓存**命中/未命中**各一行、未命中渲染完再记总耗时；`thumb_get`：只记 ≥200ms 的慢请求 |
| `crates/raybend/src/raw/worker.rs` | `acquire()`：等解码许可 ≥30ms 记一行（**排队时长**的直接证据） |
| `src-tauri/src/editor.rs` | `editor_unbind_renderer`：`join` 花了多少毫秒（退出编辑那一卡的度量） |
| `src/components/ui/viewer/store.ts` | `console.debug("[viewer] screen/full/ensure <文件> <ms>")`，只记 ≥100ms |
| `src/workspaces/editor/EditorWorkspace.tsx` | `console.debug("[editor] 停渲染线程 <ms>")`，只记 ≥100ms |
| `crates/raybend/src/diagnostics/logger.rs` | **噪音闸门**：DEBUG 级丢掉 `naga` / `wgpu*` / `mp4parse`（它们一次进编辑器就吐 ≈2 MiB 着色器源码，会把 2 MiB/文件的轮转窗口整段挤掉，刚加的诊断行反而看不到）；它们的 WARN/ERROR 照旧。带单测 |

## 怎么用

1. `pnpm debug:win` 重编（dev 构建默认 DEBUG 级；要更细可 `RAYBEND_LOG=debug`）。
2. 复现：进编辑 → 编辑一会儿 → 退出 → 双击看大图。
3. 看 `%LOCALAPPDATA%\com.cthun.raybend\logs\raybend.log`，按顺序读：
   - `[editor] 停渲染线程 / join Xms` —— 退出编辑卡了多久；
   - `[view] → / 库许可已取 / 未命中，开始渲染 / 渲染完成 … Xms / ← … Xms` —— 一条取图请求从进到出；
   - `[render] … 解码 Ams 缩放 Bms 显影 Cms 几何 Dms 编码 Ems 合计 Fms` —— **阶段的真凶**；
   - `[raw] 等解码许可 Normal Xms` —— 排队（不是自己慢）有多少。
4. 前端那几条在 devtools 控制台（`console.debug`）。

## 验证

- `cargo check --workspace` —— 通过（仅既有 3 条 warning）。
- `cargo test --workspace` —— 全绿（1586 + 1 + 7 + 4 + 155 + 2 + 2）。
- `pnpm typecheck` / `pnpm test`（1328 通过）/ `pnpm lint:i18n` / `lint:arch` / `lint:colors` —— 全绿。
- 新增单测：`diagnostics::logger::tests::shader_and_decoder_chatter_is_dropped_only_at_debug`。

## 遗留

- **根因还没结案**：本次只装表。已知待回答的三个问题（日志会直接给出）：
  ① 9.5s 里解码 / 显影 / 编码各占多少；② 有多少是排队（等许可）而非计算；
  ③ 退出编辑时的 preview 刷新与看图请求是否在抢同一张照片的重活。
- 复现用的测试库照片（`C:\src\tmp\testrespos\photos\2026-09-13` 的 MYP1000025/35/36）
  被我加了曝光/裁切(118°)/旋转等试验性编辑，**需要的时候撤销即可**（都是测试库，不是崔总的正式库）。
