# 补上加载态排除图标的 DOM 冒烟断言

完成时间：2026-10-27 02:11:33

## 背景

上一轮评审指出：`Tile` 的「缩略图未加载时也显示排除图标」只有代码路径验证，
`smoke:ui` 的排除样例全部有图，缺少加载态的 DOM 断言。

## 改动

- `src/dev/KitchenSink.tsx`
  - 在 Tile 陈列室新增「加载中的排除态」样例：`<Tile loading excluded />`，
    外层加 `data-tile-loading-excluded` 作为稳定冒烟锚点。
- `scripts/ui-smoke.mjs`
  - 在既有 tile 画廊采样中单独读取该样例，断言：
    - 没有 `img`（确实是加载态，不是误传 `src`）；
    - 禁行图标存在、位于 tile 正中、颜色使用中性次级色；
    - 没有 `animate-spin`（排除态优先于加载动画）。
  - 修正新增浏览器采样代码中的外层模板字符串转义问题（注释里的反引号会破坏
    `Runtime.evaluate` 的源码字符串）；居中测量改为图标矩形对 tile 矩形，
    不拿图标父级自身的居中结果冒充 tile 居中。

## 验证

- `node --check scripts/ui-smoke.mjs` —— 通过。
- `pnpm typecheck` —— 0 错。
- `pnpm test` —— 1328 通过 / 0 失败。
- `pnpm lint:colors` / `lint:arch` / `lint:i18n` —— 全绿。
- `pnpm dev` + `pnpm smoke:ui` —— 通过，最终 `problems: []`。

## 遗留

该断言覆盖陈列室的真实 DOM 渲染；import 工作区的端到端排除链路仍由
`pnpm check:import` 负责，真机视觉确认仍按既有分工归人类。
