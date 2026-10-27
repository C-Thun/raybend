# 编辑定稿按钮失效 + latest 被切换覆盖 + 排除的两处收口

完成时间：2026-10-27 01:20:54

本批四件事（崔总 2026-10-27 一轮给出）：M1 看图态禁用批量排除、M2 加载态也显示排除图标、
定稿按钮失效、切 issue 丢 latest。前三件是收口与修 bug，第四件改了「latest 什么时候写」的判定基准。

---

## 1. 定稿按钮「按了没反应」

`src/workspaces/editor/EditorWorkspace.tsx` + `src/features/editor/left-panel.tsx`

**根因**：编辑左栏改版（1025e2f）把定稿按钮从 toolsbar right 搬进左列时，`onFinalize`
接到了**真正保存**的 `finalize()`，而不是「填建议名 + 开弹窗」的那一个。
`finalizeName()` 初始是空串，`finalize()` 第一句 `if (name === "") return;` 直接返回 ——
按钮可点、毫无反应，也不会报错。

**修复**：抽出 `openFinalizeDialog()`（填 `suggestedName` + 开弹窗），命令槽
（`editorActionsImpl.finalize`）与左栏按钮**共用这一个入口**；真正的保存仍是
`finalize()`（弹窗主按钮）。两条路径从此不会再接反。

## 2. 切 issue 丢 latest —— 判定基准改了

`src/workspaces/editor/EditorWorkspace.tsx`（`selectIssue` / `selectLatest` / `applyViewedStack`）

**根因（两个叠加）**：

1. `selectIssue` 先 `applyDevelop(目标栈)` 再 `confirmEdit()` —— `applyDevelop` 会把
   dirty 置上，于是 confirm 把**目标栈**写进 `develop_stacks`（latest）。
   「编辑 → 点 SOOC → 想回最近编辑」时，那次编辑已经被覆盖掉了；
2. 左栏「最近编辑」那一行自 1025e2f 起就是个**没有 onClick 的 div** —— 根本点不动。

**新基准（已写回 `memory/FUNCTION-IMAGING.md`）**：

- latest（`develop_stacks`）**只在用户编辑操作（confirm）时写**；
- **切换查看目标不写**：SOOC / RAW / 历史定稿 / 回 latest 都只是「看」；
- 切走前若还有未落库的改动，**先存一次**（这就是「没退出 editor 就不丢」的全部保证）；
- 目标栈载入界面后立刻 `markCommitted(developRev())` —— 否则下一次 confirm 会把
  「看」的状态写进 latest（同一类覆盖，换个入口复发）；
- 回来时直接读库里那一份（`develop_get`），不回退旧版本；切照片同理（离开时把未落库
  改动存到上一张，回来读该张的 latest）。

**实现**：

- `applyViewedStack(stack)`：载入 + 标已落库 + 切换提示（300ms 才亮）；
- `selectIssue`：`persistPendingEdits()` → `applyViewedStack` → 面板高亮先行；
- `selectLatest`（新增）：`persistPendingEdits()` → **等 `persistTail` 落库跑完**再
  `getDevelopStack`（否则读到的可能是写入前的旧版本）→ `applyViewedStack` → 用后端返回的
  `selection` 定高亮（**前端不重算 profile 指纹**；若这份栈其实是空的，后端会说回
  SOOC/RAW，高亮不会卡在 latest）。两次 `await` 前后都带 `developRev` 守卫 ——
  等待期间用户又切了别处 / 又改了参数，这次点击作废，晚到的结果不许盖掉新选择；
- 左栏 latest 行改成 `<button>`（`onSelectLatest` 由工作区注入）。

## 3. M1：看图态禁用「批量排除」

`src/shell/ToolsBar.tsx` + `src/App.tsx`

那条链路的数据层与网格都正常，但 view / film / 对比态不渲染网格 —— 排除没有可见反馈，
按钮却能点。`ToolsBar` 增加 `excludeDisabled`，App 在 import 流里接
`importActions()?.viewing() || comparing()`（响应式；回到 tiles 自动恢复）。
将来若看图态也画排除标记，去掉这个开关即可。

## 4. M2：缩略图未到时也显示排除图标

`src/components/ui/Tile.tsx`

排除图标原来挂在 `hasImage()` 的**里面**：刚点完批量排除、缩略图还没到，看到的只有转圈。
改成 fallback 里 `local.excluded` 优先渲染同一个禁行图标（同尺寸、同色、同 aria-label），
`animate-pulse` 只在「加载中且未排除」时加；`aria-hidden` 相应放开。

## 影响面核对（`AGENTS.md` §2.19 / §13）

- **XMP**：进入 sidecar 的编辑栈形状没变（`rb:profileJson` 仍是同一个 `DevelopStack`），
  只是写入**时机**变了（不再被「看定稿」触发）—— 侧车内容与映射无需改。
- **数据库 schema**：无变化（`develop_stacks` 结构不动）。
- **命令体系**：没有新增用户可触发功能（「最近编辑」行是面板内的选择入口，
  与其它 issue 行同一层级，不是命令）。
- **缓存**：切 issue 不再刷新 latest 预览 / 缩略图（它本来就不是新编辑）；
  confirm 路径的缓存刷新逻辑未动。

## 顺带修正的旧口径

`memory/FUNCTION-IMAGING.md` 原先记着 2026-09-26 的决定「编辑状态一变即更新 latest，
包括选中 issue；返回切换前状态走撤销/重做」——本轮已被崔总明确替换，旧段标注作废并写明
它会造成的丢失现象；§6 状态表里「切 issue」那行也同步改了。`confirmEdit` 的节点清单里
「定稿 | 切定稿」改为「创建定稿才 confirm，切定稿只看」。

## 验证

- `pnpm typecheck` —— 0 错。
- `pnpm test` —— 1328 通过 / 0 失败。
- `pnpm lint:colors` / `lint:arch` / `lint:i18n` —— 全绿。
- `pnpm dev` + `pnpm check:import` —— 通过；新增第 ⑦ 段：双击进入看图态后断言
  「批量排除」`disabled`、点「返回」回到 tiles 后**自动恢复**可点（M1 的行为守卫）。
- `design/editor.md` §3.8 同步了「切定稿不写 latest、不再记撤销」与「定稿按钮在左栏底部」
  （设计文档先前仍写 2026-09-26 旧口径）。
- 单测：`features/editor/store.test.ts` 的「切换定稿…」用例原断言
  「切稿也要写入 latest 并进入撤销历史」（`developDirty() === true`）—— 与新基准
  正面冲突；已改为「载入后 `markCommitted` → dirty 清」，钉住「查看不写库」这条。

## 遗留 / 待人类确认

- **定稿按钮与 latest 回切**需要真机目视（`pnpm debug:win`）：编辑 → 点 SOOC → 点
  「最近编辑」应回到刚才的编辑；编辑后点「定稿」应弹出命名弹窗。
- 切 issue 不再进撤销栈（以前 confirm 会记一步）。要撤销「切回 latest」这类**查看**
  动作，直接用面板再切回去即可；撤销栈只记真正的编辑。
