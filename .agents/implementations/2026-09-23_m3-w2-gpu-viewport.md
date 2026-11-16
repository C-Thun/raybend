# M3-W2：GPU 编辑视口产品化（洞口里出现真实照片）

完成时间：2026-09-23 12:06:41 CST

> 计划：`plans/M3-W2.md`（本波不走 plannotator，沿用 W1 的口径）。
> 上游：`PLAN.md` §M3-W2、`plans/HANDOFF-2026-09-23.md` §3、`design/editor.md`、
> `AGENTS.md` §6.1 / §7.9、`docs/native-viewport-coordinate-guide.md`。

---

## 0. 一句话

编辑视口从「留好的洞口」变成**真的在出图**：主窗口透明 + wgpu 直绘到窗口表面，
RAW 内嵌预览与完整解码两条路、两档迟滞切换，滚轮以光标为锚缩放、拖动平移、
双击 / `0` / `1` 在适合窗口与 1:1 之间切；渲染线程有**监督器**兜底
（建不起来 / panic / 设备恢复失败都会重建并上报）。

---

## 1. 改了什么（按文件）

### Rust：渲染 crate（`crates/raybend`）

| 文件 | 改动 |
| --- | --- |
| `src/render/image.rs` | **新增**。`RenderImage`（原 `scene::TestImage` 升级）：RGBA8 + `from_rgb8`（长度不符返回 `None`，不补零）+ `clamped_to_long_edge`（夹到设备纹理上限） |
| `src/render/color.rs` | **新增**。`Srgb8` + `parse_css`（十六进制 / `rgb()` / `rgba()`）+ `to_clear_color`（**sRGB→线性**，见 §3.2） |
| `src/render/tier.rs` | **新增**。取图档位迟滞：升档 `zoom ≥ 1.0`，降档 `zoom < 0.5`（避免在 1.0 附近反复触发完整解码） |
| `src/render/supervisor.rs` | **新增**。`RestartPolicy`：指数退避 + 预算 + 「跑稳了才清零」（纯逻辑，可单测） |
| `src/render/gpu.rs` | `GpuContext` / `OffscreenRenderer` **泛化**成「照片渲染器」：`Option<RenderImage>`、`set_image` / `clear_image`、`set_backdrop`、`label_prefix`；`resize` 拆成 `set_dpr` + `resize_surface`（编辑器只换 surface，DPR 由 DOM 报）；`OffscreenRenderer` 加 `simulate_device_loss` / `recover` |
| `src/display/pixels.rs` | **新增**。像素取图口 `pixels(path, PixelSize::{Screen,Full})` → `DisplayPixels`（RGB8 + `PixelOrigin`：位图 / 内嵌预览 / 完整解码） |
| `src/thumbnail/render.rs` | 抽出 `decode_file(path, DecodeSpec) -> DecodedSource`：**全仓唯一一份「文件 → 源像素 + 方向」**；`render_file`（出 JPEG）与 `display::pixels`（出像素）共用它 |
| `src/raw/backend.rs` | `DecodeRequest::with_preview`（「限了长边但不要内嵌预览」这一档构造函数拼不出来） |
| `examples/editor-offscreen.rs` | **新增**。编辑视口的离屏像素证据（期望值外部给定 + 落 PNG） |

### Rust：外壳（`src-tauri`）

| 文件 | 改动 |
| --- | --- |
| `src/render_window.rs` | **新增**。`raw_handles`（从 spike 抽出来，两个窗口共用）+ `client_size`（surface 该配多大） |
| `src/editor.rs` | 洞口契约（W1）+ **渲染线程**：`RenderCommand` 单通道、监督器循环（`catch_unwind` + 重启 + 上报）、解码线程（最新者优先）、窗口事件只推尺寸、`ViewportIntent`（`zoomAt` / `zoomBy` / `pan` / `fit` / `toggleFit` / `reset` / `hitTest`）、5 条新命令 |
| `src/lib.rs` | 注册模块与 5 条命令 |
| `src/contract.rs` | 两条契约断言（`EditorViewportState` / `EditorRenderState`） |
| `src/spike_viewport.rs` | 改用共用 `raw_handles`；`GpuContext::new` 传合成测试图与 `"spike"` 标签 |
| `tauri.conf.json` / `tauri.linux.conf.json` | 主窗口 `transparent: true`（合并补丁那份**同步加了同一个字段**） |

