# 更新流程：决策按钮改主色 + spec 口径澄清

完成时间：2026-10-16 04:05:16 CST

## 背景（崔总 2026-10-16 反馈）

崔总在 0.1.2 上体验到的是旧流程（`1a691e8`：启动静默检查 → toast「查看更新」→ 进更新对话框 → 再点下载），
与其设计意图不符：**发现新版本要当场用阻挡式对话框问「下载 / 跳过」**，答后该版本不再烦他。

经核查：**该行为在 0.1.3（`c292e78` 更新流程 v2）已经实现**——`update-center.tsx` 的
`DownloadPrompt` 就是模态对话框（下载/跳过，关掉算跳过并记账），`specs/update-flow-v2.md` §B1 也是这么写的。
崔总还没装 0.1.3，所以没见过新流程。本次不改行为。

## 改动范围

### 1. 决策/前进按钮全部改主色（真实 bug）

`Button` 的默认 variant 是 **ghost**（透明底 + `fg-2` 文字）——更新界面的前进按钮全都没写 variant，
在对话框底色上约等于隐形（崔总：「下载按钮与窗体背景同色」）。修正：

| 位置 | 按钮 | 改动 |
| --- | --- | --- |
| `src/shell/update-center.tsx` `DownloadPrompt` | 下载 | `variant="primary"` |
| `src/shell/update-center.tsx` `InstallPrompt` | 安装 | `variant="primary"` |
| `src/shell/UpdateDialog.tsx` footer | 安装（ready） | `variant="primary"`，`disabled={!canInstall() \|\| busy()}` |
| `src/shell/UpdateDialog.tsx` footer | 继续下载（paused） | `variant="primary"` + `disabled={busy()}` |
| `src/shell/UpdateDialog.tsx` footer | 下载 {version}（idle+found） | `variant="primary"` + `disabled={busy()}` |

不冲突全局风格的核对依据：主色 = `bg-brand` + `fg-on-brand`（`Button.variantClasses`），
禁用态走全局既有样式（`text-fg-3 opacity-60`，`memory/DESIGN.md` §5 的「禁用只降前景、不动面」）；
「检查更新」「关闭」「跳过」「取消」等后退/中性按钮维持 secondary 不变（取消在左、确认在右的层级不动）。

### 2. spec 口径澄清（防再误读）

`specs/update-flow-v2.md` §0 增一条：**「不打扰」≠「藏起来」**——发现新版本必须当场阻挡式问清
（下载/跳过），作答后该版本收声；「永不阻塞」只指下载/校验不挡使用（崔总 2026-10-16 原话口径）。

## 关键决策与理由

- 行为层零改动：0.1.3 的启动决策弹窗与「只问一次」记账已符合崔总设计，无需再动；
  旧 toast+「查看」入口在 HEAD 已不存在（`rg "查看更新"` 无残留）。
- 不动 `ConfirmDialog`：其确认键本来就是 `variant="primary"`（`Dialog.tsx`），无需修。

## 验证

- `pnpm typecheck`、`pnpm test`（1316）、`pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n` 全部通过。
- 未经人类验证：更新弹窗/更新对话框的实际观感（主色按钮、禁用态）待崔总在下一个构建上目视确认。
  注意：崔总当前运行的 0.1.2 仍是旧流程，本次改动要随下一构建才能看到。

## 遗留

- 无。
