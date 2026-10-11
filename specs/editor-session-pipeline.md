# specs/editor-session-pipeline.md — 编辑会话状态与显影/预览管道（整合工作计划表）

> **性质**：这是**一个工作单元的整体计划表**（含 5 个波次），不是单波次规格。
> 按 §5.4，开工时可整批做，也可把某一波次单独细化；**进度只记 `todo` 工具**，本文件的勾选项是
> **验收清单**（做完即勾、状态如实）。
> **立项**：崔总 2026-10-10 口述四条现象（对比参照标签不符 / 切图有快有慢 / 切图先出「版本不对」的图 /
> issue 预览慢）+ 定案「白平衡在预设里存偏移量」。
> **依据**：本次会话逐条核实的代码链路与真机旁证（只读查了
> `%LOCALAPPDATA%\com.cthun.raybend\app.db` 与 `C:\src\tmp\testrespos\catalog.db` 的副本）。
> **状态**：待开工（尚未改任何代码）。

---

## 0. 背景：四条现象与三个结构性成因

四条现象都不是孤立 bug，而是同一个系统的四种表现形式。

| # | 现象（崔总原话） | 已核实的链路 | 归到 |
| --- | --- | --- | --- |
| 1 | 对比参照标签写「SOOC」，实际比对的是 RAW 原片 | 工作路径（有 `color_state`）的 SOOC 参照源**只在带过渡计划的任务里**填（`src-tauri/src/editor_working.rs:133`），任何「无计划」的缓存重建都静默丢掉它；`SetReferenceBase` 触发的 `queue_geometry_job` 恰好是这种任务（`src-tauri/src/editor.rs:2687`、`:2490`）；界面标签只读前端信号，不看后端真值 `RenderState.referenceBase`（`src/api/types.ts:875`） | W3 |
| 2 | 切图有的很快、有的很慢（设置相同） | ① V2 路径**没有过渡帧**（`editor_working::run` 不接 commands，末尾 `transition: false`）；② V2 路径**任何显式照片任务都整张重解码**（`editor_working.rs:95-103` 的缓存重建条件里带着 `job.photo.is_some()`）；③ 全进程**一条** RAW 管道（`crates/raybend/src/raw/worker.rs:56-67` 的 Mutex）无优先级；④ 每次落库/切图挂一串后台全渲染（`previews.push`、`prepareIssueSources`、胶片带缩略图、issue 快照） | W1 W2 |
| 3 | 切过去先出一张「版本不对」的图（一张裁切比例被拉伸、一张白平衡不对） | ① 过渡帧逻辑尺寸取**原图尺寸**（`editor.rs:3339-3340`），像素却可能是**已裁切**的 latest 快照 → 拉伸；真帧到达时 `set_source_size` refit（`render/gpu.rs:517-525`）才恢复；② 候选含 SOOC/RAW（必然是"别的版本"：相机白平衡、无裁切、无 LUT）；③ latest 快照在**每次落库**被整目录删除（`develop.rs:434-442`）→ 刚编辑完切回，多半只剩未编辑版本；④ 前端在切图瞬间把 as-shot 基线写 null（`EditorWorkspace.tsx:575-578`），而管线白平衡是 `temperature_gain_ratio(target, baseline)`（`develop/pipeline.rs:258-261`）→ 首帧按静态默认 6250K 算，解码真值到达后才恢复 | W1 W2 W4 |
| 4 | （另一 agent 在处理）issue 生成预览图慢 | 同源：同一条 RAW 管道 + 同一条「V2 在 3840 输入边跑全管线再缩到 1920」的重路径（`thumbnail/render.rs:437-447` + `render_working(..., 0)`）；且 `ensure_snapshots` 会 sleep-轮询最多 30s 等编辑器排队产出的 latest 预览（`issues.rs` 的 `LATEST_PREVIEW_WAIT_MS`） | W2 |

**三个结构性成因**（本计划要真正解决的东西）：

- **S1「当前在编这张」没有单一权威**：前端 store、渲染线程 `SessionParams`、显影线程的 `cached/working`
  各存一份，靠约定同步；任一处缺失就**静默降级**（参照源、as-shot 基线、预览版本，全是这个形状）。
- **S2 重活没有队列与优先级**：一条全局 RAW 管道 + 每次落库引一串「全渲染」，前台永远排在后台后面。
- **S3 快路径没有版本契约**：过渡帧只承诺「有图先给」，不承诺「和真帧同版本、同画幅」；
  而且它的投递还与参数任务抢「最新任务号」（`Developed` 里 `outcome.id != *latest_job` 直接丢），时灵时不灵。

---

## 1. 目标（可验证）与非目标

### 1.1 目标

| # | 目标 | 判定方式 |
| --- | --- | --- |
| G1 | **画出来的帧永不是「别的版本」**：任何过渡帧都与真帧同画幅（逻辑尺寸 == 真帧将报的尺寸），且来源能自证与当前编辑栈同版本 | 单测 + 真机目视（裁切照片来回切不拉伸、不跳变） |
| G2 | **切图代价可预期**：与"后台在干什么"解耦；V2 与 legacy 两条路的代价同阶；几何/参照操作不再整张重解码 | 日志（无 `decode_ms` 的帧 / 解码次数）+ 压力场景手测 |
| G3 | **白平衡口径同一**：编辑器视口、预览、缩略图、导出用同一份 as-shot 基线；白平衡不再因"基线未知"或"跨照片"被污染 | 单测 + DB 断言（`as_shot_k` 与温度同源）+ 真机目视 |
| G4 | **对比参照不再静默降级**：标签与实际来源一致（含"没有位图"这类降级） | 单测 + 真机目视 |
| G5 | **结构收敛**：两条显影路的重复骨架合成一份；预览快照命名自证版本（不再靠"删文件"来保证一致性） | 代码审查 + 既有测试全绿（行为等价） |
| G6 | **定稿图自愈**：某张照片的定稿图缺了（创建失败 / 中途退出 / 缓存被清），进 editor 或选中它时自动补齐；全齐时零渲染 | 单测（缺项判定 + 幂等）+ 真机手工删缓存验证 |