### 前端（`src/`）

| 文件 | 改动 |
| --- | --- |
| `lib/editor-viewport.ts` | 载荷加 `backdrop`（**字符串原样上行**）；去重比较一并纳入 |
| `lib/editor-intent.ts` + `.test.ts` | **新增**。平移意图的帧合并（尾样本必发）+ 拖动会话（点击 / 拖动判据用总位移） |
| `api/editor.ts` / `api/types.ts` | 5 条新命令的封装 + `EditorRenderState` 等 DTO；两块状态进 `dto-contract.json`（两侧断言） |
| `features/editor/viewport.tsx` | 滚轮（复用看图件的 `createWheelZoom`）、拖动平移、双击切换、松手命中测试、洞口提示（拿到状态、浏览器、载入中、解码失败、渲染失败） |
| `features/editor/source.ts` | 加 `editorViewportNotice`（纯函数，优先级：空态 > 有没有照片 > 错误 > 进度） |
| `features/editor/store.ts` | 加 `renderState` / `holeActive` 两个信号（根节点也要读 `holeActive`，所以只能有一份） |
| `features/editor/actions.ts` | **新增**。编辑工作区的动作槽（`viewing` / `filmVisible` / `cycling` / `hasPhoto`） |
| `workspaces/editor/EditorWorkspace.tsx` | bind / 轮询（250ms）/ 换照片 / 把缩放适配注册进 `ViewerActions` / 中列条件透明 |
| `App.tsx` | 根节点条件透明；`activeWorkspaceActions` 纳入编辑 |
| `i18n/*` | 洞口提示的 7 条文案（中英双份；删掉 W1 的 `editor.viewport.pending`） |

---

## 2. 关键决定与理由

### 2.1 洞口底色**由 wgpu 画**，DOM 链在出图时让开

物理约束：**DOM 在 GPU 的上面** —— 洞口之上任何一层画了底色，照片就一个像素也漏不出来。
所以出图时 `根节点 → main → 视口` 这条链必须透明（`holeActive`），洞口里的底色由
wgpu 的清屏负责。三条配套：

1. **清屏色 = 洞口底色**（主题相关）→ 由前端把 `getComputedStyle` 的字符串当**事实**报上来，
   解析与 sRGB→线性在 Rust（`Srgb8`）。Rust 不需要认识主题，浅色主题也不会出现色带；
2. **清屏作用于整张 surface**（不只是洞口）→ 于是 DOM 里那些「没画东西的缝」
   （比如三点条 8px 的静止态、它本来靠父节点底色）露出来的是**洞口底色**，不是桌面；
3. **出图失败 / 线程放弃 / 浏览器里** → `holeActive` 为假，DOM 自己画底色 + 一句说明，
   编辑器其余部分照常可用（**退路成立**，`PLAN.md` M3-W2 的风险条款兑现）。

**从 RapidRAW 学的**（`/home/andares/repos/refers/RapidRAW`，AGENTS.md §7.5 的参考实现）：
它的主窗口同样是 `transparent: true` + `decorations: false`，`html`/`body` 常驻透明、
根节点**只在 GPU 渲染器启用时**透明，并且把 `bgPrimary` 从 DOM 报给 Rust 当清屏色 ——
本波的做法与它同源（差别只在「谁做坐标数学」：按 §6.1 红线，我们的坐标全在 Rust）。

### 2.2 渲染线程 + 监督器：panic 不许静默冻住

`AGENTS.md` §7.9 记的那次事故（`create_bind_group` panic 打死渲染线程 → 界面照旧响应、
图永远冻住、日志安静）在这一波被堵住：

* 线程体包在 `catch_unwind` 里，**panic / 建不起来 / 设备恢复失败**统一走 `RestartPolicy`
  （指数退避 250ms→5s，5 次后认输并把错误写进状态）；
* 「跑稳了（≥5s）才清零重启预算」—— 否则「起来就崩」的循环里计数永远回零；
* 设备丢失：`render()` 报错且设备丢失记录非空 → `recover()` 整套重建
  （layout / buffer / texture 都记着自己属于哪个设备）；重建失败交回监督器；
* 连续 5 次渲染错误（同一套坏状态）也交回监督器 —— 真机上那类错误只有重建才好；
* 前端 250ms 轮询把 `restarts` / `lastError` / `history` 拿回来，**出图成功就把错误清掉**
  （spike 的「红字永远挂着」教训）；界面上只有「当前有错」时才给提示与「重试」。

