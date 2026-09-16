# 左列宽度把手改为自写实现 + 揪出「Windows 产物内嵌旧前端」的流程漏洞

完成时间：2026-09-16 15:17:30 CST

## 一、这一轮到底在修什么

用户的复测结论是：左列拖动条**一拖就卡死**、目录树**变得很矮、下面还空着**。
前两次诊断（store 反馈回路）都不对。这次**先读 Zag splitter 的源码再动手**，
结论是：**「Ark 的 Splitter 套 Ark 的 Splitter」这个结构本身有问题。**

上一版为了满足「只给左边加把手」，把整条左列塞进外层横向 splitter，
而左列内部本来就有竖向 splitter（最近 / 来源）。拖外层时，内层会不断收到
ResizeObserver 回调，量到的是**过渡中的尺寸** —— 于是「树很矮 + 下面空白」；
两边互相触发就是**卡死**。

修法：**左列宽度不再用 Ark**，换成自己写的把手（`ImportWorkspace.tsx`）：

* 按下记起点、`pointermove` 按比例算、`pointerup` 才落盘；
* 上下限 12%–50%，另加 `min-width: 220px` 的像素闸门；
* `role="separator"` + 左右方向键各 2%，可访问性自己补；
* 容器里只剩**一个** splitter（左列内部那个竖向的），没有嵌套、没有 observer 干扰。

## 二、顺带揪出来的**更严重**的问题：产物是旧的

`dist/` 是 14:29 构建的，而 14:56 / 15:07 的前端修复**根本没进 dist** ——
那个 14:59 的 exe 内嵌的是**没有修复的界面**，人类等于白测了一轮。

为什么没发现：当时的核对方式是「exe 里含不含 `dist/assets/` 的资源名」——
**这是个假绿**，因为 dist 自己就是旧的，旧名字当然对得上。

两道防线：

1. `scripts/check-win-artifact.mjs` + `pnpm check:win`：**既比时间也比内容** ——
   exe 必须比 `dist/` 里任何文件都新，且 `index.html` 引用到的每个资源都能在 exe 里找到；
   不合格就打印补救命令（含 `touch src-tauri/src/lib.rs`，逼 cargo 重编把新 dist 嵌进去）。
2. `AGENTS.md` §5.3 硬规矩第 7 条：**先 `pnpm build` 再构建 Windows，构建完跑 `pnpm check:win`**。

## 三、涉及文件

* `src/workspaces/import/ImportWorkspace.tsx`：自写把手（拖动 / 键盘 / 持久化），去掉外层 splitter
* `scripts/check-win-artifact.mjs`（新）、`package.json`（`check:win`）
* `scripts/ui-smoke.mjs`：新增 `widthHandle` 断言（拖动改宽 + 落盘 + **刷新还原**）
* `src/i18n/{zh-CN,en-US}.ts`：`common.resize_left`
* `AGENTS.md` §5.3

## 四、验证（Agent 侧，全部是冒烟）

* `pnpm typecheck` / `pnpm test` / `pnpm lint:arch` / `pnpm build` 全绿
* `pnpm smoke:ui` → `problems: []`，其中：
  * `widthHandle`：拖动让左列 **317px → 437px**，`localStorage` 写入 `leftRatio 0.3033`，
    **刷新后按存的比值还原**（浏览器里对「重启还原」能做到的最接近验证）；
  * `leftColumn`：竖向两段量到 **184px / 391px**（不再是卡在 120/160 下限的畸形状态）；
  * `resizeProbe`：改窗口尺寸后仍应答、面板没塌成 0
    （页面真卡死的话 CDP 的 20 秒时限会先报出来）
* `pnpm check:win` → ✓ 产物与前端一致（exe 15:15:26 > dist 15:15:08）

## 五、遗留

* **真机（Windows）行为仍未验证** —— 归人类：拖动是否不卡、树高是否正常、最大化是否正常。
  本次修复的判据全在浏览器里验证过，但「WebView2 上是否也这样」按纪律只能由人类确认。
* 若真机仍然卡死，说明根因不止「嵌套 splitter」这一条，届时按 `ASSISTANCE.md`
  整理现象（是否闪屏 / CPU 是否飙高 / 日志是否重复刷屏）交外援处理。
