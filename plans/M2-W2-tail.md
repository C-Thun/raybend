# M2-W2 收尾：阶段 3–6 + 欠账清零

> 计划文件：`plans/M2-W2-tail.md`。上游是 `plans/M2-W2.md`（49 步的 tracker，1–34 已完成）。
> 本计划覆盖**剩余的 35–49 步**，外加**你本轮的决定（对比规则改动）**与**攒着的欠账**。
>
> 编写时间：2026-09-19

## Context（为什么是现在）

* W2 的**阶段 1（看图）与阶段 2（胶片带 / 对比）已完成并验证**：步骤 1–34 全绿
  （`cargo test` 791 / `pnpm test` 571 / `check:browse` 通过）。
* 剩下的 35–49 步里，**后端几乎全部就绪**，本波主要是**界面装配**：
  * 标记动作：`store.mark/undo/redo/removeSelected/setFlag/clearFlags` 已在，`MarkResult` 已带
    `undoLabel/redoLabel/canUndo/canRedo`（步骤 41 的「可读文案」不用自己拼）；
  * 筛选/排序：`BrowseFilter`（含 `combinator`、`tags`、各 EXIF 区间）与 `BrowseSort` 已在，
    store 已有 `filter/sort/setFilter/patchFilter/setSort`；
  * 删除：`browse_delete`（走回收站）已在，`DeleteResult` 带 `blockedLocked/alreadyGone/failed`。
* **唯一的后端缺口是标签**：`crates/raybend/src/store/tags.rs` 功能齐全，但 `src-tauri` 里
  **没有对应的 IPC 命令**；`MarkingItem` 也还没有「这张图有哪些标签」。
* 顺带把**攒着的欠账**一起清掉：计划里的过时标记、文档收敛（`BROWSE.md` §10 / `DESIGN.md`）、
  老库回填、画布复核、以及需要你签字的几条。

## 你本轮的答复（已写入本计划）

| # | 问题 | 你的答复 | 对计划的影响 |
| --- | --- | --- | --- |
| 1 | 对比超过 4 张怎么办 | **只对比最近选中的 4 张**，不支持超过 4 张 | 0.1–0.4（改掉「取显示顺序前 4 张」） |
| 2 | 窗口从左到右怎么排 / 谁是基准 | **按选择先后排，最早选中的当基准** | 0.1 返回的窗口**按选择先后**（不是显示顺序）；胶片带与画幅同序 |
| 3 | `Shift+Delete` | **保持「删除永远确认」**（`AGENTS.md` §11.3 定案不变） | 5.1 只做 `Delete` + 确认；`BROWSE.md` §10.4 按此收敛 |
| 4 | 3–4 张画幅怎么摆 | **2 张一排 / 3 张一排三个 / 4 张 2×2** | 新增 0.5–0.6（布局纯函数 + 冒烟）；画布 6.2 时补 4 张帧 |
| 5 | 老库回填时机 | **现在就做，先 `--dry-run`** | D2 提到本波前段（数据先干净，阶段 3 的排序/筛选才能用真实数据验） |

---

## 一、立即修：对比 = 最近选中的 4 张

* 现在的 `compareIds()` 取「显示顺序的前 4 张」。改成**按选择先后的滚动窗口**：
  选中集合里**最近**的 4 张（最后点的 4 张），更早的自动被挤出去；
  **窗口内部按选择先后排**（最早选中的在最左，它就是画幅基准）——例：Ctrl 依次点 A B C D E F，
  窗口 = `C D E F`（基准 C，A/B 被挤掉）。
* 选择先后从哪来：`SelectionState.ids` 是 `Set`，**插入顺序天然就是选择先后**
  （Ctrl 点一下 append 一个；区间翻转按范围顺序 append）。**不新增字段、不动选择内核** ——
  只需要在 `compare.ts` 里把这层语义写清楚 + 补测试。
* 锚点（最后点中的那张）**必须在窗口里**：区间选择时锚点在中间，只按 Set 尾部取可能把它挤掉
  （规则：锚点缺席就用它替换窗口里**最早**的那张，并把自己放在窗口末尾）。
* 超出 4 张时的提示改成说明句（**只在真的挤掉图时出现**，不静默截断）：
  「已选 N 张，只对比最近选中的 4 张」。
* **注意**：workspace 里现在把 id 集合按**显示顺序**过滤回照片，那会把窗口顺序扔掉 ——
  要改成**按窗口顺序映射**（与胶片带 `onlyIds` 同一套做法），画幅与胶片带才同序。

**已核实的两个前提与两个边界**（已经读过 `lib/selection.ts`，不再靠猜）：

