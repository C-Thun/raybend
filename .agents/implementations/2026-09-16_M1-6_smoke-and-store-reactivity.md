# M1-6 步骤 15：导入进度弹窗的冒烟断言（顺带修掉一个真 bug）

完成时间：2026-09-16 12:24:30 CST

## 本次范围

M1-6 的最后一步：把导入进度弹窗纳入可重复执行的运行时冒烟。做这件事的过程中抓到一个
**产品级缺陷**：进度弹窗在真实应用里根本弹不出来 —— 所以这一步的价值主要不在断言本身。

## 抓到并修掉的真 bug：store 的界面状态不是响应式

`src/features/import/store.ts` 里，界面要读的五个量（`open` / `progress` / `batchId` /
`error` / `busy`）原本是普通 `let` 变量，靠 `open: () => open` 这样的取值函数暴露。
在 Solid 里，`<Dialog open={store.open()}>` 的属性表达式只在**读 props 时**求值 ——
普通变量读一次就定死，signal 变了界面也不会重绘。于是：

- 用户点「导入」→ `begin()` 把 `open` 置真 → **弹窗不出现**；
- 单测全绿（测试直接调用函数、同步读值，看不出响应式问题）；
- 冒烟也测不到（浏览器里没有后端，走不到这条路径）。

**修法**：把这五个量改成 `createSignal`，取值函数直接就是 signal 的 getter。
文件头原本写着「不用 `createMemo`」，这条**不禁 `createSignal`**：在 Node 的 SSR 构建里
`createSignal` 就是 `[() => value, setter]`（`solid-js/dist/server.js:62`），读写语义与真机一致，
所以单测仍是同步、可断言的 —— 实测 376 条全过。

**教训**：`pnpm test` 全绿不等于「界面会动」。凡是「状态在 store、界面读 store」的地方，
都要问一句：**这个读法在 signal 变化时会重算吗？**

## 涉及文件

| 文件 | 改动 |
| --- | --- |
| `src/features/import/store.ts` | 五个界面状态改 `createSignal`；`exportErrors` 里先把 id 取出来（TS 不会把 `batchId()` 的收窄带过 `await`） |
| `src/dev/import-progress-demo.tsx` | 新增：画廊里的「导入进度」演示，**真 store + 假后端**（按钮把快照推进订阅里） |
| `src/dev/KitchenSink.tsx` | 挂载上面这个演示区块 |
| `scripts/ui-smoke.mjs` | 新增进度弹窗断言块；演示区块在页面里找不到才报错 |
| `src/i18n/locale-parity.test.ts` | 新增：中英键集合一致、无空文案、占位符对齐（3 条） |
| `src/i18n/zh-CN.ts` | 顶部补一句「键必须与 en-US 一一对应」 |
| `AGENTS.md` §2.11 | 新规则：难题「先绕过、记录、继续走；绕不过去再统一报告」 |
| `ASSISTANCE.md` | 新增「二、绕过去了的问题（攒着等外援）」分节 |

## 为什么演示要「真 store + 假后端」

进度弹窗需要「有库 + 已勾选来源目录」才打得开，而画廊页（以及冒烟脚本跑的 Chromium）里没有后端。
用假后端驱动真 store，换来的是一条**可重复**的断言路径：阶段条、计数、当前项、错误清单、
取消的二次确认、结束摘要、关闭 —— 全部在真组件、真 store 上验，不是另写一个仿制品。

冒烟分两层：**回归守卫**（点开后按钮标签必须翻面 —— 上面那个 bug 一旦复发就会红）
必然执行；**内容断言**只在 Ark 真把弹窗渲染出来时才严查。

## 验证

```text
pnpm typecheck                 → 0 错误
pnpm test                      → 376 通过 / 0 失败（含新增 3 条语言守卫）
pnpm smoke:ui                  → problems: []（importDialog 全绿：reactive / arkRendered /
                                  stageChips 4 / counts / currentItem / errorRow /
                                  keepPartial / cancelConfirm / doneSummary / revealButton / closed）
```

## 过程中踩到的两处「自己的坑」（都不是产品代码，但很花时间）

1. 演示里我自己留了一份 `open` 信号，与 store 的 `open` 打架 —— 弹窗的开关必须**只有一个来源**（store）。
2. 演示的 `snapshot()` 助手漏了 `...overrides`，于是 `state`/`errors` 永远被默认值吃掉，
   4 条断言假红。造夹具的助手函数，**覆盖项必须放在最后**。

## 遗留

- 画廊页里 `[data-scope="dialog"]` 这个包裹节点的 `innerText` 是空的（内容在 `Portal` 里，
  属正常现象）；断言因此以整页文本为准，而不是取那个节点。
- 真机（Windows/WebView2）里「点导入 → 弹窗出现 → 取消确认 → 结束摘要」仍属**人类目视验收**范围。