### 1.2 非目标（本期明确不做）

- 不动色彩管理 / 工作色彩域（`linear_rec2020_v2` 的输入策略、ICC 解析都不改）。
- **不改 XMP 映射**：`temperature` 仍然不写 sidecar（`crates/raybend/src/xmp/mapping.rs:4`、
  `specs/xmp-sidecar.md` §5）；本期白平衡表示的变化只发生在**预设 payload** 与编辑器会话内，
  编辑器落库的 `develop_stacks` 字段形状不变（§2.19 的评估结论就写在这里）。
- 不改照片原文件、不改导出像素口径（导出只换调度通道）。
- 不重写显影管线、不做 GPU 化、不新增相机/格式支持。
- 不做任何「画质/分辨率档位」的用户可见设置项（2026-10-11：看图没有档位，一律原图）。

---

## 2. 波次总览（顺序即依赖）

| 波次 | 内容 | 依赖 | 规模 | 主要风险 | 交付物 |
| --- | --- | --- | --- | --- | --- |
| **W1** | 过渡帧契约：同画幅 + 版本分层 + V2 接入 + 投递可靠 | — | 中 | 渲染线程投递规则改动可能带来陈旧帧 | 代码 + 单测 + `implementations/` 记录 |
| **W2** | 管道与缓存：预览快照按栈指纹命名、不再整目录作废、后台重活队列、worker 优先级、V2 重建判据、解码预取 | W1（命名配合） | 大 | 缓存 GC 策略、队列改造面广 | 代码 + 单测 + 压力冒烟 |
| **W3** | 会话状态收敛：SOOC 参照源按 asset 解析、对比参照真值上屏、两路判据统一 | W2 部分 | 中 | 前端标签语义变化（设计稿已要求） | 代码 + 单测 + 设计说明同步 |
| **W4** | 白平衡：渲染侧以解码真值为基线 + 落库一致性 + **预设存偏移量（v3）** + 旧预设迁移 | 可并行 | 中 | payload 版本升级 / 旧预设观感变化（需崔总确认） | 代码 + 双端契约测试 + `specs/editor-presets.md` §3 更新 |
| **W5** | 定稿图像自愈：进 editor / 选中定稿时扫描该照片所有定稿的三档图，缺什么补什么（静默、幂等、低优先） | 与 W2 共享队列（可先落地） | 小 | 与前台抢重活（必须走低优先 + 去重） | 代码 + 单测 + 验收 |
| **W6** | 收尾：两条显影路骨架合一、文档、性能证据、真机验收清单 | W1–W5 | 中 | 结构抽取必须行为等价 | 代码 + 文档 + 验收清单 |

**开工建议**：W1 → W4（并行，独立面）→ W2 → W3 → W5 → W6。
W1/W4 完成后真机观感应立刻改善；W2/W3 解决"慢"与"静默降级"；W5 补定稿图的自愈；W6 只做结构收敛（行为等价）。

---

## 3. W1　过渡帧契约

### 3.1 口径（三条，定案句）

1. **同版本**：过渡帧的来源必须能自证与"真帧将要呈现的那份编辑栈"同版本，分三层：
   - **L1**：`latest-<base>` 快照存在且**编辑栈非空** → 用它（W2 之后按栈指纹精确匹配，见 §4.2）。
   - **L2**：**编辑栈为空**（未编辑） → 允许基准图（SOOC 位图，或 RAW 内嵌预览）。
     此时真帧 = 该基准的中性显影，属于"近似同版本"，与今天的 `preview-sooc` / `preview-raw` 一致。
   - **L3**：**有编辑栈但没有 L1 快照** → **基准图兜底**（2026-10-10 崔总追加：
     「就算没生成……至少比空屏好」）。兜底帧必须做到两件事，否则退回「不出帧」：
     ① **套上编辑栈的几何**（同一套裁切 / 旋转 —— 画幅与真帧一致，不被拉伸）；
     ② **强制软化**（`amount ≥ 0.35`，缩小档也看得出是临时图，不冒充编辑结果）。
     旧行为（拿未套几何的 SOOC/RAW 冒充编辑结果）仍然废除 —— 废除的是「冒充」，
     不是「给一张临时图」。
2. **同画幅**：过渡帧的逻辑尺寸 = `geometry.output_size(原图尺寸)`，即**真帧将报的
   `source_width/source_height`**；`original_image` 单独报原图尺寸。不得再拿原图尺寸当逻辑尺寸。
3. **投递可靠**：过渡帧不再受"最新任务号"判据支配。渲染线程对过渡帧的接受条件：
   `path == state.photo_path` **且** 该 path 的真帧尚未应用（`decode != "ready"` 或 `painted_path != path`）；
   真帧一到，之后到达的过渡帧一律丢弃。

### 3.2 改动清单（文件级，实现时按当时代码复核）

- `src-tauri/src/editor.rs`
  - `transition_plan()`：新增 `output_size`（用命令层已读到的栈 + `EditGeometry::output_size`，
    输入是 `read_photo_meta` 的原图尺寸；`geometry.is_identity()` 时等于原图尺寸）。
  - `transition_outcome()`：`DevelopedImage.source_width/height` 用 `plan.output_size`，
    `original_width/height` 用原图尺寸；日志行保留并补上两者（便于真机核对）。
  - `transition_sources()`：新增 `stack_is_empty` 入参；**非空栈不推 SOOC/RAW 候选**。
  - 过渡帧投递：从「与 `latest_job` 共用 id 判据」改为独立通道/独立序号
    （例如 `RenderCommand::TransitionFrame` 或 `Developed` 上带 `transition: true` + 新的接受规则），
    真帧路径（`Developed`）判据不动。
- `src-tauri/src/editor_working.rs`
  - `run()` 在 `decode_working_source` **之前**调用 `super::transition_outcome(...)` 并发帧
    （需要把 `commands: &Sender<RenderCommand>` 传进来；`develop_loop` 调用点改一行）
    —— **复用同一份实现**，不写第二份（§2.12）。