### 2.3 两档取图与「什么时候要哪一档」

* `preview`：RAW 走内嵌预览（毫秒级）、位图缩到长边 ≤1920；`full`：完整解码（RAW 走传感器）；
* 迟滞：`zoom ≥ 1.0` 升档、`zoom < 0.5` 降档 —— 停在 1.0 附近抖动不会反复触发完整解码；
* 解码**不在渲染线程上**：一条解码线程 + 单调任务号，换照片/换档把新任务压进去，
  过期结果直接丢（连排队时就知道自己过期的任务都不做）；
* 上传前 RGB8 → RGBA8（`write_texture` 每行要 256 字节对齐，6000 宽的 RGB8 不满足）；
  超过设备 `max_texture_dimension_2d` 的图先降采样（否则 `create_texture` 直接报错）。

### 2.4 复用（不是新造）

* **一个 surface 渲染器**：spike 的合成测试图与编辑器的真实照片用同一个 `GpuContext`
  （原来写死测试图，W2 泛化成 `Option<RenderImage>` + `set_image`）；
* **一份解码**：`thumbnail::render::decode_file` 是「文件 → 像素 + 方向」的唯一实现，
  `render_file`（JPEG 字节）与 `display::pixels`（GPU 像素）都走它 —— 方向那套优先级踩过三次坑；
* **一份滚轮手感**：编辑器复用 `components/ui/viewer/interaction.ts` 的 `createWheelZoom`
  （指数映射 + 帧合并 + 尾样本），没有第二份；
* **一份窗口胶水**：`render_window::raw_handles` 由 spike 与编辑器共用；
* **命令不另立**：缩放 / 适配 / 1:1 / 上一张 / 下一张**复用** `viewer.*` 那五条命令，
  编辑器只是把自己的实现注册进 `components/ui/viewer/actions.ts` 的动作槽。

### 2.5 前端只发意图（新增两条「不给坐标」的意图）

`zoomAt{x,y,factor}`（滚轮）与 `zoomBy{factor}`（按钮/快捷键）分开，是因为
**锚点不同**：前者是光标（前端只报 `clientX/clientY`），后者是洞口中心（只有 Rust 知道）。
`toggleFit` 同理：双击时由 Rust 判「现在是不是 1:1」，避免前端拿 250ms 前的状态去猜
（会出现「双击之后反而跳远」的窗口期）。

---

## 3. 验证（Agent 侧能跑的都跑了）

### 3.1 命令与结果

| 命令 | 结果 |
| --- | --- |
| `npx tsc --noEmit -p tsconfig.json` | 通过 |
| `pnpm test` | **847 通过 / 0 失败**（W1 是 835，本波 +12） |
| `pnpm lint:colors` / `lint:arch` / `lint:i18n` | 三条全绿 |
| `pnpm build` | 通过（只有既有的「chunk > 500kB」提示） |
| `cargo test -p raybend --lib` | **837 通过 / 1 ignored**（含新增：颜色解析、档位迟滞、监督策略、像素取图口、设备丢失恢复） |
| `cargo test -p raybend-desktop --lib` | **57 通过**（含契约两条、意图反序列化用真实字段名、底色解析与兜底） |
| `cargo clippy --workspace --all-targets` | 仅 1 条**既有**警告（`store/backfill.rs` 的复杂类型），本波新增代码无警告 |
| `pnpm smoke:ui`（厨房水槽，默认目标） | `"problems": []` |
| `pnpm smoke:ui http://localhost:1420/` | 外壳渲染与主题/密度/分栏断言全过（脚本报的 8 条问题都是「画廊演示块不在这个页面」，因为打的是 `/` 而不是画廊） |

### 3.2 离屏像素证据（`cargo run -p raybend --example editor-offscreen -- /mnt/c/src/tmp/raybend-w2`）

15 条断言全过，产出 `01-fit.png` / `02-hole.png` / `03-one-to-one.png`。
期望值**全部外部给定**（合成图四块的颜色是常量、坐标是手算的），断言覆盖：

* Fit 的倍率（1.5）与四块落在正确的象限（顺带证伪了「上下/左右镜像」那类错法）；
* 照片之外是洞口底色**且不透明**；
* 洞口外一格**一个像素都没碰**（scissor）；
* 1:1 下紧挨图心的四个像素分属四个不同的块（「1 图像像素 = 1 物理像素」+ 象限正确）。

