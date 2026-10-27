# 修复导入工作区「批量排除」点击无视觉反馈

完成时间：2026-10-27 00:46:23

## 现象

import 工作流里选中照片 → 点 toolsbar 的「批量排除」→ **画面毫无变化**。
（集合内部确实变了：右列底部的「已排除 N 张」会变，所以看起来像「按钮没接上」。）

## 根因

`src/features/photo-grid/source.ts::importSource` 是导入侧网格的数据适配器。
`PhotoGrid` 的排除态**只读 `GridItem.excluded`**（`PhotoGrid.tsx` 的
`excluded={item()?.excluded === true}`），而 `toGridItem` 自 2026-09-19 统一网格
（`75334bd` 删 `BrowseGrid`）起**从未填过这个字段**：

- 重构前：`PhotoGrid` 直接收 `isExcluded` prop，`const excluded = () => props.isExcluded?.(id())`
  —— 读的时候顺带建立了对 `excluded()` 信号的依赖，所以是活的；
- 重构后：这个回调被搬到 `ImportSourceOptions`，但只挂在了 `TilesSource` 旁边
  （`importSource` 返回对象上的一个 `isExcluded` 键），**网格从不读它**。
  于是 `item()?.excluded` 永远 `undefined`，`Tile` 的变透明 + 正中禁行图标永不出现。

也就是说：数据层（排除集合、计数、传给后端的 `excludedForImport()`）一直是好的，
断的是「数据 → 网格」这一跳。

## 改动

`src/features/photo-grid/source.ts`

1. `toGridItem` 增加 `isExcluded` 参数，填充 `excluded: isExcluded?.(id) === true`
   （读的是导入 store 的 `excluded()` 信号，因此网格会跟着重渲染）；
2. `itemAt` / `itemById` 都传入 `options.isExcluded`；
3. 删掉返回对象上那个没人消费的 `isExcluded` 键，返回类型收回 `TilesSource`
   —— 同一个状态只留一个出口（`GridItem.excluded`），避免后来人以为它生效。

新增 `src/features/photo-grid/source.test.ts`（4 条）钉住：
适配器必须把排除状态带到 `GridItem` 上（`itemAt` / `itemById` 各一条）、
没给回调时是 `false`、集合变化后重新取值跟着变、RAW 角标与排除互不影响。

`scripts/check-import-boot.mjs`（第 ⑥ 段）：假后端 `source_scan` 给来源目录两张照片，
真浏览器里走一遍「选中一张 → 点批量排除 → tile 变排除态 → 再点一次恢复 → 再排除」
并断言右列「已排除 1 张」；最后在 `import_start` 的入参上断言
`excluded` 与网格里那张的路径一致（跨层链路一次算完）。

## 验证

- `pnpm typecheck` —— 0 错。
- `pnpm test` —— 1328 通过 / 0 失败（含新增 4 条）。
- `pnpm lint:colors` / `lint:arch` / `lint:i18n` —— 全绿。
- `pnpm check:import`（另开 `pnpm dev`）—— 通过。
- **反证**：把 `source.ts` 临时回退到修复前再跑 `pnpm check:import`，
  第 ⑥ 段红在「点「批量排除」后 tile 应当立刻变成排除态」，而同一时刻界面底行
  已经写着「已排除 1 张」—— 独立证实了「数据层是好的、断的是画面绑定」这条诊断。

## 遗留

- 真机目视（`pnpm debug:win` 后点一次批量排除，看 tile 变透明 + 禁行图标）
  归人类确认。
- 看图态（`view` / `film` / 对比）**没有**排除标记 —— 那里看的是内容，
  排除动作只在 tiles 里发生；如果以后要在看图里标出来，得走 `photosFromSource`
  这条同一份转换（不要另写一套）。
- 缩略图还没出来时 tile 只有加载态，排除图标不出现（`Tile` 的排除标记挂在
  `hasImage()` 那条 `Show` 里面）。属于既有画法，本轮未动。