- `design/editor.md`：补一节「过渡帧口径」（同版本 + 同画幅；L2 的边界）。
- 前端：本波**不需要改动**（洞口那张图与真帧同一条显示链）。

### 3.3 不变量（写进注释，供后续守）

1. 过渡帧的 `source_width/height` 必须等于真帧的 `source_width/height`（同一次几何计算的口径）。
2. 过渡帧不携带 `histogram` / `as_shot_temperature` / `applied_params_rev`（不得冒充真帧；
   今天已经如此，保持）。
3. 过渡帧的 `decode` 状态仍为 `loading`（界面提示不提前撤掉）。

### 3.4 测试

- Rust 单测（`src-tauri/src/editor.rs` 的 tests 区）：
  - `transition_outcome` 用假 plan（含 `output_size`）断言 `source_*` 与 `original_*` 各归其位；
    裁切（3:2）与旋转两组数据。
  - `transition_sources`：顺序恒为 `latest → SOOC → RAW`；有编辑栈时基准图只作**兜底**
    （`stack_is_empty = false` 时 `transition_outcome` 会套几何 + 强制软化）。
  - 投递规则：构造「过渡帧晚于参数任务到达」的顺序，断言仍被画；真帧之后到达的过渡帧被丢。
    （若指令级单测不便，抽一个纯函数 `accepts_transition(state, frame)` 并在注释里说明。）
- 前端：无新增（不改前端）。
- 门禁：`cargo test --workspace`、`cargo clippy -p raybend -p raybend-desktop`、
  `pnpm typecheck && pnpm test && pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n && pnpm build`。

### 3.5 验收清单

- [ ] 裁切（含旋转）照片切换时首帧画幅正确，真帧到达**不发生** refit 跳变
- [ ] 有编辑栈、latest 快照缺失时**不出现**任何过渡帧（只有「载入中」）
- [ ] 未编辑照片仍有快帧（SOOC/RAW 基准），且真帧到达无画幅跳变
- [ ] V2（有 `color_state`）照片也走过渡帧（日志能看到 `[editor] 过渡帧：…`）
- [ ] 过渡帧不再因为随后的参数任务被丢弃（顺序构造用例 + 日志）
- [ ] `[editor] 过渡帧…` / `[editor] 真帧…` 诊断行保留且语义不变（真机排查用）

### 3.6 回退

投递规则改动风险最高。若真机出现"陈旧帧盖新帧"，回退到"只改画幅与候选分层（3.1 的 1、2）"，
投递按旧判据保留，并把问题转入 W2 的队列改造一起解决。

---

## 4. W2　管道与缓存（重活的调度）

### 4.1 目标

把"随手 `spawn_blocking` + 抢一把全局锁"变成**一条有优先级、去重、可取消的队列**，
并让预览快照**自证版本**（不再靠"落库时删光"来保证一致）。

### 4.2 预览快照按栈指纹命名（W1 的 L1 由此变成精确判定）

- **命名**：`latest-<profile_hash 前 12 位>-src<源签名>-<base>-v<pipeline>.avif`
  （hash 用 `issues::profile_hash` 同一套，和 `issue-<id>-<hash>` 对齐）。
- **落库不再整目录作废**：`develop.rs:434-442` 的 `FullCache::invalidate(asset_id)` 移除；
  旧文件因名字不同自然失配。
- **GC**：按资产保留最新 **2** 份 `latest-*`（按 mtime，其余删除）；`issue-*` 仍随 issue 生命周期
  （`FullCache::remove_issue`）。GC 在"写新快照之后"顺手做，不进热路径。
- **读取方同步**：`thumbs.rs::render_profile_thumb`（latest 变体）、`issues.rs::reuse_latest_preview`、
  `editor.rs::transition_plan` 三处都要带 hash 读/写——统一走一个 helper，不各写一份拼法。
- **验收**：落库后立刻切走再切回，首帧仍是**同版本**快照；缓存目录每个资产 ≤ 2 份 latest。

### 4.3 后台重活队列（Rust 侧一条）

- 新模块 `src-tauri/src/work.rs`：任务 = 闭包 + **优先级** + **key（去重）** + **取消令牌**。
  优先级建议：
  | 级别 | 内容 |
  | --- | --- |
  | P0 | 编辑器当前照片（换照片解码、过渡帧、参数帧） |
  | P1 | 交互等待（定稿按需预览、看图/全屏、全屏切换） |
  | P2 | 缩略图 / 1920 预览预热（`previews.push` 那条链） |
  | P3 | 后台批量（issue 快照、raw-original 小图、导出批、AI） |
- 接入点：`thumbs.rs`（`render_profile_thumb` / `render_latest_cached` / `prepare_source_images`）、
  `develop.rs::develop_preview_refresh`、`issues.rs::ensure_snapshots`、`export/*` 的预览路径。
- **与「issue 预览慢」那个会话的接口**（避免两边各造一份队列）：
  - 共用本队列；
  - 用**就绪登记表**（内存 `(asset_id, profile_hash) → Ready`）替代 `reuse_latest_preview` 的
    30s sleep 轮询；同一画面只渲一遍（key 去重）。
  - 对齐方式：本文件 §4.3 为接口，具体由先动手的一方落地，另一方接。

### 4.4 RAW worker 优先级

- `crates/raybend/src/raw/worker.rs`：新增 `acquire(priority)`（多级队列，高优先可插队），
  保留 `shared()` 作为默认 Normal 的兼容入口。
- 调用点标注优先级：编辑器解码/过渡帧 = High；缩略图/看图 = Normal；快照/批量/导出预览 = Low。
- 交付判据：一台机器上跑批量快照时，切图仍能在预期时间内出帧（压力场景手测 + 日志）。

### 4.5 V2 路径重建判据对齐 legacy