* ✅ `toggle` / `range` / `extendSelection` 都是 `new Set(旧)` 再 `add` —— **新增的 append 到末尾**，
  所以「Set 插入顺序 = 选择先后」成立；`pruneSelection` 也**保序**（换目录后不丢先后）。
* ⚠️ `selectAll`（`Ctrl+A`）与「日组全选」是**批量**，没有「先后」可言：
  它的插入顺序 = 列表顺序（全选）或组内顺序（日组）。按规则就是「窗口 = 这批的最后 4 张」——
  行为确定、可解释，但**不是**用户一张张点出来的意图。实现时在注释里写明白。
* ⚠️ `selectAll` 的锚点 = 组内第一张（既有行为），会被锚点规则拉进窗口；
  `extendSelection` 同样。这两条各补一个单测。

## 二、0.5–0.6 对比布局（你的答复 4）

* `compareLayout(count)` 纯函数 → `{ columns, rows }`：2 → 2×1，3 → 3×1，4 → 2×2；
  断言：1 张不该走对比布局（由调用方保证 ≥2）、5 张被截到 4、非法值兜底。
* `CompareView` 改用 CSS grid（`repeat(N, minmax(0,1fr))`）：每种布局里**每格尺寸一致**，
  于是「所有画幅扣成同一个比例」仍然成立（格子大小不影响比例与百分比同步）。
* 冒烟：4 张时断言列数/行数（`data-compare-cols` / `data-compare-rows`）。

## 二、阶段 3：toolsbar 全量装配与标记动作

| 步 | 内容 | 关键点 |
| --- | --- | --- |
| 3.1 | 标记边界 | 批量/混合态提示；锁的三态（不可删 / 不可编辑）；`skippedLocked` 要在界面上说出来；旗标「移除所有」走 `easy destroy` 确认（它清空**所有**旗标）；标星再点扣 0 |
| 3.2 | **标签弹窗** | 新 IPC（`tag_list` 搜索 / `tag_ensure` 创建 / 读取某张的标签）+ 前端 `TagDialog`：单张可增可删（瀑布流 + 每个标签一个叉）、批量**只能加**、输入即搜 **0.6s 防抖**、上下键选择、**直接回车 = 创建新标签** |
| 3.3 | 筛选开关全语义 | 打开后标记/标签/锁变成**筛选条件**；只筛当前 tiles/胶片带；**不影响正在看的那张** |
| 3.4 | 条件 chips + 与/或 | `3★ 以上 ×` / `红色 ×` / `已喜欢 ×` / `共 N 张`；与/或切换（引擎两种都支持，**默认「或」**） |
| 3.5 | 排序 UI | `takenAt` / `importedAt` / `fileName` / `rating` / `camera` + 升/降（`store.setSort`） |
| 3.6 | **不做**高级筛选面板 | 登记 `FUTURE.md`，UI 上不露入口 |

## 三、阶段 4：撤销 / 重做、toast、模态

| 步 | 内容 | 关键点 |
| --- | --- | --- |
| 4.1 | 撤销/重做 UI | 按钮 + 文案直接用后端 `undoLabel/redoLabel`（「撤销：设为 3 星」）；无牌可打时禁用 |
| 4.2 | **toast**（新组件） | 右上角、`--z-toast`、带边框（成功 `$brand` / 失败 `$danger`）、约 5 秒、**不阻塞**、带「撤销」按钮；`motion.css` 里已留了动画位置 |
| 4.3 | 中央模态统一 | 标签弹窗、创建子目录、确认类（`easy destroy` 默认态）、删除错误清单 —— 都用 `components/ui/Dialog.tsx` |

## 四、阶段 5：删除与键盘

| 步 | 内容 | 关键点 |
| --- | --- | --- |
| 5.1 | `Delete` 接线 | `browse_delete`（回收站）+ 确认；失败清单进模态；锁住的要报出来。**不给 `easy destroy`**（你 2026-09-19 的批注）：删除支持**多选批量**，而 `easy destroy` 的 Shift 快通道是给「不让界面长批量控件、又要连续快速删单张」那种场景准备的 —— 能批量的操作不需要它。实现上就是**直接用 `ConfirmDialog`**，不用 `EasyDestroyButton`（后者天生带 Shift 跳过） |
| 5.2 | 全键盘评片流 | `←`/`→` 切图、`0`–`5` 打星、`P`/`X` 旗标、`U` 取消标记、`Enter` 进看图、`Esc` 退出/取消选择 |

## 五、阶段 6：收尾

