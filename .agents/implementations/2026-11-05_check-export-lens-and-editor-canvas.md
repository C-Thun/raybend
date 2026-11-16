# check:export 收口、lens 报错核查、编辑器画布同步

> 完成时间：**2026-11-05 05:36:20 +08:00**
> 三件事：① `check:export` 的红灯（崔总：「你看着处理一下，降低实现成本」）；
> ② pi-lens 面板上 12 error / 168 warning 的核查；③ `design/editor.pen` 画布同步。

## 1. `check:export`：是测试的事，但顺手挖出一个真 bug

结论：**拖动那段是测试脚本的两处过期期望**，产品行为本身正确；顺带在排查途中修了两个真问题。

| # | 现象 | 性质 | 处理 |
| --- | --- | --- | --- |
| 1 | 「浏览器 → 导出」点 chip 没反应（表现为切不回工作流） | **真 bug（不是测试）** | `TagFilterMenu` 的 `createMemo` 拿 `store.tags()`（未加载时是 `null`）直接 `tags.filter` → 在 Solid 计算里抛异常，**整棵响应式根停止刷新**（信号变、DOM 不变，`elementFromPoint` 仍命中 chip）。修：`filterTagMenu` 接受 `readonly Tag[] \| null`（`null` → 空菜单）+ 单测 |
| 2 | `ghost centred on the pointer` 断言失败 | **测试期望过期** | 幽灵画的是「最近一次指针位置」，而拖动要越过阈值才成立 —— 越过那一次之后没有新 move 时它停在**触发点**，测试却拿 `+12` 的位置去比。修：断言前再推一格（`+24`），既符合真实手势也真的验了「跟着指针走」 |
| 3 | `ghost is one card wide with four cards` 失败（实际 96，期望 132） | **测试期望过期** | `DragPreview` 在 2026-10-11 与 browse 合并成唯一一份，尺寸是 `size-24`（96px）；132 是旧版 export 内联实现的数字。修：期望改 96 |
| 4 | 末尾 `cdp.exceptions` 非空：`MaskDebugDialog` 读 `defaults.lumaCore` 抛错 | **真问题（开发期边界）** | `editor_mask_defaults` 在 fixture/旧后端没有实现 → 返回 `undefined`，被当成配置用。修：`api/editor.ts::getMaskDefaults` **在 I/O 边界解码**（四个 `[number, number]` 齐全才算数，否则退回零配置） |

结果：`pnpm check:export` **整跑通过**（`✓ populated export: bounds, quality visibility, issue selection/queue removal, modal, photo filters, stable library, failure retries`）。

## 2. pi-lens 的 12 error：全是**过期缓存**

面板里报的 12 条 blocking 全部集中在两个文件，且都在**本轮修好之前**的中间状态：

| 文件 | 报的错 | 现在的实际状态 |
| --- | --- | --- |
| `ExportWorkspace.tsx` | `Cannot find name 'FieldRow'` ×8、`export.currentPreset` 不可赋值 | 组件与 i18n key 都已补上；`pnpm typecheck` 通过 |
| `BrowsePanels.tsx` | L1240 `')' expected` / `Cannot find name 'div'` | 那次多余的 `</div>` 在几分钟内就删掉了 |

复核方式（不是看缓存，是**重新拉一次 LSP**）：
`lens_diagnostics source=lsp moderate scope=paths` 扫这 7 个文件 → **0 条 error**（3 个 clean、4 个静默即无问题）。
另外 `pnpm typecheck` 全仓通过，这是权威判据。

剩下的 ~204 warning 分三类，**都不是错误**，且绝大多数在本轮之前就有：

1. **风格/复杂度**（`high-complexity` / `high-fan-out` / `high-import-coupling` / `inline-styles` / `console-statement`）——
   落在 `App.tsx` / `EditorWorkspace.tsx` / `ExportWorkspace.tsx` / `store.ts` 这些本来就大的组装层。
   拆它们是独立的重构工作，不属于本轮；`console.*` 多数是登记过的诊断日志（`i18n-exempt` 同款理由）。
2. **ast-grep 与项目纪律冲突的**：例如 `docs/pre-release-acceptance.md` 的 `MD034 no-bare-urls` ——
   `~/.pi/agent/AGENTS.md` 明确要求「URL 写成纯文本，不要 markdown 链接」，**这条是误报**。
   同理 `../../` 相对导入（项目就是按相对路径组织的，`lint:arch` 才是权威）。
3. **markdown lint**（`MD004` 列表符号、`MD012` 连续空行等）—— 历史文档风格，不影响渲染。

本轮只消了自己引入的那一条（`prefer-at`）：`editor-area-focus.ts` 保留 `[length - 1]` 并在注释里写明
**tsconfig 的 lib 没有 ES2022 的 `Array.at`**（试过，直接编译不过），属于刻意不采纳。

## 3. `design/editor.pen` 画布同步

| 画了什么 | 节点 |
| --- | --- |
| toolsbar left 加第 4 个开关 `预设`（`bookmark` 图标），左段宽 76 → 112、渐隐覆盖右移 | `W6a3Hv` / `j1fFJP` / `KnMVq` |
| toolsbar mid 重排：**编辑源 · 重置 · 撤销 · 重做 · 眼睛 │ 拟合（带文字）· 裁切 · 旋转 · 对比** | `tnhAT`（新增 `hj0cw` 重置、`udArg` 眼睛；`a6H4Fm` 加「拟合」文字并加宽到 68） |
| toolsbar right 清空（眼睛与重置搬走） | `mPyiT` 删掉 `yB7yC` |
| 右栏页签 `预设` → `色盘` | `kEpRZ` |
| 第 3 组样本：`RightPanel / 高级 · 预设` → **`LeftPanel / 预设（左列第 4 个面板）`**（去掉右栏页签条） | `e4WYe` |
| 新建 **`RightPanel / 高级 · 色盘`**：页签条（曲线/色盘/色彩管理，选中色盘）+ 空态「还没有色盘参数」 | `cFdk6`（含 `eYs0e`） |
| 更新两条过时注记（左三段 → 四选一；中段顺序 + 右段为空） | `J5zQnG` / `F8b8N9` / `ygP1t` |

`design/editor.md` 的帧索引与 §2.1 表格同步更新（`Ltq5G` 说明、`A0UG5v` 说明、mid 顺序）。

**未做**：`FPvEw`（ToolsBar 三段式说明帧）里那两份**缩略的工具条副本**（`L9zwBh` / `YbjD6` / `k2FY8u`）
仍是旧按钮序列 —— 它是示意帧，主稿 `P1JiB` 才是准；等下次动到那段说明时一起刷。

## 4. 验证

- `pnpm typecheck`、`pnpm test`（**1421 passed**）、`pnpm lint:colors/arch/i18n`、`pnpm build` 全绿。
- `pnpm check:export`：**通过**（整条用例跑完）。
- `pnpm smoke:ui`：`problems: []`。
- lens 复核：7 个改动文件 0 error（LSP 实扫）。
