# 预设里的 LUT 改为「追加到链末尾 + 从上向下按 LUT 去重」

完成时间：2026-10-31 19:04:34

## 1. 起因

多层 LUT 链（`specs/editor-lut-chain.md`）落地时，预设只处理了**形状**（存整链、旧单支升成一层），
没有处理**应用语义**：`planPresetApply` 仍然是「用预设的链整体替换当前链」。

崔总 2026-10-31 定口径：**预设里的 LUT 贴到当前链末尾**，并按「从上向下」去重
（相同的 LUT **保留最上层那个**）。本轮把这条语义补上，并顺带对齐 area 的求值方向。

## 2. 改动

| 文件 | 改动 |
| --- | --- |
| `src/lib/presets.ts` | 抽出 `LutChainLayer` 类型；`planPresetApply` 新增第 7 个参数 `options: { currentLutChain?, currentLutEnabled? }`；LUT 分支改为**合并**：现有链在前、预设层逐个追加（已存在同 id 的跳过 ⇒ 保留最上层），不超上限的截断交给 store |
| `src/features/editor/store.ts` | `applyPresetSnapshot` 把当前链与总开关传进计划；改链走 `commitChain`（唯一写入口，负责截到 8 层与 `bumpDevelop`），随后**显式落定总开关**；返回值新增 `lutDropped` |
| `src/workspaces/editor/EditorWorkspace.tsx` | `lutDropped > 0` 时提示一次（沿用 wbSkipped 那条提示通道） |
| `src/i18n/{zh-CN,en-US}.ts` | 新增 `editor.preset.lutDropped` |
| `src/lib/presets.test.ts` | 新增两条：追加不替换；同名保留最上层（现有链的 `enabled`/`strength` 原样保留） |

## 3. 关键决策

1. **总开关的归宿**：既有测试「预设的总开关不能被链自动打开」是对的，不能简单 `enabled: true`。
   规则定为：**从空链起步 → 听预设**；**已有链 → 原来开着就保持开着**（`currentLutEnabled || preset`）。
   后者是为了防一种更糟的意外：用户链正开着，套个"存的是关"的预设，整条链突然消失。
2. **上限在哪截**：不在 `lib` 里截（那里没有 `MAX_LUT_LAYERS` 的所有权），交给 `commitChain`；
   丢了几个由返回值告诉调用方，**不静默丢**。
3. **空链 / `id: null` 的预设仍按旧口径清空当前链**：保留"一键去掉 LUT"这个能力，
   与"有 LUT 就追加"不冲突（一个是加、一个是我明确不要）。**这条已标记为待崔总确认**。
4. **预设仍不存 area**：本轮只动 LUT，不引入层的结构；area 侧的预设规则写进
   `specs/editor-area.md` §5.9。

## 4. 验证

* `pnpm typecheck` —— 通过。
* `pnpm test` —— **1358 通过 / 0 失败**（含新增两条）。
* 没有跑 `pnpm build` 与 Windows 侧：本轮不涉及打包与真机。

## 5. 遗留

* 空链预设是否应改为"不动当前链"（现在会清空）—— 待崔总一句话。
* 预设**不捕获 area**、只作用于当前层：这条要等 W4 面板做出来才有 UI 可验。