- `editor_working.rs:95-103`：缓存重建条件里的 `job.photo.is_some()` → 对齐 `should_reload_source`
  （路径 + 签名 + color state），并**同时**做 `Cached.sooc` 的 carry-over（见 W3）。
- 验收：V2 照片进/出裁切、切对比参照、确认工具**不再重解码**（日志无新 `decode_ms`）。

### 4.6 解码缓存与邻图预取（可分批）

- 现状：`develop_loop` 的 `cached` / `working` 各一份，换图必重解。
- 目标（按风险分批）：
  1. **先做**：当前照片 ±1 的**预览档**预取（1920 级，内存约 16MB/张），走 P2、可取消；
     连续扫胶片带时不排满（沿用现有 300ms 空闲节流口径）。
  2. **后做**：full 源小 LRU（最多 2 份，带内存预算上限）。
- 验收：胶片带前后各一张来回切换时，第二张的回访明显更快（同一会话内可感）。

### 4.7 测试与验收清单

- [ ] 预览快照命名带栈指纹；同栈重复渲染只写一份（key 去重）
- [ ] 落库后不整目录删缓存；GC 后每资产 ≤ 2 份 `latest-*`，磁盘不无限增长
- [ ] 队列：同 key 任务只跑一次；高优先任务在批量满载时可插队（压力用例）
- [ ] `issues.rs` 不再 sleep-轮询等 latest（有就绪登记表）
- [ ] V2 照片几何/参照操作不再整张重解码
- [ ] `cargo test --workspace` / `pnpm` 全门禁 / `pnpm smoke:ui` 全绿

---

## 5. W3　会话状态收敛（参照源与真值上屏）

### 5.1 SOOC 参照源按 asset 解析（不再挂在任务载荷上）

- 现状：`Cached.sooc` 只在带过渡计划的任务里填（`editor_working.rs:133`、`editor.rs:3046-3056`），
  无计划的重建就静默丢 → 对比参照退化成 RAW（现象 1）。
- 做法：
  - `raybend` 侧加"照片源解析"入口（`develop::ReferenceSources`：`sooc_source(asset)` +
    `has_bitmap`），编辑器会话（渲染线程）**按 asset 缓存一份**；
  - 渲染时不再依赖任务载体的偶然字段；
  - 兜底规则写死：`has_bitmap == false` → SOOC 参照**不可用**（不允许用 RAW 文件冒充 SOOC）。
- 顺带修掉 `edit_target(Sooc)` 的 `bitmap.or(raw)` 歧义在**参照**这条路上的影响
  （编辑基准的兜底语义不变，只把"参照可不可用"显式化）。

### 5.2 对比参照真值上屏

- `RenderState.referenceBase` 已是后端真值（`editor.rs:750-751`）：状态栏按它显示**实际**来源
  （设计稿 `design/editor.md:203`「SOOC 不存在时标出实际的 RAW 参考帧」）。
- `src/api/types.ts:876` 的类型放宽到含 `issue:<id>`。
- `EditorWorkspace.tsx:1575-1600`：菜单选中态仍按用户选择，标签按真值（降级时显示实际）。
  具体形态见 §9.2。
- 「同一选项不重发」（`lastCompareReferenceKey`）保留，但**允许用户对同一项"再点一次"强制重发**
  （把重复选择也当成一次意图；避免"点了没反应"）。

### 5.3 两条显影路的判据统一

- 复核 W2-4.5 的结果，把"何时重建缓存"的判据抽成共享函数（legacy / V2 同一份），并补一条共享单测。

### 5.4 验收清单

- [ ] 对比参照切 SOOC → 进裁切 → 退出 → 参照仍是 SOOC（不再退化成 RAW）
- [ ] 没有位图的照片：SOOC 项禁用且标签显示实际来源（RAW）
- [ ] 定稿参照（`issue:N`）与真值标签一致；切参照后旧帧不会覆盖新选择
- [ ] 对同一参照项再点一次能强制重发（不再"点了没反应"）

---

## 6. W4　白平衡（基线一致性 + 预设存偏移量）

### 6.1 现状与证据（本次调研核实）

| 事实 | 位置 / 证据 |
| --- | --- |
| 管线白平衡 = `temperature_gain_ratio(target_k, baseline_k)`，baseline = 这张照片的 as-shot K | `crates/raybend/src/develop/pipeline.rs:258-261`；`color.rs:183` |
| 编辑器视口用 **payload 带的值**当基线；预览/缩略图/导出用**解码真值** | `editor.rs`（`DevelopParams::from_values(values, as_shot)`）vs `display/output.rs::render_linear/render_working` 的 `shot` |
| 切图瞬间前端把基线写 null → 首帧按静态默认 6250K 算 | `EditorWorkspace.tsx:575-578`；静态默认 `params.rs:94` |
| 落库时 `as_shot_k` 取 payload 的值，可能写 null | `EditorWorkspace.tsx:828`；库里 asset 3：`as_shot_k NULL` + `temperature 5008.4043` |
| 预设按**绝对值**存白平衡 | `src/features/editor/store.ts:846-852`；`specs/editor-presets.md` §3 |
| 库里污染证据 | 预设「润彩」`temperature 5008.4043`（= asset 1/2/4 的 as-shot）→ 套进 asset 3；预设「复古」`temperature 4602.028`（= asset 5 的 as-shot）→ 套进 asset 6/7（它们自己的 as-shot 是 3280.57 / 3672.39，比值 1.40 / 1.25 → 明显偏暖） |

### 6.2 渲染侧口径：解码真值优先

- 显影线程在渲染前用**解码得到的 as-shot K** 覆盖 `params.as_shot_temperature()`
  （legacy：`entry.as_shot_temperature`；V2：`decode_working_source` 的 `shot`）。
- payload 的 `asShotTemperature` 退化为"解码还没有值时的占位"。
- **结果**：首帧就是正确白平衡；编辑器与预览/缩略图/导出同一口径；
  "基线为 null 的首帧"这条路径从根上消失。