**这个例子当场抓到一个真 bug（这波最有价值的一条）**：
第一版把 `Srgb8` 直接除以 255 当清屏色，回读出来是 **110,115,125** 而不是 **40,44,52**。
原因：wgpu 在 `*-srgb` 目标上把清屏值当**线性值**，由硬件再编码成 sRGB。
修法是 `to_clear_color` 走标准 sRGB→线性变换（`c ≤ 0.04045` 走线性段）。
这条约定现在钉在 `color.rs` 的注释与测试里、并且**由那个例子守着**（约定一变就报红）。

### 3.3 没验证的（**归人类**，`AGENTS.md` §2.8）

* 照片是否真的出现在洞口里、1:1 是否锐利、透明合成是否正确（洞口外不漏桌面、边缘不发白）；
* 拖窗口 / 跨屏 / 最小化恢复时视口是否跟随；浅色主题下洞口底色是否与界面一致；
* 渲染线程崩溃恢复在真机上的观感（监督器逻辑有单测，真机现象没看过）；
* 大图（6000×4000）× 两档切换时的内存与手感。

---

## 4. 真机验收清单（Windows）

```bash
pnpm build && pnpm debug:win     # 产物必须比 dist 新（check:win 会拦）
```

**最短门槛（三步，任何一步不过就停下报告）**：

1. 进「编辑」，从胶片带选一张 —— **洞口里出现照片**（RAW 与 JPG 各试一张）；
2. 按 `1`（或双击）→ 1:1：能看到真实像素（不是糊的放大）；再按 `0` 回适合窗口；
3. 把光标压在照片里某个特征点上滚轮缩放 —— **特征点不漂**。

通过之后再看：

* **洞口外不漏桌面**：拖动窗口、切主题（浅色下洞口底应是浅灰而不是深灰或黑）；
* 窗口拖到另一块屏（若有多屏）→ 照片仍然居中、不偏；
* 换照片（胶片带左右点）→ 旧照片停留到新照片画出来，不闪黑；
* 切到「浏览 / 导入」→ 外观与以前**完全一致**（这一步是窗口透明那 3 处条件类是否收干净的判据）；
* 大图 JPG（或 RAW）按 `1` 等它完整解码的那一下（应看到画面从预览档换成全尺寸档）。

---

## 5. 遗留与欠账

| 欠账 | 排期 | 说明 |
| --- | --- | --- |
| 编辑栈（拉杆改画面、落库、撤销） | W3 | 渲染线程已能「换图 + 重画」，管线接上去即可 |
| 「位图 + RAW」时默认载入 `_RAW/` 同名 RAW | W3 | `REPOSITORY.md` §4.1 的约定；它属于「编辑落在 RAW 上」，与编辑栈一起定（本波解码的是资产当前展示的那个文件） |
| 缩略图金字塔 / mipmap | W6 | 本波是「预览档 ≤1920 + 线性过滤」，大幅缩小时的抗锯齿质量留给性能基线那一波 |
| 全尺寸 RAW 解码与缩略图共用 worker（会互相排队） | W6 | 解码线程串行 + 最新者优先；真机若明显卡顿再给 worker 优先级或独立进程 |
| 颜色管理 | 后期 | 第一阶段只保证 SDR 不变形（`AGENTS.md` §6.1） |
| 视口的「拖拽类控件」回归 | W5 | 本波没有拖拽控件；W5 的裁切/旋转把手照 `AGENTS.md` §2.17 那份判据做 |
| `AGENTS.md` §7.9 补两条（清屏值要线性化、整窗清屏＝洞口底色的来源） | 待办 | 本波**没有**改 `AGENTS.md`：工作区里已有一份**不是本会话**的 `AGENTS.md` 改动没提交，按纪律只报告、不动手（详见汇报） |

---

## 6. 给下一个 session 的一句话

洞口里已经能出图了，**接下来别再从「怎么让照片出现」开始**：
`editor_render_state` 是唯一的状态入口，`editor_set_photo` 换图、`editor_viewport_intent` 发意图；
W3 要做的只是「把编辑栈的参数变成渲染线程的一级输入」——
`GpuContext::set_image` 已经证明换纹理这条路是通的，剩下的是 shader 与参数缓冲。
动视口之前仍然先读 `AGENTS.md` §7.9 与 `docs/native-viewport-coordinate-guide.md`。
