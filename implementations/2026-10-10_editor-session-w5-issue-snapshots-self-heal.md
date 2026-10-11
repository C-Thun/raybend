完成时间：2026-10-10 18:48:36 +0800

# 编辑会话管道 W5：定稿图自愈（缺什么补什么）

## 依据

`specs/editor-session-pipeline.md` §7。崔总 2026-10-10 口述：定稿生成失败界面无警示 ——
「至少进某个图的 editor 时扫描所有 issue 看有没有缺图、缺图重生，以及在 browse 中选中
这个图的某个 issue 时没有 thumb 要重生成；错误不一定要报出来，不影响主线给个自动修复的机会就行」。

## 一、Rust：扫描 + 补齐（`src-tauri/src/issues.rs`）

- 新命令 **`issue_snapshots_ensure(repository_id, asset_id, sizes?)`**（缺省三档）：
  - 列该资产全部定稿（`issues::list`）；
  - **判缺**（纯函数 `missing_snapshot_sizes`）：`screen` 查 `FullCache::read`，
    `grid`/`strip` 查 `thumbs` 缓存库（键与签名沿用 `thumb_bytes` 同一份拼法）；
  - **顺序固定 Screen → Grid → Strip**（`SNAPSHOT_ORDER`）—— 小图能从 1920 缩出来，
    先渲大图 = 每张照片只付一次整管线；
  - 缺 `screen` → 走既有 `ensure_snapshots(app, repo, &issue, false)`（不等待 latest）；
    缺小图 → 走既有 `thumb_bytes`；
  - 返回**这次真补了**的条目 `[{ issueId, sizes }]`；单条失败只记日志、跳过，不影响其它定稿；
  - **静默自愈**：界面不新增任何「生成失败」常驻提示（§7.2-3）。
- **复用而非重写**（§2.12）：判定后的生成、写缓存、命名全部走现有函数，不新造缓存键。
- **去重**：命令层走后台队列 `run(P3, key = issue-snapshots:<repo>:<asset>)` ——
  同一张照片并发进入只跑一次（同 key 单飞），且批量优先级可被前台插队。
  结果经 `Arc<Mutex<Vec<..>>>` 回传；等待者拿到的是空表（它没跑任务），
  这不影响界面：真正补图的那位调用方会刷新缩略图。

## 二、前端接入

| 位置 | 行为 |
| --- | --- |
| `src/api/issues.ts` | `ensureIssueSnapshots(repositoryId, assetId, sizes?)` 封装（非 Tauri 返回 `[]`） |
| `EditorWorkspace.tsx` | 定稿库就绪（既有 60ms 防抖）之后调用；补过的条目逐个 `issueThumbs.refresh(issueThumbKey("issue:N"))`（`IssueThumb` 旧图留守 → 不闪）；失败只记控制台 |
| `display-variants.ts` | 新增 `onVariantSelected?` 依赖，`select()` 里对 `issue:N` 变体通知一次（不等结果） |
| `BrowseWorkspace.tsx` | 接线：`choice.startsWith("issue:")` → `ensureIssueSnapshots(repository, asset)` |

## 三、测试与文档

- Rust：`missing_snapshot_sizes_reports_only_the_missing_ones_in_order`
  —— 三档全有 → 空；缺 screen → 只报 screen；缺小图 → 只报小图；顺序固定；只求一档时只报这一档。
  「幂等」与「先补 screen」是结构性保证：全齐时 `missing` 为空、函数直接返回（零渲染）；
  顺序由 `SNAPSHOT_ORDER` 与纯函数测试共同锁住。
  （`ensure_issue_snapshots` 需要 `AppHandle` 与真实库，无法在单测里跑；未造集成测试。）
- `memory/FUNCTION-IMAGING.md`：新增「定稿图自愈」段（生成节点 + 调用点 + 静默口径）。
- 门禁：`cargo test --workspace` 全绿（1477 + 130 + …）；`pnpm typecheck && pnpm test`
  （1259）`&& pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n && pnpm build` 全绿。

## 四、遗留 / 未验证

- **未经人类验证**（§10.2）：手工删掉某定稿缓存文件后进 editor 图自己回来、browse 选中
  定稿变体后 thumb 到位、补齐后不闪 —— 归崔总真机。
- 与 W2 队列的「小图等同一张的 screen」依赖未做：面板挂载时若先请求 `strip`，
  自愈的 `screen` 可能排在它后面，小图会各自整渲一遍（今天本就有这层浪费，§7.6 允许）。
- `sizes` 参数目前前端不传（固定三档）；保留它是给将来「只补某一档」用。
