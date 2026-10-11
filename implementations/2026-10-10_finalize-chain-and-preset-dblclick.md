完成时间：2026-10-10 13:28:34 +0800

# 预设双击应用 + 定稿链路提速与去闪

## 范围与依据

崔总 2026-10-10 口述（延续同日上一批 `implementations/2026-10-10_editor-presets-and-fade-truncation.md`）：

1. 预设支持**双击直接应用**；
2. 定稿生成预览「一次花了将近 1 分钟」，且**生成过程中定稿面板几次闪烁** ——「感觉逻辑链路可能不太对，检查一下」。

## 诊断（先量数、再看链）

### 耗时：BM3D × 两次渲染，全在同步链上

用现成探针量（本机、release；数字真跑）：

| 探针 | 输入 | 结果 |
| --- | --- | --- |
| `cargo run -p raybend --release --example bm3d-probe 1920 1280` | 7.37M samples | **3.96s** |
| `cargo run -p raybend --release --example bm3d-probe 3840 2560` | 29.49M samples | **16.02s**（线性） |
| `cargo run -p raybend --example develop-probe`（dev 档） | 1:1 6000×4000 全部阶段 | 1.0s；降噪(60) 0.48s |

关键事实：`thumbnail::render::geometry_input_edge(Screen) = 1920 × RAW_OVERSAMPLE(2) = 3840` ——
**预览档的整条管线（含高质量降噪 BM3D）跑在 3840 输入边上（约 9.8MP）**，一趟约 16s。

链路（修复前）：

1. `confirmEdit` 把 `refreshDevelopPreview` **await 在落库串行链里** —— 每松一次拉杆就排一趟整渲染，
   连续调节排一串；`persist` 串行化，排队的谁也跑不掉；
2. `finalize()` 又 `await persistTail`，把上面排着的预览全等完才创建定稿；
3. `issue_create`（Rust）**同步**做 `ensure_snapshots`：Screen 缓存只认定稿自己的名字，
   于是**把同一张画面再渲一遍** + 解码 + grid/strip 两次 AVIF 编码，然后才回列表。

即：一次定稿最坏要等 **2 趟 3840 管线（约 32s）+ 3 次 AVIF**，再叠上排队在前的若干趟 —— 与「近 1 分钟」吻合。

### 闪烁：缩略图键集体重建 + 占位条目一开一关

- `confirmEdit` 的 `.then()` 每次落库都 `setIssueThumbRevision(+1)`；`issueThumbKey` 把修订号编进**所有**
  定稿缩略图的 key → 面板里每一行的 `IssueThumb` 都换 key；旧实现 `Show when={thumb().url}` 在新图
  就绪前把 `<img>` 整个撤掉 → **整列闪**（且新图要重新 IPC 取，闪的时间不短）；
- 「占位先行」条目在创建**一开始**就插进列表、创建返回才撤 —— 在旧链路里它要挂几十秒，
  期间列表顶部一直占着一行，接替时又是一次跳变。

## 修复

### 前端

1. **预览刷新离开落库链**：新增 `src/lib/preview-queue.ts`（**单飞 + 最新者优先**；带「落库闸门」——
   闸门失败就**不渲染**，绝不用旧栈写一张过期预览）。`EditorWorkspace` 四个入口全部改走它：
   提交（闸门 = 当前 `persistTail`）、撤销重读、进/出编辑、换照片前的兜底落库。
   落库链现在只装 commit（毫秒级），`finalize` 的 `await persistTail` 不再等渲染。
   总览图的刷新挂在预览 promise 之后（预览落地再刷，避免它按需再渲一遍）。
2. **旧图留守**（`panels.tsx::IssueThumb`）：新 key 的图就绪前把上一张留在原位，只有**换照片**
   （key 里的 repo/asset 变了）才清空 —— 任何原因的重取都不再闪白。
3. **修订号只推给会变的三类**（`issueThumbKey`）：sooc / raw / latest 照旧；命名定稿是不可变快照，
   key 不再跟着落库重建（同时保留换照片、源小图就绪时的刷新语义）。
4. **占位条目 300ms 才现身**（同切定稿提示的口径）：`issue_create` 现在几十毫秒就返回，
   立刻插占位反而是一闪；真慢（库忙 / IPC 卡）时它照旧出现。
5. **预设双击应用**（`preset-panel.tsx`）：双击预设行 = 应用这一条，与工具行「应用」同一条路
   （`props.applyPreset` → 落库 confirm）；`props.enabled` 与拖拽抑制时不清。

