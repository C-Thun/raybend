# 图像规格（IMAGING.md）+ 编辑器交互档位 / preview 节点 / 小图放大 / 毛玻璃提示

完成时间：2026-09-24 16:17:19 CST

---

## 0. 这一轮做了什么（人类口述 → 规格 → 落地）

人类口述了「应用内所有图的方案」（库内 vs 库外、thumb / preview / 大图三种图的分工与生成节点、
只缩不扩、截断比例、issue 与 latest 的切换语义、载入毛玻璃提示、export 用 preview），
要求**记成长久规格**；随后追加：把它**与导入导出位图格式支持的方案整理到一起**，
并「按这个方案继续做完 todos」。本记录对应这次落地。

---

## 1. 新增：`IMAGING.md`（唯一事实源）

顶层新文件，两件事合成一份，避免两处各写一份：

* **§1 格式支持范围** —— 位图导入 6 种（jpg/tiff/png/webp/avif/heic）、导出 5 种（不含 HEIC）、
  RAW 只进不出（跟 rawler 走：Bayer 先全、X3 可不支持、X-Trans 看 rawler）、
  JXL 只做未来导入导出全流程、issue 快照恒 AVIF、以及「哪些格式要另接解码器」。
* **§2–§7 库内三种图** —— thumb（384/192，`thumbs.db`）、preview（1920，库根 `cache/full/`）、
  大图（直读 + 渲染管线）的尺寸、存放、**生成节点**、显示口径与状态速查表。
* **§8 现状对照**（逐条标 ✅ / ❌ / 待办）、**§9 未决点**（AVIF 解码器选型、RAW+JPG 的 preview 源）。
* 人类口述的规范句**原文照录**，Agent 的解释另起一行标注，两者不混。

**合并动作**：`FUTURE.md` §C8 与 `PLAN.md` §M4-W2 里原本各抄了一份格式清单 ——
现在都改成指向 `IMAGING.md` §1（C8 只留决策与代价：AVIF 编码实测、JXL 定位）；
`AGENTS.md` §10 索引加 `IMAGING.md` 一行，§6.5 开头指向它。

**两处事实纠正**（写进 §2 的醒目位置）：

