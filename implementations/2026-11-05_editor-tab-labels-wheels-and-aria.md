# 编辑右栏页签名：英文 `Color wheels` → `Wheels`，外加三条页签带的组名

> 完成时间：2026-11-05 11:41:55

## 起因

崔总问「新加的色盘英文名精简成 `Wheels` 行不行」。核了一遍宽度账，结论是**该缩，且只该缩英文**。

## 关键事实（宽度账）

右栏内容区宽度 **268px**（紧凑 276−6−2，宽松 288−12−8 —— 两档刻意等宽，见 `tokens.css` 的
`--panel-pad` 注释）。第 3 组三个页签按 14px（`--fs-2`）粗算：

| 页签 | 文字宽 | chip 宽（含 pad-x） |
| --- | --- | --- |
| Curve | ~38 | 62 |
| Color wheels | ~83 | 107 |
| Color mgmt | ~76 | 100 |
| 合计 | ~197 | **~269** |

预算 262（紧凑）/ 260（宽松）—— **改之前英文这一行就是超的**（宽松档更明显）。而且
`SegmentedControl` 根上带 `shrink-0`，压不下去、`fade-x-end` 也不会触发，实际表现只会是
整条往右溢、被滚动容器裁掉右端。中文侧 `曲线·色盘·色彩管理` ≈190/262，一点不紧。

## 改了什么

1. **英文页签** `editor.group.swatch`：`Color wheels` → **`Wheels`**。
   理由不只是短：兄弟页签 `Curve` 就是同一个命名法（**控件形状当名字**），Wheel 是调色台那根拨轮；
   顺带解掉与相邻 `Color mgmt` 的**同前缀撞车**（两个都 `Color…`，眼睛只能读到词尾才分得开）。
   完整词 `color wheels` 由**正文**负责：空态文案本来就是 `No color-wheel settings yet`，天然接住；
   将来四色轮落地时面板自己也该有一行标题（`Color wheels` / `Color grading`），页签永远只是短别名。
   `Wheels` 下合计约 228/262（紧凑）、252/260（宽松）—— 两档都进得去。
2. **中文侧不动**：`色盘` 两字已经最紧，再缩没人懂；中英不对等在这里是刻意的。
3. **三条页签带的 `aria-label` 改成组名**（`SegmentedControl.label` 是整条带的读屏名，
   以前传的是**其中一个页签**的名字）：

   | 位置 | 以前（页签名） | 现在（组名） |
   | --- | --- | --- |
   | 右栏第 2 组（影调/色彩/清晰度/镜头） | `editor.group.tone` | **`editor.tabs.basic`** 基础调整 |
   | 右栏第 3 组（曲线/色盘/色彩管理） | `editor.group.curve` | **`editor.tabs.advanced`** 曲线与色彩 |
   | 左列底部（总览/信息） | `editor.group.view` | **`editor.tabs.leftBottom`** 总览与信息 |

   以前读屏会念成「曲线, 选项卡列表」，里面却是三个不同页签。左列底部那条是同一种病，
   顺手一起修（同一条规矩不该有两处不同写法）。

## 涉及文件

- `src/i18n/en-US.ts` / `src/i18n/zh-CN.ts`：`editor.group.swatch` 改值 + 新增 `editor.tabs.*` 三条
- `src/i18n/index.ts`：key 变更台账（同时也是检查器缓存刷新锚点）
- `src/features/editor/panels.tsx`：第 2/3 组的 `label` + 色盘那两处注释（写明为什么只写 `Wheels`）
- `src/features/editor/left-panel.tsx`：左列底部那条 `label`
- `design/editor.md` §3.13：记下「中文 `色盘` / 英文 `Wheels`」这条命名决定

**不用改的**：代码键 `swatch`（键名与显示名解耦）、`design/editor.pen`（画稿里是中文页签名）、
`specs/editor-presets.md`（同上）、空态文案（英文仍是完整词 `color-wheel`）。

## 验证

- `pnpm typecheck` → 退出码 0（`MessageKey` 由 `zhCN` 推导，新键先进中文包再进英文包）
- `pnpm test` → `tests 1421 / pass 1421 / fail 0`（含中英语言包 key 一致性）
- `pnpm lint:i18n` → ✓ 无硬编码中文文案；`pnpm lint:colors` → ✓ 无硬编码色值
- **未经人类在真机上目视**：英文行是否真的不再溢出（估算 ±10%，`Curve | Wheels | Color mgmt`
  在紧凑/宽松两档都应在 268px 内），以及读屏念的组名。

## 遗留

- 「`Color mgmt` 也偏长」没动：现在有 `Wheels` 腾出来的 ~30px 富余，够用；真要再缩得另议（`CMS` 更差）。
- 工作树里还有其它未提交改动（`design/browse.md`、`memory/*`、`Tooltip.tsx` 等，不是本轮的），
  本轮**没有提交**，避免把别人的改动卷进同一个 commit。
