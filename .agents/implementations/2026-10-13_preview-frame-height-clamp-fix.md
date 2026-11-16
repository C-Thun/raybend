# PreviewFrame 高度被内容撑爆的修复（超长纵向图 + 重挂载）

**完成时间**：2026-10-13 03:35:12 CST（崔总 2026-10-13 报的 bug）
**位置**：`src/components/ui/PreviewFrame.tsx`（浏览右栏与编辑左列底部共用那一份）

## 一、症状（崔总原文）

> 找一个超出长宽比上限的纵向图 → 点中它 → 切到信息面板 → 再切回总览：
> 总览高度爆炸，把整个 left 吃光；在别的图之间切一下就能恢复。

比例上限本身是对的：`lib/preview-frame.ts` 夹在 **3:1（宽）～3:4（长）**，尺寸未知时回落 4:3。

## 二、复现与根因（真浏览器实测）

临时复现页（已删）用**真 `PreviewFrame`** + 真结构（底部块固定高 → 切页签 → 切回），
1080×8000 的纵向图，CDP 量高度：

| 时刻 | 外框高 | 里层拟合尺寸 |
| --- | --- | --- |
| 首次显示 | **357px**（= 宽 268 / 0.75，正确） | 46×341 |
| 切信息再切回 | **1883px**（爆炸） | 252×1867 |
| 再切一轮 | 1883px（稳定复现） | 252×1867 |

根因：外框是 **flex 项**，默认 `min-height: auto` 会取**内容的 min-content 高**。
`<img>` 已缓存时（重挂载就是这种情况），它的固有比例把 min-content 撑成
「可用宽度 × 原图高宽比」（252 × 8000/1080 = 1867），**盖过 `aspect-ratio` 算出的高度**。
首次显示之所以正常：图还没解码，第一次布局没有固有比例，`aspect-ratio` 先定下 341，
ResizeObserver 量到 341 后里层拿到显式尺寸，之后再也不会被撑开。
重挂载时图已在缓存里，第一帧布局就带着固有比例 → 直接爆。

## 三、修法

`PreviewFrame` 外框加 **`min-h-0`**（一个类）：

```tsx
"flex min-h-0 w-full items-stretch justify-center rounded-ui bg-surface-bar p-2"
```

外框高度从此只由 `aspect-ratio` 决定；内容超出由里层 `imageArea` 的 `overflow-hidden` 裁
（它本来就有 `min-h-0`）。修完同一复现流程：357 / 357 / 357 / 357（四轮稳定）。

## 四、回归闸门（写进可重复执行的测试）

`scripts/ui-smoke.mjs` 新增一条断言 + 画廊新样例：

* `src/dev/KitchenSink.tsx`：新增 **PreviewFrame 区**（3:2 + 视野框样例；超长纵向 1080×8000 样例，
  带「重挂载」按钮 —— `Show keyed` 就是「切页签再切回来」那种重挂载）。
  ⚠️ 样例容器必须是 **flex 列**（与编辑器左列同形）：`min-height: auto` 只对 flex 项生效，
  写成普通 block 的话样例永远复现不出这个 bug（第一版就踩了，闸门假绿）。
* `scripts/ui-smoke.mjs`：量超长样例在**重挂载前 / 后**的外框高，必须 ≈ `宽 / 0.75`（±8px）。

**闸门自检**（“测试的测试”）：临时撤掉 `min-h-0` 重跑 →
`problems: ["PreviewFrame（超长纵向）重挂载后高度是 1794px，按 3:4 夹取应当是 341px —— 内容把框撑爆了（min-h-0 那条不变量破了）"]`；
恢复后 `problems: []`。

## 五、验证

| 命令 | 结果 |
| --- | --- |
| 临时复现页（CDP 量高度，已删） | 修复前 1883px → 修复后 357px，四轮稳定 |
| `pnpm smoke:ui .../dev/kitchen-sink` | `problems: []`（含新断言）；撤掉修复时如上报红 |
| `pnpm smoke:ui http://localhost:1420/` | `problems: []`（浏览右栏共用同一组件，无回归） |
| `pnpm typecheck` / `pnpm test` | 通过 / 1308 passed |
| `pnpm lint:arch` / `lint:colors` / `lint:i18n` | 全绿（样例图填充改用颜色名 —— `lint:colors` 只管界面色值） |

## 六、遗留

* 真机（Windows + 真实照片）目视：视野框在缩放 / 拖动下的跟手 —— 仍归人类 E2E。
* 同类隐患已排查：`PreviewFrame` 是唯一一处「aspect-ratio 外框 + 图片内容」的组合
  （浏览右栏与编辑左列底部共用），别处没有第二份实现。
