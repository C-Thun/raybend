# 定稿确认后「伪假定稿」+ 按钮变灰 + LUT 开关失灵（Solid 响应式爆栈）

> 完成时间：2026-10-31 01:45:57

## 现象（崔总真机，稳定复现）

点「定稿 → 弹窗确认」之后：

1. **不生成任何定稿**（列表里连占位条目都没有）；
2. 定稿按钮**永久变灰**（像「已经定过稿」）；
3. 重启后再进，仍然**看不到**那份定稿；
4. 同一时刻 **LUT 面板按钮也坏掉** —— 切不过去、按下态也不对。

第 3 条是关键判据：库里根本没有行，所以不是「写入成功但没刷新」。第 4 条是关键线索：
两件毫不相干的 UI 同时僵住，说明坏的不是某个业务流程，而是**整个响应式运行时**。

## 根因（一条，全部解释得通）

`src/features/editor/store.ts::updateChrome` 当年写成：

```ts
const next = produce(chrome());   // ← 同步【读】了 chrome 信号
setChrome(next);                  // ← 又【写】它
```text

而 `src/features/editor/left-panel.tsx:142` 把它放在 **effect** 里调：

```ts
createEffect(() => {
  if (props.issueFocusTick > 0) props.store.setLeftPanel("issues");
});
```

于是这个 effect 在执行时把自己**订阅到了 `chrome`** 上：写 → 立即失效 → 再跑 → 再写 ……
生产构建没有「潜在循环」保护，同步递归到

```ts
RangeError: Maximum call stack size exceeded
  at runUpdates(pe) ↔ completeUpdates(me)   （互递归）
```

探针在真机页面里抓到**一模一样的栈**，`Error.stackTraceLimit` 拉满后仍是这对互递归。

**爆栈点决定了一切症状**：它在 `finalize()` 的第三句

```ts
setFinalizeOpen(false);
setFinalizeBusy(true);
setIssueFocusTick((value) => value + 1);   // ← 就在这一行爆栈
try { confirmEdit(); await persistTail; await createIssue(...) }  // 一行都没执行
finally { setFinalizeBusy(false) }                               // 永远不执行
```

| 现象 | 直接原因 |
| --- | --- |
| 不生成定稿 | `createIssue` 从未发出（后端、数据库、XMP 全都无辜） |
| 按钮永久变灰 | `finalizeBusy` 卡在 `true`，`finally` 没跑 → `canFinalize()` 恒 false |
| 重启也看不到 | 库里没有行（不是显示问题） |
| LUT 开关失灵 | 整棵 Solid 响应式树在这次溢出里被炸断，之后所有 signal 更新都不再驱动 DOM |

触发条件为什么是「定稿确认」：`issueFocusTick` 只有 `finalize()` 会 +1；这个 effect 是
2026-10-12 左栏改版加的，而定稿按钮本身 2026-10-27 才修好、弹窗 2026-10-30 才通，
所以这枚雷一直没机会炸。

## 修复

| 文件 | 改动 |
| --- | --- |
| `src/features/editor/store.ts` | `updateChrome` / `resetChrome` 改成 **setter 的函数式更新**（`setChrome(state => produce(state))`）—— 函数形参不建立依赖，从根上消除「写自己读的信号」 |
| `src/features/editor/left-panel.tsx` | 调用点补 `untrack(() => props.store.setLeftPanel("issues"))`（第二道保险，并写明意图） |
| `src/features/editor/store.test.ts` | 两条回归：effect 里 `setLeftPanel` / `toggleLeft` **只跑一次** |
| `package.json` | `test` 加 `--conditions=browser`（见下） |
| `memory/ARCHITECTURE.md` §9.1 | 记下这个「C 面」坑与规矩 |

**测试为什么要带 `--conditions=browser`**：Solid 在 Node 默认条件下解析到 **server 构建**，
那里 `createEffect` 是空函数（effect 跑 0 次），根本复现不了。带 browser 条件后
旧代码立刻抛 `RangeError`，新代码通过 —— 判据有效而且不依赖真机。

## 验证

* 回归测试**先在旧代码上红**：`✖ … RangeError: Maximum call stack size exceeded`（两条都红）；
  改完后两条均 `✔`，且 `runs === 1`。
* `pnpm test` → **1343 passed / 0 failed**（带 `--conditions=browser`）。
* `pnpm typecheck`、`pnpm lint:colors`、`pnpm lint:arch`、`pnpm lint:i18n` 全绿。
* `pnpm build` 通过（本次只改前端，`dist/` 已重建，等 `pnpm debug:win` 编进 exe）。

## 顺带记录（不是本 bug，但排查时看到的）

* 探针创建过一条测试定稿：**asset 43 / id 14 / 名字「探针测试 01」**（我为了验证后端
  `issue_create` 通路直接调的 IPC）。修好后可在界面里删掉它，或告诉我来删。
* 定稿按钮「为什么灰」目前界面**不给任何解释**（后端 `can_finalize` 的两种否定理由：
  当前配置与已有定稿相同 / 与原始源相同）。建议后续在按钮旁给一句带变量的说明
  （属新需求，未在本轮实现）。

## 遗留 / 待人类确认

* **需要重新 `pnpm debug:win`** 才生效（前端嵌在 exe 里）。
* 真机复测：编辑 → 定稿 → 确认 → 列表应立刻出现新条目；之后 LUT 开关应正常。
* 同类风险已按「store 更新函数一律函数式 setter」收口，但仓里可能还有别的
  `const next = f(someSignal())` + 写同一信号的写法；`lint:arch` 暂未加静态规则。