* 缩略图**不在 `catalog.db`**，在缓存库 `thumbs.db`（`%LOCALAPPDATA%\raybend\cache\<库 id>\`）；
* 截断比例人类原口述 4:1，**当天确认「3:1 也没问题」** ⇒ 保持现状 3:1 不动
  （`MAX_DISPLAY_ASPECT = 3.0`，只作用于网格/胶片带）。

---

## 2. 按规格落地的四件事

### 2.1 小图必须**放大**（`IMAGING.md` §3.1-4）

人类澄清：tiles / film 的 tile 与总览的固定 4:3 幅面，都是**保持比例拉撑到显示区（含 padding）**；
大图缩小、**小图放大**，不是「小图保持原始像素」。

* 三处展示面的现状核对：网格 `Tile` = `object-cover` ✅、胶片带 = `object-contain` ✅
  （两者都是 `h-full w-full`，**会放大**）；只有 `PreviewFrame` 的**尺寸未知过渡态**
  用了 `max-h-full max-w-full` —— 那正是「小图不放大」的写法。
* 改：`src/components/ui/PreviewFrame.tsx` 过渡态 → `h-full w-full object-contain`（含注释写明口径）。
* 胶片带维持 `contain`（竖图完整显示，2026-09-19 定的）—— 两条都是「填满」，区别只在裁不裁。

### 2.2 preview 的生成节点（`IMAGING.md` §4）

* 规则本体抽成纯函数 `store::develop::needs_preview(choice, stack)`：**只有编辑过的（latest）才有 preview**，
  没编辑过时 SOOC / RAW 的内置位图就代替它（+ 4 条边界测试）。
* 新命令 `develop_preview_refresh(path)`（`src-tauri/src/develop.rs`）：
  复用 `thumbs::render_latest_cached`（**与 `view_image` 同一份**：命中只读、未命中才渲染并写），
  所以 preview 只有一条生成路径。返回 `false` = 没生成（没编辑过 / 资产找不到），**不是错误**。
* 接线（`EditorWorkspace`）：**进编辑**（照片解析出来时）与**退出编辑**（effect 的 cleanup，
  换照片 / 卸载都算）各调一次；落库那条路**先 commit 再刷新**（preview 读的是库里的栈）。
  它是后台那一路，不等它、失败只记日志。
* 存 issue 那个节点**没做** —— issue 体系还没实现（`IMAGING.md` §8 已记）。

### 2.3 编辑视口：拖动中只算预览档（人类 2026-09-24 追加）

人类问「调拉杆时只对展示的像素处理 + 限流 + 释放鼠标才对全图处理」有没有做。核查结论：

* **限流** ✅ 两层本来就有：前端 `createLatestCoalescer`（按帧合并 + 同值不发）、
  后端 `develop_loop` 收任务前 `merge_jobs`（队列里被顶替的合并掉）+ 渲染线程只认最新任务号；
* **只算展示所需分辨率** ⚠️ 只有「按缩放」那一半（`tier_for`：zoom ≥ 1 升 Full、< 0.5 退回，带迟滞）；
* **松手才全图** ❌ 没有 —— 松手只写库 + 作废缓存 + 刷缩略图。

补齐：新增纯函数 `render::tier::tier_for_params(interactive, zoom, current)`
（拖动中**一律 Preview**，哪怕 1:1；松手按缩放重新算 ⇒ 要全尺寸就给全尺寸）+ 2 条测试。
链路：`DevelopParamsPayload.interactive`（前端 store 的 `paramDragging` 信号）
→ `DevelopParamsDto.interactive` → `RenderCommand::SetParams { interactive }` → 档位。

* 滑杆：Zag 的 slider **没有** `onValueChangeStart`（只有 `onValueChange` / `onValueChangeEnd`），
  所以起点认在**第一次值变化**上（点标签/空白不会误报），终点用 Zag 的 end（带指针捕获）；
* 曲线：`pointerdown` 抓到点 / 加点时开始、`pointerup` 结束（先松「拖动中」再落库）；
* 防卡死：换照片（`loadDevelop`）时**强制清掉**拖动标志 —— 否则「拖到一半换图」会让画面永远偏软。

### 2.4 载入提示改半透毛玻璃（`IMAGING.md` §5）

编辑视口那条提示改成与全屏模式同观感的毛玻璃面板（`bg-surface-layer/70 backdrop-blur-md` + 转圈），
并显式写清两条纪律：**只遮 view**（这个容器就是洞口，胶片带/面板照常可点）、
**`pointer-events-none`**（连 view 里的拖动缩放也不挡）。
「进来先读 preview」那半**没做** —— 它要 AVIF 解码器（见 §4）。

---

## 3. 涉及文件

| 文件 | 改动 |
| --- | --- |
| `IMAGING.md` | **新增**（规格唯一事实源，含现状对照与未决点） |
| `FUTURE.md` | §C8 改成指向 IMAGING；§D1.5 记录「拖动中只算预览档 + 限流」已补、仍缺视口裁切与分期缓存 |
| `PLAN.md` | §M4-W2 格式清单改成指向 IMAGING §1 |
| `AGENTS.md` | §10 索引加 `IMAGING.md`；§6.5 指向它 |
| `src/components/ui/PreviewFrame.tsx` | 过渡态小图放大（`max-*` → `h-full w-full`） |
| `src/features/editor/viewport.tsx` | 载入提示改毛玻璃（+ 转圈图标，去掉不再用的 `PendingNote` 导入） |
| `src-tauri/src/develop.rs` | 新命令 `develop_preview_refresh` |
| `src-tauri/src/thumbs.rs` | `render_latest_cached` 改 `pub(crate)`（两条路共用一份） |
| `src-tauri/src/lib.rs` | 注册新命令 |
| `crates/raybend/src/store/develop.rs` | `needs_preview` + 测试 |
| `crates/raybend/src/render/tier.rs` | `tier_for_params` + 2 条测试 |
| `crates/raybend/src/render/mod.rs` | 重导出 `tier_for_params` |
| `src-tauri/src/editor.rs` | `DevelopParamsDto.interactive`、`RenderCommand::SetParams { interactive }`、档位选择 |
| `src/api/types.ts` / `src/api/editor.ts` | 载荷字段 + `refreshDevelopPreview` |
| `src/features/editor/store.ts` | `paramDragging` 信号 + begin/end + 载荷字段 + `loadDevelop` 清标志 |
| `src/features/editor/SliderRow.tsx` / `CurveEditor.tsx` / `panels.tsx` | 拖拽起止接线 |
| `src/features/editor/store.test.ts` | **新增** 3 条测试（载荷标志 / 换图清标志 / 标志不掺进 values） |
| `src/workspaces/editor/EditorWorkspace.tsx` | 进/出编辑两个节点调 preview 刷新 |

---

## 4. 验证

* `cargo test -q -p raybend --lib` → **949 passed / 1 ignored**（946 → 949：`needs_preview` 1 条 + `tier_for_params` 2 条）
* `cargo test -q -p raybend-desktop --lib` → **66 passed**
* `cargo clippy -q -p raybend -p raybend-desktop --all-targets` → 只剩既有那条
  （`store/backfill.rs:396 very complex type`）
* `pnpm test` → **906 passed**（903 → 906：store.test.ts 3 条）
* `npx tsc --noEmit -p tsconfig.json` → **0 错**
* `pnpm lint:colors` / `lint:arch` / `lint:i18n` → 全干净
* `pnpm check:browse`（起 dev server 后跑）→ ✓「库非空时进浏览正常」（含 §2.17 的滑块拖动 +
  横向适合窗口两条判据 —— 这次动过 `PreviewFrame`，这条回归是必须跑的）

**未经人类验证（真机 E2E，`AGENTS.md` §2.8）**：拖杆时画面是否「跟手但偏软、松手变锐」的观感、
毛玻璃提示的实际效果、1:1 下拖动与松手的档位切换体感、preview 在退出编辑后是否立刻可用、
小图（缩略图小于 tile）在网格/胶片带里的放大效果。

---

## 5. 遗留 / 未做（都登记在 `IMAGING.md` §8/§9）

1. **「进来先读 preview」**：要 Rust 侧能解 AVIF（`image` 只有编码器）—— 新增依赖，
   按 `AGENTS.md` §2.9 要先与人类对齐选型（`avif-native`/dav1d vs 别的，或先用 DOM 覆盖层）。
2. **AVIF / HEIC 导入**仍是占位图（同一件事：缺解码器）。
3. **存 issue 时生成 preview**：等 issue 体系（切 issue 的语义已写进 §3.2）。
4. **1:1 只算可见区域**（视口裁切渲染）：架构级，`FUTURE.md` §D1.5 登记。
5. **RAW + JPG 成对时 preview 从哪个文件渲染**：现在两条路都按显示路径（JPG）渲染，
   而编辑器编的是 `_RAW/` 那个 RAW —— 观感会略有差异。列进 `IMAGING.md` §9 待拍板。
6. `design/editor.pen` 里「缩放控制 + 信息三组 + 毛玻璃提示」还没画（Pencil 要人类把该文件打开）。