- 测试：Rust 单测（同一条栈、两种基线输入 → 覆盖成同一结果的像素/参数断言）。

### 6.3 落库一致性

- 前端 `currentDevelopStack()`：`asShotK` 用"最近一次已知真值"；若 `asShot == null` 且温度是显式值 →
  **不写温度项**（视为跟随 as-shot）并提示一次。
- 后端 `develop_commit`：温度 dirty 且 `as_shot_k` 缺失 → 用会话里的解码真值回填；
  两边都拿不到 → 记诊断（不静默）。
- 验收：任何新落库的栈都满足"温度 dirty ⇒ `as_shot_k` 非空"，且 `as_shot_k` 等于该照片的解码真值。

### 6.4 预设白平衡改为偏移量（崔总 2026-10-10 定）

**定义（写死在一处，前后端同源）**：

```text
mired(K)      = 1_000_000 / K
Δmired        = mired(asShot) − mired(target)     // 正 = 更暖（target 更低 K）
target        = 1_000_000 / (mired(asShot) − Δmired)
范围          : target clamp 到 2500..10000（参数表口径，`params.rs:94`）
```

- 语义：预设记的是**相对拍摄色温的偏移**，套到另一张照片得到"同一种冷暖倾向"，而不是同一个绝对 K。
- **payload v3**（`specs/editor-presets.md` §3 是契约正文，本文件只引用形状）：

```jsonc
{
  "version": 3,
  "color": { "saturation": 4, "vibrance": 35, "wbMired": -120 }
  // 没有 temperature；wbMired: 0 = 跟随拍摄色温（未动过白平衡）
}
```

- **保存**（`buildPresetSnapshot`）：由当前 `temperature` + `asShot` 算 `wbMired`；
  `temperatureExplicit == false` → `wbMired = 0`（沿用今天"未动过用基线"的口径）。
- **应用**（`planPresetApply` + `applyPresetSnapshot`）：需要 `asShotK`；
  `asShotK === null` → **跳过白平衡项**（其它项照常应用）并提示一次（i18n）。
- **换算实现**：`src/lib/white-balance.ts`（纯函数 + 单测：往返、边界 clamp、asShot 未知、极端值）。
  本期 Rust 侧不需要换算（预设 payload 由前端构建/应用）→ **只写一处**；
  若实现中发现后端也需要，则两端共用同一组测试向量并在 `develop/color.rs` 加同款实现。
- 契约拥有者：`specs/editor-presets.md` §3 **同步更新**（v3 形状、规则、示例）；
  后端 `crates/raybend/src/store/presets.rs` 的版本集合加 3
  （`payload_version_contract_matches_frontend_cleaner` 测试同步）。

### 6.5 旧预设迁移（v1/v2 → v3）

- **读时规范化**（`preset_library` 后端 + 前端 `sanitizePresetSnapshot` 接受 v3）：
  旧 payload 的 `color.temperature` **按「跟随拍摄色温」处理**（删该项、`wbMired = 0`）。
- 理由：旧值存的是**当时那张照片的 as-shot**（DB 已证），当偏移量没有依据；按 as-shot 处理正好是它的本意。
- 面板**一次性提示**：`editor.preset.wbMigrated`（两语言）「旧预设的白平衡已按“跟随拍摄色温”处理，
  需要保留原效果请重新保存一次」（提示形态照现有轻提示，不拦操作）。
- DB 行不批量改写：下次保存该预设时自然写成 v3。

### 6.6 测试

- TS：`src/lib/white-balance.test.ts`（形状与边界）、`src/lib/presets.test.ts`（v3 清洗 / 应用 /
  迁移 / `asShotK === null` 跳过 / clamp）。
- Rust：`store/presets.rs` 版本契约测试（v3 通过、v4 拒绝）、迁移规范化（旧 payload → v3 形状）。
- 端到端契约：预设版本集合两侧一致（沿用既有契约测试的口径）。

### 6.7 验收清单

- [ ] 一张照片调过白平衡 → 存成预设 → 套到另一张（as-shot 不同的）照片：冷暖倾向一致，不再是"同一个 K"
- [ ] 未动过白平衡的预设（所有项默认）套到别的照片：白平衡跟随该照片的 as-shot（中性）
- [ ] `asShot` 未知时应用预设：白平衡项被跳过且提示一次，其它项照常
- [ ] 编辑器首帧白平衡正确（不再有几秒后的跳变）
- [ ] 新落库的栈满足"温度 dirty ⇒ as_shot_k 非空"
- [ ] 旧预设读得出来、能应用、有一次性提示
- [ ] XMP sidecar 无新增字段（`specs/xmp-sidecar.md` 的口径未变）

### 6.8 可选：存量数据修复（需崔总确认，会改变观感）

- 对象：`develop_params.temperature` 是显式值 **且** `as_shot_k IS NULL` 的栈（库里目前只有 asset 3）。
- 做法：把 `as_shot_k` 补成该照片的解码真值（asset 3 与 1/2/4 同场景，真值应为 `5008.404296875`）。
- 预期观感：该照片白平衡回到中性（= 预设原本想表达的"as-shot"）；当前（bug 造成的）偏低 K 的偏暖消**失**。
- 交付形态：一次性修复命令/脚本 + 修复前后的同一画面留证，交崔总裁决后再执行。

---

## 7. W5　定稿图像自愈与预热（issue 快照 / 缩略图补齐）

> **来源**：崔总 2026-10-10 口述 —— 「生成失败界面上也没警示了；至少要做到进某个图的 editor 时
> 扫描所有 issue 看有没有缺图、缺图重生，以及在 browse 中选中这个图的某个 issue 时
> 没有 thumb 要重生成；错误倒不一定要报出来，不影响主线给个自动修复的机会就行」。
> **前置**：另一 agent 2026-10-10 的「定稿链路提速与去闪」批已落地
> （`implementations/2026-10-10_finalize-chain-and-preset-dblclick.md`：`preview-queue` 单飞队列、
> `reuse_latest_preview` 复用、`IssueThumb` 旧图留守）—— 本波**只补自愈，不重做那一批**。

