完成时间：2026-10-08 23:58:12 +0800

# editor 对比态补上拖图 + 定稿列表改名入口

## 一、对比态下图像不能拖（崔总报）

**症状**：editor 里开对比后图像拖不动，只能靠滚轮定向缩放看细节，很别扭。

**根因**：`features/editor/viewport.tsx` 在 `tool === "compare"` 时把 down/move/up 全部当成
「拖分线」发给 `comparePointer`；Rust 侧 `Gpu::compare_pointer` 只在**起手命中分线把手**之后
才认后续位移，否则整条手势被丢弃 —— 没有第二条路走平移。

**修法（只看起手，一处判）**：把对比手势做成一个纯状态机 `viewport.rs::CompareGesture`：

| 起手 | 这一手势 |
| --- | --- |
| 命中分线把手（洞口内、离线 ≤ 14 CSS px） | 拖分线（原行为，`Handle(fraction)`） |
| 其它任何位置 | **平移图像**（`Pan((dx, dy))`，物理像素） |

* 与普通视口拖动**同一手感**：3 CSS px 拖动阈值（`COMPARE_DRAG_SLOP_PX`，口径对齐前端
  `lib/editor-intent.ts::CLICK_SLOP_PX`）；越过阈值时把「从起手算起」的位移一次补上，之后逐样本走；
  **松手补尾样本**（`up` 自带最终位置，不补的话图像停在半路）；`cancel` 丢弃这一手势的剩余位移。
* 阈值内不移动 ⇒ 不调 `pan_by` ⇒ 不会因为「点一下」把 `FitMode` 悄悄切成 `Free`。
* CSS → 物理的 DPR 只乘一次，仍然只在 Rust 里乘；前端一行没改（它本来就在上报原始指针事实）。
* `compare_pointer` 返回值仍是「这一帧变了没有」，平移返回 true 让调用方重画。
* 顺手删掉 `Gpu::compare_dragging`：改为手势状态机之后它没有第二个使用者，留着就是第二份状态。

**没做的**：分线上没有换成 `col-resize` 光标 —— 前端不掌握分线位置（fraction 与洞口都在 Rust），
要做就得让 Rust 把 fraction 报回前端；现在整块视口是 `grab / grabbing`，与「拖得动」是自洽的。

**测试**：`viewport.rs` 新增 3 条 —— 起手在把手上只拖分线（含尾样本）、起手不在把手上平移
（阈值内不动 / 越阈值补位移 / 逐样本 / 松手尾样本 / 全程位移 = 手指位移 × DPR）、`cancel` 与
NaN 坐标不改状态。

## 二、定稿（issue）列表的改名入口（崔总要）

**交互**：已保存的定稿行悬停时，名称右侧依次出现**笔**（改名）与垃圾桶（删除）；
SOOC / RAW / 最近编辑是三种**基准**不是条目，不给改名。点笔打开改名窗 —— 形状与
「保存为新定稿」同构（输入框 + 取消 / 保存），只是标题换成「重命名定稿」、输入框**预填当前名字**，
回车即保存，80 字符上限。

**Rust**：

* `store/issues.rs::rename(conn, asset_id, issue_id, name)` 只改 `name` 一列；
  名称规则抽成 `validated_name()`（`create` 与 `rename` 共用，不再各写一份），
  撞名抽成 `ensure_name_free()` —— DB 本来就有 `UNIQUE(asset_id, name)`，先拦一步给「已有同名定稿「x」」
  这样的人话，而不是抛 SQLite 约束原文。`import_issue` 的校验也改走同一份规则。
* 改名**不动**配置 JSON / `profile_hash` / `source_base` / `created_at` / `ordinal` ⇒
  选中态（按 `id`）与画面都不受影响；改完 `queue_sync` 这张照片的 sidecar
  （`rb:profiles` 每条都带 name，规则见硬约束 §2.19 与 `specs/xmp-sidecar.md` §6.1，表已补 `issue_rename`）。
* 新命令 `issue_rename`（`src-tauri/src/issues.rs`，`lib.rs` 注册），返回整份 `IssueLibrary`
  —— 与 create / delete 同一条口径，前端拿它直接回填。

**前端**：`api/issues.ts::renameIssue`；`features/editor/panels.tsx` 的行内笔 + `onRenameIssue` 回调；
`workspaces/editor/EditorWorkspace.tsx` 的改名窗（busy / error / revision 作废、换照片关窗，
形状与写法沿用同文件里「保存为新定稿」与面板里「基础曲线档案改名」两处的既有模式）。
文案新增 `editor.issue.renameTitle`（中「重命名定稿」/ 英「Rename issue」），保存按钮复用 `common.save`。

**测试**：`store/issues.rs` 新增 2 条 —— 只改名字（首尾空白、改成自己的名字、其余字段原样、
选中态不变、撞名报人话且不改动、另一张照片的同名不算撞、踩空 id 返回 false）、
非法名与保留名（空 / 控制字符 / `sooC` / `RAW` / `Latest` / 81 字符拒绝，80 字符通过，失败不改原名）。

## 涉及文件

* Rust：`crates/raybend/src/render/viewport.rs`、`render/gpu.rs`、`store/issues.rs`；
  `src-tauri/src/issues.rs`、`src-tauri/src/lib.rs`
* 前端：`src/api/issues.ts`、`src/features/editor/panels.tsx`、`src/workspaces/editor/EditorWorkspace.tsx`、
  `src/i18n/zh-CN.ts`、`src/i18n/en-US.ts`
* 文档：`specs/xmp-sidecar.md`（§6.1 触发表）、`design/editor.md`（2026-10-08 小节）

## 验证

* `cargo test --workspace`：1581 条全绿（`raybend` 1451 + desktop 119 + …），0 失败
* `cargo clippy -p raybend -p raybend-desktop`：本轮只引入过一条 negated-comparison 提示，已改成
  `travelled <= SLOP`；其余提示都是仓里既有的（复杂类型 / 参数过多等）
* `cargo check -p raybend-desktop`：通过（新命令注册无误）
* `pnpm typecheck` / `pnpm test`（1225）/ `lint:colors` / `lint:arch` / `lint:i18n` / `pnpm build`：全绿
* 未验证（需崔总真机）：拖图手感（阈值、跟手、松手不跳）、改名窗在真机上的外观与错误提示文案；
  这两项都要**重新构建 Rust 侧**才会生效（前端改动走 HMR 即可）