| 步 | 内容 |
| --- | --- |
| 6.1 | 左右列宽度可拖拽（复用导入那边的 `SplitHandleDots`；宽度存设置） |
| 6.2 | 画布复核：对比 4 张的帧、标签弹窗、toast、chips；导出截图给你过目（**先改 `.pen` 再改代码**的纪律照旧） |
| 6.3 | 质量门全跑 + `implementations/` 记录 |
| 6.4 | 文档联动：`BROWSE.md` §10 收敛、`DESIGN.md` 补直方图/对比同步/弹性动画例外、`FUTURE.md` 登记 |
| 6.5 | `ASSISTANCE.md` 更新（本波新人验项：胶片带 / 对比 / 直方图 / 标签弹窗 / toast） |
| 6.6 | **收尾提交**（你批了之后我 commit；评审期间按纪律不提交） |

**`BROWSE.md` §10 八条的收敛结论（直接照这个改）**：

| §10 | 结论 |
| --- | --- |
| 1 与/或 | 引擎两种都支持；**UI 默认「或」**，另给「任一/全部」切换（3.4） |
| 2 混合态视觉 | 无值 = 全暗 `fg-3`；取值一致 = 显示该值；**混合 = 半亮 `fg-2` + 一个短横**（已实现） |
| 3 `Tab` 三态 | **并列三态** ①默认→②关左右→③关胶片带→①（已实现） |
| 4 删除 | **保持「删除永远确认」**（你 2026-09-19 定；`Shift` 不给快通道） |
| 5 弹性动画 | 按你口述确认为**有意例外**，写进 `DESIGN.md`（约 200–260ms + 弹性曲线，允许动高度） |
| 6 旗标不持久化 | 按你口述确认：**关软件即清空**（临时工作集，不是元数据） |
| 7 直方图 | 本期做**单张合成 24 柱**；通道叠加/分离切换进 `FUTURE.md` |
| 8 对比最多几张 | **只对比最近选中的 4 张**（你 2026-09-19 定），不支持更多 |

## 六、欠账清零

| # | 欠账 | 怎么清 |
| --- | --- | --- |
| D1 | `plans/M2-W2.md` 里「⏸ 进行中（2026-09-18 下班暂停）」的残留字样（1.0 那条已完成） | 直接删掉那段说明，并在 tracker 里把 1.0 标干净 |
| D2 | **老库回填**（`taken_at` 等空值） | 按你的答复：先 `--dry-run` 看数量，或直接跑（幂等、只填空值） |
| D3 | `ASSISTANCE.md` §二 的 4 条 | 三条是环境/上游问题（CJK 偏移、RAW 缩略 134ms、cargo 链接偶发）**保持现状**并更新记述；「陈旧快照误报」本波又出现两次，按既有判据（以 `tsc`/编译器为准）补一句到记述里 |
| D4 | 画布批次过目（§三 3b） | 本波新增帧一起导图，一次性给你看 |
| D5 | M1 验收 / M2-W1 目视（§三 1、2） | 提醒你抽时间跑一次重建后的 exe；我这边先把构建脚本备好（`pnpm spike:win`） |
| D6 | 工作区一堆未提交改动 | 收尾时按你的批准提交（分几个语义清晰的 commit） |

## Files to modify（关键路径）

```text
改   src/features/browse/compare.ts / compare.test.ts        （0.1–0.2 最近选中的 4 张）
改   src/features/browse/CompareView.tsx                     （0.3 说明文案）
改   src/features/browse/BrowseToolbar.tsx                   （3.1 / 3.3–3.5 / 4.1）
新   src/features/browse/TagDialog.tsx                       （3.2）
新   src/features/browse/FilterChips.tsx                     （3.4）
新   src/components/ui/Toast.tsx (+ 一个很小的 toast store)   （4.2）
改   src/features/browse/store.ts                            （3.2 标签动作、5.1 删除接线）
改   src/api/browse.ts + src/api/types.ts                    （3.2 标签 IPC、MarkingItem.tagIds）
新   src-tauri/src/tags.rs + lib.rs 注册                      （3.2 三个命令）
改   src/workspaces/browse/BrowseWorkspace.tsx               （键盘 5.2、列宽 6.1、模态挂载）
改   scripts/check-browse-boot.mjs                           （每步的断言）
改   BROWSE.md / DESIGN.md / FUTURE.md / ASSISTANCE.md / plans/M2-W2.md （6.4–6.5、D1）
```

## Reuse（已有的、别重写）