### Rust（`src-tauri/src/issues.rs`）

1. **`issue_create` 不再在响应里等渲染**：写库 + 排 sidecar 同步后立刻返回整库；
   `ensure_snapshots` 移进后台线程。`snapshot_error` 因此不再有同步来源（见「遗留」）。
2. **同一张图不渲两遍**：新增 `reuse_latest_preview` —— 定稿那一刻 issue 栈与 latest 相同，
   把已经/正在生成的 latest 预览**复制**为定稿预览。等待策略分两档：
   - 定稿创建的后台任务：最多等 30s（150ms 纯 sleep 轮询，不吃 CPU；期间感知库退场，
     退场立刻放弃，不把 `TaskPermit` 押满超时）；
   - 按需路径（对比 / 全屏读预览）：只查一次（`wait_ms = 0`），等不到直接渲，不拖用户；
   - 每次拿到文件都**重读库栈比对**：latest 被继续编辑后不再复用（定稿快照必须永远是它自己的画面）。

## 关键决策与理由

- **预览渲染用「队列 + 闸门」而不是「取消/版本号」**：Rust 侧渲染不接取消，队列只要保证
  ①同一时刻一张 ②排队只留最后一张 ③渲染前落库已完成，就同时解决排队积压与「旧栈写过期预览」。
- **定稿快照后台化而不是缩短等待**：在 3840 边上跑 BM3D 是**本质成本**（16s/趟），
  能做的只有「不重复渲」「不让用户等」。用户要的是条目立刻出现 —— 「占位先行」的原始设计就是
  这个意图（`implementations/2026-09-29_finalize-pending-placeholder.md`），本轮只是让实现追上它。
- **复用而非删路径**：`reuse_latest_preview` 复用已有 `FullCache`「latest」名字与既有
  `render_profile_thumb` 的 preview 复用口径，不新造缓存通道。
- **双击不绕过守卫**：双击与工具行「应用」走**同一个** `applyPreset`，不复制一份应用逻辑（§2.12）。

## 涉及文件

- 前端新增：`src/lib/preview-queue.ts`、`src/lib/preview-queue.test.ts`（4 条）。
- 前端改动：`src/workspaces/editor/EditorWorkspace.tsx`（队列接线、缩略图 key、占位计时）、
  `src/features/editor/panels.tsx`（`IssueThumb` 旧图留守）、`src/features/editor/preset-panel.tsx`（双击）。
- Rust：`src-tauri/src/issues.rs`（`issue_create`、`ensure_snapshots`、`reuse_latest_preview`）。
- 文档：`memory/FUNCTION-IMAGING.md`（§2 表 + 2026-10-10 段）、`specs/editor-presets.md`、
  `design/editor.md`（双击应用）。

## 验证（Agent 冒烟）

- `pnpm typecheck` 退出码 0；`pnpm test` **1246 通过 / 0 失败**（新增 4 条队列纯逻辑）；
  `lint:colors` / `lint:arch` / `lint:i18n` 全过；`pnpm build` 通过（10.18s）。
- `cargo test --workspace --offline`：raybend lib **1472 通过 / 12 忽略 / 0 失败** + desktop 120 + 其余全过。
- `cargo clippy -p raybend-desktop --offline --all-targets`：`issues.rs` 无告警。
- `pnpm smoke:ui`（dev server + Chromium）：`problems` 为空。
- 本轮**未做**：真机（Windows/Tauri）上的定稿手感 —— 按纪律归人类 E2E。

## 遗留与待真机

1. **`snapshot_error` 可见性变化**：后台快照失败不再同步回给界面（只 `eprintln!` 一行）；
   真正去看这份定稿（对比 / 全屏）时按需路径会再报一次错。若崔总希望「面板上直接标一条失败」，
   需要一条后台→界面的状态通道（本轮没做，先记录）。
2. 小图仍可能**双份渲染**（面板先按需渲 strip，后台任务再算 Screen），但 strip 档极小（BM3D 约 0.1–0.5s），
   不构成耗时；且后台任务发现小图已缓存会跳过。
3. 30s 等待期内持有 `TaskPermit`：已做「库退场即放弃」，但若真机上遇到「切库时被定稿等待拖住」
   的体感，把 `LATEST_PREVIEW_WAIT_MS` 调小即可（常量在 `src-tauri/src/issues.rs` 顶部）。
4. 真机待验：双击应用的确认/落库节奏（与工具栏应用一致否）、高质量降噪下定稿「条目即现 + 缩略图稍后到」
   的观感、滚动定稿列表时缩略图不再闪。
