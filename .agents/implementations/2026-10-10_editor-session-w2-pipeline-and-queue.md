完成时间：2026-10-10 18:40:51 +0800

# 编辑会话管道 W2：预览快照按栈指纹命名、后台重活队列、worker 优先级、V2 判据、邻图预热

## 依据

`specs/editor-session-pipeline.md` §4（W2），服务现象 2（切图有快有慢）与现象 4 的同源部分。

## 一、预览快照按栈指纹命名（§4.2）

- `FullCache` 文件命名从 `latest-src<sig>-<base>-v<pipeline>.avif` 变为
  **`latest-<profile_hash 前 12 位>-src<sig>-<base>-v<pipeline>.avif`**（与 `issue-<id>-<hash>` 同一套哈希）。
  拼接只有一处：`develop::latest_variant`（+ `latest_preview_key` 供单飞/等待用）。
- 读取方同步：`thumbs::render_latest_cached`、`thumbs::thumb_get`（胶片带/网格）、
  `issues::reuse_latest_preview`、`editor::transition_plan`（过渡帧 L1）全部走同一个 helper。
- **落库不再整目录作废**：`develop_commit` 删掉 `FullCache::invalidate` 调用；
  旧文件因名字不同自然失配。`color_batch` / `browse` 的 invalidate 保留（那是另一类失效）。
- **GC**：新增 `FullCache::gc_latest(asset_id, keep)`（按 mtime，每资产保留最新 2 份
  `latest-*`），在 `render_profile_cached` **写新快照之后**顺手做；单测覆盖
  （三份保留两份、定稿快照不动、幂等）。

## 二、后台重活队列（§4.3）

新 `src-tauri/src/work.rs`（挂 Tauri state，默认 2 条工作线程）：

- **优先级** P0 交互 / P1 前台等待 / P2 预热 / P3 批量 —— 取任务时从最高优先级的非空队列拿；
- **key 去重**：同一个 key 排队或执行中就不再入队（`spawn` 返回 `None`）；
- **同 key 单飞**：`run` 的第二个调用者**等**第一个的结果（条件变量，不轮询）；
- **取消**：`CancelToken` + `cancel(key)`；排队中的直接丢弃并把结论交给等待者；
  已开跑的靠任务在检查点看令牌（不抢占）；
- `on_worker_thread()`：在工作线程上不许等同一个队列的任务（避免把工作线程全占满的自锁）。

接入点：

| 位置 | 方式 |
| --- | --- |
| `develop_preview_refresh` | `run(P2, latest_preview_key)` —— 同一张画面只渲一遍，等待者拿到同一结果 |
| `issues::reuse_latest_preview` | `wait(key, 30s)` 替代 **sleep 轮询**；没有任务就自己整渲（正确优先） |
| `issues::issue_create` 的后台快照 | `spawn(P3, issue-snapshots:…)`（可被前台插队） |
| `thumbs::issue_sources_prepare` | `spawn(P3, source-images:…)` |
| `editor::editor_prefetch_neighbors` | `spawn(P2, latest_preview_key)`，新一次调用撤掉上一批未开跑的 |

**未接入（记录在案）**：`thumb_get` / `view_image` 这类**按需交互**路径保持原样 ——
它们由用户动作直接触发、前端已限流（同时 4 个），排进队列反而会被批量任务拖住；
导出预览路径未动（导出本身在导出工作区串行，收益低）。这与 §4.3 的清单有出入，
实施记录如实记下。

## 三、RAW worker 优先级（§4.4）

- `raw/worker.rs` 新增 `DecodePriority`（High/Normal/Low）+ 优先级闸门 `acquire(priority)`
  （RAII 许可；高优先等待者先拿到许可）。**不可重入**：只包最外层重解码。
- 标注：编辑器解码（legacy `decode_linear_source` + V2 `decode_working_source`）= High；
  缩略图/预览（`thumbnail::render` 两处 + `decode_working_source` 的缩略图调用）= Normal；
  导出（`export.rs` 两处）= Low。
- 单测：批量 + 缩略图排满时，编辑器 High 先走（独立闸门实例，不碰全局）。

## 四、V2 重建判据对齐 legacy（§4.5）

- `should_reload_source` 抽成两路共用的 `source_needs_reload(cached_path, cached_signature,
  wanted_path, observed_signature, explicit_photo, color_changed)`；
  V2 的重建条件去掉 `job.photo.is_some()`，改为「路径 / 色彩状态 / 显式照片任务的签名」。
  ⇒ 几何 / 对比参照操作**不再整张重解码**（V2 以前每次重解，现象 2 的一半）。
- 单测覆盖：显式照片任务签名未变 → 复用；色彩状态变 → 重建。

## 五、参照源跨缓存保留（§5.1，与 W3.1 同一次改动）

- 新增 `develop::ReferenceSources { sooc, bitmap_available }`（命令层按 asset 解析一次）；
  `RenderCommand::SetPhoto` 携带它，显影线程存成 `wanted_sources`（**跨缓存重建保留**）。
- `CachedSource.sooc` / `Cached.sooc` / `TransitionPlan.sooc` 三个字段删除；
  两路共用 `reference_sooc_path(sources, source_path)`：`bitmap_available == false` →
  SOOC 参照**不可用**（不允许 RAW 冒充）；不在库里时位图文件自己就是 SOOC 侧。
- 单测：`reference_sooc_path_never_masquerades_a_raw_as_sooc`。

## 六、邻图预览预热（§4.6 的「先做」档）

- 新命令 `editor_prefetch_neighbors(paths)`：对**编辑过且缺 latest 快照**的邻居
  `spawn(P2, latest_preview_key)`；新一次调用撤掉上一批未开跑的。
- 前端在「当前真帧就绪、停留 300ms」的既有空闲窗口里算前后各一张
  （`neighborPaths`：显示序下标缓存 + 越界跳过），随后调用；与 `prepareIssueSources` 同一条节流。
- **未做（登记遗留）**：full 源小 LRU（§4.6「后做」档）。

## 验证

- `cargo test --workspace`：全绿（含新增 `work::tests::*` 5 条、
  `decode_priority_lets_the_editor_jump_a_full_batch_queue`、
  `gc_keeps_only_the_newest_latest_snapshots`、
  `changed_source_reloads_but_unchanged_and_parameter_jobs_reuse`（扩了色彩/几何两例）、
  `reference_sooc_path_never_masquerades_a_raw_as_sooc`）。
- `pnpm typecheck && pnpm test`（1259）`&& pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n && pnpm build` 全绿。
- **未经人类验证**：压力场景（跑批量快照时切图）、邻图回访体感、缓存目录不堆满 —— 归崔总真机。
  性能证据（W1 前 / W2 后的切图时序）在 W6 收尾时补。

## 遗留

- `thumb_get` / `view_image` / 导出预览未进队列（理由见上）。
- full 源小 LRU 未做（§4.6 后做档）。
- 队列的「取消执行中任务」是协作式的：长渲染仍会跑完（结果可能写缓存），
  只保证**排队中的**一定丢。