| 要用的东西 | 在哪 |
| --- | --- |
| 选择语义（点击/Shift/Ctrl/锚点） | `src/lib/selection.ts`（`applySelection` / `clickMode`） |
| 三态（无值/一致/混合） | `src/lib/marking-state.ts`（`triState` / `MixedMark` 已在 toolbar 里用） |
| 确认弹窗与「按住 Shift 跳过」 | `src/components/ui/Dialog.tsx` + `EasyDestroy.tsx` + `lib/easy-destroy.ts` |
| 分栏拖拽 | `src/components/ui/SplitHandle.tsx`（`SplitHandleDots`，导入工作区在用） |
| 筛选/排序引擎 | store 的 `setFilter/patchFilter/setSort`（`BrowseFilter` / `BrowseSort` 已支持全部条件） |
| 撤销/重做文案 | `MarkResult.undoLabel/redoLabel`（后端已给） |
| 标签存储 | `crates/raybend/src/store/tags.rs`（`list_tags` / `ensure_tag` / `attach_tags` / `detach_tags` / `tags_of_asset`） |
| 回收站删除 | `browse_delete` + `DeleteResult.blockedLocked/failed` |

## Steps

- [x] 0.1  compare.ts：对比集合 = 选择集合里最近 4 张（锚点必含；Set 插入顺序即选择先后；窗口按选择先后排）
- [x] 0.2  compare.test.ts：6 选 4 的滚动窗口、基准 = 窗口最早、区间批量选、反选、锚点必含、<2 张不对比
- [x] 0.3  CompareView / i18n：把「最多 4 张」的临时提示换成「已选 N 张，只对比最近选中的 4 张」（仅在挤掉图时出现）
- [x] 0.4  check:browse：加「选 5 张 → 只 4 幅 + 最早那张被挤掉 + 基准 = 窗口最早」
- [x] 0.5  compareLayout(count) 纯函数 + 测试（2→2×1、3→3×1、4→2×2）
- [x] 0.6  CompareView 改 CSS grid 摆放 + 冒烟断言列数/行数
- [ ] 3.1  标记边界（锁三态 / skippedLocked 提示 / 旗标清空确认 / 标星扣 0）
- [ ] 3.2  标签：Rust 三个命令 + MarkingItem.tagIds + TagDialog（0.6s 防抖、回车建标签、单张可删批量只加）
- [ ] 3.3  筛选开关全语义（patchFilter；不影响正在看的那张）
- [ ] 3.4  chips + 与/或切换
- [ ] 3.5  排序 UI
- [ ] 3.6  高级筛选面板 → FUTURE.md（UI 不露入口）
- [ ] 4.1  撤销/重做按钮 + 文案
- [ ] 4.2  Toast 组件 + 接线（带撤销按钮）
- [ ] 4.3  模态统一（标签/子目录/确认/错误清单）
- [ ] 5.1  Delete 接线 + 确认 + 失败清单（**Shift 不给快通道**：永远确认）
- [ ] 5.2  全键盘评片流（←/→、0–5、P/X、U、Enter、Esc）
- [ ] 6.1  左右列拖拽 + 宽度持久化
- [ ] 6.2  画布复核 + 截图导出给你过目（补 4 张 2×2 的对比帧）
- [ ] 6.3  质量门 + implementations 记录
- [ ] 6.4  文档联动（BROWSE §10 / DESIGN / FUTURE）
- [ ] 6.5  ASSISTANCE 更新
- [ ] 6.6  收尾提交（你批后）
- [ ] D1   plans/M2-W2.md 清理 ⏸ 残留
- [ ] D2a  老库回填 --dry-run → 报出「会补多少张」给你看
- [ ] D2b  你点头后真跑（幂等、只填空值）
- [ ] D3   ASSISTANCE §二 记述更新
- [ ] D4   画布批次截图（与 6.2 合并）
- [ ] D5   M1/M2-W1 人验提醒 + Windows 构建脚本备好

## Verification

| 层 | 命令 | 期望 |
| --- | --- | --- |
| 前端 | `pnpm typecheck` / `test` / `lint:arch` / `lint:colors` / `lint:i18n` / `build` | 全绿 |
| 前端冒烟 | `pnpm smoke:ui` | `problems: []` |
| 浏览冒烟 | `pnpm check:browse` | 现有断言全过 + 每步新增断言（对比窗口 4 张/挤掉/基准、布局列数、标签弹窗、筛选 chips、排序、toast、删除确认、键盘） |
| Rust | `cargo test -p raybend` / `cargo clippy --workspace --all-targets` | 全绿、零警告 |
| 标签后端 | 新增单测（名字折叠/去重/批量只加/单张增删/空名与超长名） | 与 `tags.rs` 既有测试风格一致 |
| 回填（D2） | `--dry-run` 先看数字 → 真跑 → 再跑一次验幂等 | 幂等、不动已有值 |
| 真机手感 | Windows 重建（`pnpm spike:win`）→ 人跑一遍 | **由你判定**（我不声称已验证） |

## 待你定夺（本计划开工前）

（本轮已全部答复，见上方表格。开工后若再撞到新的岔路，我停下来问。）