### 7.1 现状：已具备的自愈点与缺口

已具备（实现时**复用**，不要重写）：

| 能力 | 位置 | 说明 |
| --- | --- | --- |
| 定稿缩略图缺了就渲 | `src-tauri/src/issues.rs::thumb_bytes`（`issue_thumb_get` / `export_variant_image` 共用） | 未命中 → `render_profile_thumb` → 写缓存 → 读回；本来就幂等 ✔ |
| 定稿 1920 缺了就渲 | `issues.rs::preview_bytes` → `ensure_snapshots(wait_for_latest = false)` | 按需路径（对比 / 全屏 / browse 大图）不会干等 |
| 创建后后台补三张图 | `issues.rs::issue_create` → 后台线程 `ensure_snapshots(wait = true)` | 与 latest 预览复用，避免同画面渲两遍 |
| 面板缩略图不闪 | `panels.tsx::IssueThumb`（旧图留守）+ `issueThumbKey`（修订号不推给命名定稿） | 2026-10-10 那批的成果 |

缺口（本波要补的）：

1. **进 editor 只保证 strip 缩略图**：定稿列表是普通 `<For>`，逐行挂载各请求一次 `strip`；
   **1920 预览与 grid 没人预热** —— 第一次去对比 / 全屏 / browse 看它时才现算（高质量降噪下十几秒）。
2. **创建后的后台任务是「一次性」**：失败、或进程在渲染完成前退出 → 该定稿的三张图永久缺失
   （`issue_create` 里只 `eprintln!` 一行，没有任何补做入口）。
3. **缺什么没人扫描、也没有去重**：面板 / 对比 / 全屏 / browse tiles / 导出各自按需渲，
   同一张画面可能被多次整渲，也没有一个「这张照片的定稿图齐了没」的廉价判定。

### 7.2 口径（三条）

1. **一次扫描、只补缺的**：按 `(库, 资产)` 扫描该照片**所有定稿**的三档产物
   （`screen` 1920 / `grid` 384 / `strip` 192），只重建缺失项；全齐时只读缓存与文件状态，毫秒级返回。
2. **顺序固定 Screen → Grid → Strip**：小图能从 1920 预览缩出来（`render_profile_thumb` 的既有口径），
   先渲 Screen 再渲小图 = **每张照片只付一次整管线**（今天按需路径里 strip 先到会各自整渲一遍）。
3. **静默自愈、不拦主线**：失败只写日志 + 记诊断，不回传给界面；下一次进入 / 选中时再试。
   界面**不新增**任何「生成失败」的常驻提示（崔总口径：不影响主线，自动修复即可）。

### 7.3 改动清单

- `src-tauri/src/issues.rs`
  - 新命令 `issue_snapshots_ensure(repository_id, asset_id, sizes: Option<Vec<String>>)`（缺省三档）：
    - 列该资产的定稿（`raybend::store::issues::list`）；
    - 逐个判缺：`screen` 用 `FullCache::read(asset, preview_name(issue, sig), base, PIPELINE_VERSION)`；
      `grid` / `strip` 用 `thumb_bytes` 同一份 key 拼法查 `thumbs` 缓存库；
    - 缺 `screen` → 走既有 `ensure_snapshots(app, repo, &issue, false)`（**不等待** latest）；
    - 缺小图 → 走既有 `thumb_bytes(...)`（它自己会从 Screen 缩，或整渲）；
    - 返回只包含**这次真补了**的条目的列表（`{ issue_id, sizes }`）；单条失败记日志后跳过，不影响其它定稿。
  - **复用而非重写**：判定后的生成、写缓存、命名全部走现有函数（§2.12）；不新造缓存键。
  - **去重**：命令层一张 `(repository_id, asset_id)` 的 single-flight / 代际闸门
    （照 `src-tauri/src/editor.rs::RequestGate` 的口径，必要时抽成可复用工具），
    同一照片并发进入只跑一次；W2 落地后换成共享队列的 P2/P3 任务（key 同一对）。
  - 注册进 `src-tauri/src/lib.rs` 的 `generate_handler!`。
- `src/api/issues.ts`：`ensureIssueSnapshots(repositoryId, assetId)` 封装（非 Tauri 运行时直接返回空）。
- `src/workspaces/editor/EditorWorkspace.tsx`
  - 在**定稿库就绪 / 换照片**那条 effect 里（60ms 防抖之后）追加一次调用；
    时机与 `prepareIssueSources` 同一条空闲节流口径（真帧就绪后 300ms），不占前台。
  - 拿到「补过」的列表后，把对应缩略图 key 交给 `issueThumbs.refresh(...)`（旧图留守，不闪）。
- `src/features/browse/display-variants.ts`（或 browse 的变体选择处）
  - 选中 `issue:N` 变体后触发同一命令（scoped 到该资产）：保证该定稿三档齐。
    按需路径已能渲「被请求的那一档」，这一步是**补齐其余 + 给失败一次重试**。
- 文档：`memory/FUNCTION-IMAGING.md` 的「定稿三档图」生成节点补一条「进入 / 选中时自愈」；
  `design/editor.md` 无视觉改动（没有新控件）。

### 7.4 测试

- Rust 单测（`src-tauri/src/issues.rs` 的 tests 区）：
  - **缺项判定**（抽成纯函数）：三档全有 → 空；缺 screen → 只报 screen；缺小图 → 只报小图；
    定稿已删除 / 源文件不可用 → 跳过该条且不报错。
  - **幂等**：连跑两次，第二次结果为空且不触发渲染（计数桩）。
  - **顺序**：同时缺 screen 与小图时先补 screen（小图随后从预览缩，不额外整渲）。
- 前端：调用点若加节流，配一条队列单测；`refresh` 不应把未变的条目重新拉取（沿用队列既有语义）。
- 冒烟：`pnpm smoke:ui`；真机手工删掉某定稿的缓存文件 → 进 editor → 图自己回来。

