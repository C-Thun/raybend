# browse view 的「正在载入原图…」改回视口正中间

> 完成时间：2026-11-01 15:34:04

## 现象

browse 里进 view 看 RAW：占位图已经上屏、真身还在路上时，「正在载入原图…」出现在
view **顶部**（`top-3` 的横条），而不是视口正中间；与 editor 视口里居中的
「正在载入照片…」不一致。

## 根因

这条提示是 2026-10-11「view 两段载入」那一批新加的（`e0f61c4`），当时按「角标」定位
写成了 `absolute start-1/2 top-3 -translate-x-1/2`（顶部居中）。
editor 的同类提示（`ViewportMessage`）一直是父容器 `flex items-center justify-center`
的正中布局，两者因此分叉；实现记录与规格里也一直沿用「角标」的说法。

## 改动

| 文件 | 改动 |
| --- | --- |
| `src/components/ui/viewer/Viewer.tsx` | 提示改为 `absolute inset-0 flex items-center justify-center` 的居中容器，视觉（脉冲点 + 半透明底、`pointer-events-none`、`z-20`）不变 |
| `src/components/ui/viewer/store.ts` | `fullPending` 注释从「角标」改为「视口正中间显示的不拦输入提示」 |
| `memory/FUNCTION-IMAGING.md` | 口径从「角标」改为「居中提示」 |
| `specs/view-true-image.md` | 验收条目同步改为「居中提示」 |

`pointer-events-none` 仍在最外层容器上，载入期间的拖动 / 缩放 / 返回照常可用；
`data-viewer-loading="full"` 与 `role="status"` 保留在原节点上。

## 关键决策

- 不改视觉样式（脉冲点、半透明底），只改定位：崔总这次要的是位置与 editor 对齐，
  不是换一套载入表现。
- 用 `inset-0` + flex 居中而不是 `top-1/2 left-1/2 -translate-*`：与 editor / 全屏
  两处的居中写法一致，也不受提示行高影响。

## 验证

| 项 | 结果 |
| --- | --- |
| `pnpm typecheck` | 通过 |
| `pnpm test` | 1380 通过 / 0 失败 |
| `pnpm lint:colors` / `lint:arch` / `lint:i18n` | 全部通过 |

视觉未在真机验证（按纪律 E2E 归人类）：需要在 browse view 打开一张 RAW，
确认占位期提示位于 view 正中间、真身到达后消失，且期间拖动 / 缩放 / 返回不受影响。
