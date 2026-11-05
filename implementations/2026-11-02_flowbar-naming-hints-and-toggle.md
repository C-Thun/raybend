# flowbar 五段定名；层行提示；提示区辅色与总开关

> 完成时间：2026-11-02 02:14:01

## 1. flowbar 五段命名（崔总 2026-11-02）

从左到右：`flow` · `settings` · `hint` · `info` · `fullscreen`。
记进 `AGENTS.md` §11.1（`flowbar` 行 + 新行「flowbar 五段」）与
`memory/FUNCTION-BROWSE.md` 的框图层级图；`flowhint` 词条同步指向 `hint` 段。

## 2. 编辑区层行：指向即提示

* 普通层行：`双击更名，拖动调整顺序`
* `默认` 行：`拖动调整顺序`（它不能改名）
* 实现：`area-panel.tsx` 两行各挂 `onPointerEnter/Leave` → `showFlowHint/clearFlowHint`，
  两行共用 owner `editor.area.row`；面板卸载时清一次（切到 LUT / 定稿面板不留残影）。
* i18n：`editor.area.rowHint` / `editor.area.rowHintDefault`（中英）。

## 3. 提示文字用辅色

`FlowBar` 的 `hint` 文字从 `text-fg-3` 改为 `text-fg-accent`。

新增令牌 `--fg-accent`（`src/styles/tokens.css`，两主题同一批）：

| 主题 | 值 | 说明 |
| --- | --- | --- |
| dark | `var(--brand-2)`（#F0B033） | 对 `surface-bar` ≈7:1 |
| light | `#8F5C00`（同色系深琥珀） | 纯辅色在浅色底只有 ≈1.9:1，读不清；这档对 `surface-bar` ≈4.9:1 |

映射 `--color-fg-accent`，记进 `memory/DESIGN.md` §1.1（并写明「不要把 `text-brand-2` 当正文色」）。

## 4. 提示区总开关（住进 hint 组件）

* `lib/flow-hint.ts` 新增 `flowHintsEnabled()` / `setFlowHintsEnabled()`，
  设备级偏好 `localStorage: raybend.flow-hint.v1`，**默认开**；
* 关掉时：`flowHint()` 恒 `null`，`showFlowHint()` **直接丢弃**（不排队 —— 重新打开不会补发
  关着时指过的地方的提示），并清掉正在显示的那条；
* ❗判定**只在模块内**：所有调用点继续 `showFlowHint(...)`，谁都不读设置（崔总要求）；
* 设置页「编辑器」页新增开关 `settings.editor.hints`；
* 单测：`src/lib/flow-hint.test.ts` 增加「关闭即清 / 关着不显示 / 重开不补发 / 打开后正常」。

## 5. 验证

```bash
pnpm typecheck   # 通过
pnpm test        # 1383 passed / 0 failed
pnpm lint:i18n   # 通过
pnpm lint:colors # 通过（新令牌在 tokens.css）
pnpm lint:arch   # 通过
pnpm build       # 通过；产物 CSS 里 `text-fg-accent{color:var(--fg-accent)}`
```

出包由崔总执行。