### 7.5 验收清单

- [ ] 进 editor：该照片所有定稿的三档图齐全（缺的自动补齐；已齐时零渲染、毫秒返回）
- [ ] browse 里选中某个定稿变体：其 thumb 与 1920 都在（缺的自动补，不卡交互）
- [ ] 定稿生成失败 / 中途退出：下一次进 editor 或选中时自动补上
- [ ] 补齐的渲染走低优先（不抢当前照片的解码 / 参数；连续扫胶片带不排满）
- [ ] 补齐完成后面板该定稿缩略图**不闪**且显示新图
- [ ] 失败仍是「静默 + 日志」，界面不新增常驻错误提示
- [ ] 幂等：同一张照片重复进入不产生重复渲染（日志 / 计数可证）

### 7.6 与 W2 的关系

- W2 先落地 → 本波只加「扫描 + 触发 + 前端 refresh」，渲染走共享队列（P2/P3）；
- 本波先落地 → 先用 single-flight 闸门，W2 时把它换成队列任务（对外接口不变）。
- 自愈路径一律 `wait_for_latest = false`（不等待、直接渲）；「等 latest」只属于创建时那一次。
- **面板小图与自愈的先后**：面板一挂载就按行请求 `strip`，可能排在自愈的 `screen` 之前（那样小图会各自整渲一遍）。
  W2 的队列落地后，按 `(资产, 定稿)` 同 key 去重 + 依赖（小图等同一张的 screen 落地）即可消掉这份重复；
  队列未落地时不做额外工程（今天本就有这层浪费，不属于本波回归）。

---

## 8. W6　收尾（结构收敛与交付）

1. **两条显影路的骨架合一**（行为等价，不带新行为）：
   把「任务 → 取源 → 参数/曲线 → 参照 → 直方图 → outcome」这段骨架抽成一份
   （legacy 与 V2 只提供 `decode_source` 与 `render` 两个钩子）。
   验收：`cargo test --workspace` 全绿 + 离屏 GPU 冒烟不变 + 真机观感无变化。
   **不做**像素路径合并（V2 是未来主线）。
2. **文档**：
   - `memory/ARCHITECTURE.md`：新增「编辑器会话与重活调度」一节（权威状态、队列与优先级、缓存命名）。
   - `memory/FUNCTION-IMAGING.md`：预览快照命名与生成节点更新（W2 之后的口径）。
   - `AGENTS.md` §6.5：缓存红线里 `latest` 的命名口径一句话更新。
   - `design/editor.md`：过渡帧口径（W1 已加）+ 对比参照实际来源的显示（W3）。
   - `specs/xmp-sidecar.md`：确认「`temperature` 不映射」的声明在本期后依然成立（§2.19 的落档）。
3. **性能证据**：同一批照片、同设置下的切图时序（`[editor] 真帧…` 的解码/管线 ms、过渡帧耗时）
   W1 前 / W2 后各记一份，进 `implementations/`。
4. **真机验收清单**（交崔总）：见 §10.3。

---

## 9. 开工前需确认（4 条，含建议）

| # | 问题 | 建议（默认按此实施，除非崔总另有指示） |
| --- | --- | --- |
| 1 | 过渡帧 L2（未编辑照片用 SOOC/RAW 基准图）保留吗？要不要角标？ | **保留、不加角标**（角标属"不带变量的说明文字"，违反 §2.20；未编辑照片本就没有"旧版本"可比） |
| 2 | 对比参照降级时的标签形态 | **`SOOC → RAW`**（既表明用户的选择，也表明实际来源）；菜单里 SOOC 项照旧禁用 |
| 3 | 预览快照 GC 策略 | **每资产保留最新 2 份 `latest-*`**（按 mtime）；不设全局体积上限（后续按需再加） |
| 4 | 存量白平衡修复（§6.8） | 做**一次性修复**（只动 `as_shot_k IS NULL` 的那类），执行前给崔总看修复前后同画面两张图 |

---

## 10. 测试与验收分工

### 10.1 Agent 侧（每波都必须做到）

- 单元测试覆盖边界：空/单元素、上下限、非法值、Unicode 与中文（涉及名称/文案）、
  超长路径、大小写规范化、并发竞争（队列去重/取消/优先级）、溢出截断（尺寸/clamp）。
- 质量门：`pnpm typecheck && pnpm test && pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n && pnpm build`；
  `cargo test --workspace`、`cargo clippy -p raybend -p raybend-desktop`。
- 运行时冒烟：`pnpm smoke:ui`（起 `pnpm dev` 的另一终端）；离屏 GPU 冒烟保持既有项。
- 每波写一份 `implementations/YYYY-MM-DD_<简述>.md`（首行精确到秒；含改动范围、关键决策、
  验证命令与结果、遗留）。

### 10.2 不得由 Agent 判定

GUI 交互与视觉正确性、多显示器与 DPI、真实照片库观感、性能体感、色彩正确性 —— 归崔总真机。
**「进程起来了」≠「功能对了」**：报告必须区分「已验证（冒烟）」与「未经人类验证」。

### 10.3 真机验收清单（交崔总）

- [ ] 裁切/旋转过的照片来回切：首帧画幅正确、无拉伸、无跳变
- [ ] 编辑过的照片：切回时首帧要么是同版本快照、要么是"载入中"（不出现别的版本）
- [ ] 未编辑照片：切图仍然快
- [ ] 切图体感：连续切 10 张（含 1:1、含大降噪设置），无明显"某张特别慢"
- [ ] 白平衡：调过白平衡的照片来回切，首帧颜色正确（无几秒跳变）
- [ ] 预设跨照片：套用到 as-shot 不同的照片，冷暖倾向一致
- [ ] 对比参照：SOOC/RAW/定稿切换，标签与实际一致；无位图照片显示实际来源
- [ ] 定稿（issue）：生成后立刻在对比/全屏里看到（不再长时间等待）
- [ ] 定稿自愈：手工删掉某定稿的缓存图后进 editor，图自己回来（且不闪）
- [ ] 缓存目录：反复编辑同一张 20 次后，`cache/full/<asset>/` 不会堆满

---

## 11. 与其它在途工作的耦合

- **「issue 预览慢」（另一 agent，2026-10-10 已完成）**：那批已交付 `preview-queue`（单飞 + 最新者优先）、
  `reuse_latest_preview`（同画面不渲两遍）、`IssueThumb` 旧图留守（不闪）—— **W5 在它之上补自愈**，
  不重做；W2 的队列与就绪登记表落地后，两边共用同一份（谁先动手谁定接口，另一方接入）。
  两个会话在 `implementations/` 互相留指针，避免各造一份队列。
- **色彩管理**：`decode_working_source` 的口径不动；W4-6.2 的 as-shot 覆盖要**同时**覆盖 legacy 与 V2。
- **M4 导出**：导出预览进 P1/P3 队列，但导出像素口径与产物不变。
- **XMP**：本期明确无 sidecar 字段变化（§1.2）；若 W4 实现中出现需要写 sidecar 的冲动，
  停下来按 §2.19 重新评估，不在本计划内顺手改。

---

## 12. 改动文件索引（预估，实施时按当时代码复核）

| 层 | 文件 | 波次 |
| --- | --- | --- |
| Rust 核心 | `crates/raybend/src/raw/worker.rs`（优先级）、`display/full_cache.rs`（命名/GC）、`develop/params.rs`（基线覆盖入口） | W2 W4 |
| Rust 外壳 | `src-tauri/src/editor.rs`、`editor_working.rs`、`develop.rs`、`thumbs.rs`、`issues.rs`、新 `work.rs` | W1 W2 W3 W4 |
| 前端 | `src/lib/white-balance.ts`（新）、`src/lib/presets.ts`、`src/features/editor/store.ts`、`src/workspaces/editor/EditorWorkspace.tsx`、`src/api/types.ts`、`src/features/browse/display-variants.ts` | W3 W4 W5 |
| 文案 | `src/i18n/zh-CN.ts` / `en-US.ts`（迁移提示、跳过错项提示；两语同步） | W4 |
| 文档 | `specs/editor-presets.md`（§3 v3）、`design/editor.md`、`memory/ARCHITECTURE.md`、`memory/FUNCTION-IMAGING.md`、`AGENTS.md` §6.5、`specs/xmp-sidecar.md` | W1 W3 W6 |
| 记录 | `implementations/` 每波一份 | 全部 |

---

## 13. 整体完成定义（DoD）

1. §1.1 的 G1–G6 全部有证据（单测 + 冒烟 + 真机清单已交崔总）。
2. §10.3 的真机验收清单在 Windows 真机上逐条勾过（或明确记录哪几条未验与原因）。
3. 每波的 `implementations/` 记录齐备；`specs/editor-presets.md` / `memory/*` 已同步到实施后的口径。
4. 没有引入第二份实现：过渡帧、参照源解析、白平衡换算、预览缓存命名、定稿图自愈的判定与生成，各自只有一处。

---

## 14. 实施状态（2026-10-10 收尾，Agent 侧）

> 勾选项保持为**验收清单**；真机项（GUI / 观感 / 体感 / 色彩）一律未勾，归崔总按 §10.3 执行。
> 每波的实施记录在 `implementations/2026-10-10_editor-session-w*.md`。

| 波次 | 状态 | 证据 / 说明 |
| --- | --- | --- |
| W1 过渡帧契约 | ✅ 代码 + 单测 | `implementations/2026-10-10_editor-session-w1-transition-contract.md`；投递判据比规格收紧一格（`decode != ready` **且** `painted_path != path`），理由在记录里 |
| W2 管道与缓存 | ✅ 代码 + 单测（W2.7 的 full LRU 未做） | `…-w2-pipeline-and-queue.md`；`thumb_get` / `view_image` / 导出预览未进队列（理由在案） |
| W3 会话状态收敛 | ✅ 代码 + 单测 | `…-w3-reference-sources.md` |
| W4 白平衡 | ✅ 代码 + 单测；§6.8 存量修复**未执行**（待裁决） | `…-w4-white-balance-v3.md`；修复工具 `fix-as-shot-k`（默认预演） |
| W5 定稿图自愈 | ✅ 代码 + 纯函数单测 | `…-w5-issue-snapshots-self-heal.md` |
| W6 收尾 | ⚠️ 部分：骨架件已合一、**执行体未合并**（理由在案）；文档/证据/清单齐 | `…-w6-closeout.md` §一 |

**未验证清单（真机）**：§10.3 全部条目 + `pnpm smoke:ui`（需另一终端 `pnpm dev`）。
**未决**：§6.8 存量白平衡修复（工具已就绪，等崔总看修复前后留证再决定）。
**未做**：W2.7 full 源小 LRU；W6.1 执行体合并。

### 3.7 过渡帧的有意软化（2026-10-10 崔总追加）

过渡帧通常是 1920 预览；高分屏 / 大窗口下会被放大显示，而渲染侧放大采样是最近邻
（1:1 看像素的要求），于是 1920 会变成马赛克。补两件事：

1. 过渡帧用**线性放大**采样（`GpuContext::set_soft_preview`，真帧复位）；
2. 按当前视口的**放大倍率**做有意软化：`amount = clamp((scale − 1) × 0.75, 0, 1)`，
   `scale = Viewport::zoom_for_fit()`（与视口同一份数学）；核 = 3×3 可分离
   `[0.25·a, 1−0.5·a, 0.25·a]`，边缘复制。
   实现落点：`display::DisplayPixels::soften` / `soften_rgb8`（纯函数 + 单测）、
   `editor::transition_soften`（纯函数 + 单测）、`DevelopJob.transition_soften`
   （随换照片任务走，legacy / V2 共用同一个 `transition_outcome`）。

**边界**：只软过渡帧；真帧、对比参照、覆盖层都不动。倍率 ≤ 1（缩小 / 1:1）不软。
