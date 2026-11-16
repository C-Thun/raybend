# tile 尺寸档位：17 档、400 封顶

完成时间：2026-09-20 04:54:53 CST

## 范围

人类 2026-09-20 口述（原文）：

> 「明显减小 tiles 放大的最大尺寸，比如根本不用放那么大，感觉放到 400 上下就够了，
> 然后把这 9 级过渡改成 17 级过渡，让尺寸变化变得更加细腻，方便适配不同的屏幕尺寸。」

同一轮里另交办「写好的计划不用给我看、直接开干」——`plans/M2-W3.md` 已按此调整。

## 改了什么

| 项 | 旧 | 新 |
| --- | --- | --- |
| 档数 | 9 | **17** |
| 阶梯 | `128,152,180,216,256,304,360,432,512` | `128,136,148,160,172,184,196,212,228,244,260,280,300,324,348,372,400` |
| 级差 | 约 ×1.19 | 约 **×1.074**（1.05–1.10 之间，有测试钉住） |
| 最大 | 512 | **400** |
| 默认 | 256（下标 4） | **260**（下标 10） |

* 默认为什么是 260：新表按几何阶梯排下来，离旧默认 256 最近的是 260（差 1.6%，肉眼无差）。
  **不为了保住「256」这个数字去把某一级掰弯** —— 阶梯均匀更重要。
* 存过的旧下标**按尺寸换算**：`migrateTileStepIndex()` + 存储里的 `tileStepScale` 标记
  （`lib/display-prefs.ts`）。旧「最大档 512」→ 新上限 400，而不是直接夹取成中档。
  写入时带上标记；读取时只有**缺标记**（旧记录）才换算一次，之后原样读。

## 涉及文件

* `src/lib/tile-flow.ts` —— 新阶梯 + `LEGACY_TILE_SIZE_STEPS` + `migrateTileStepIndex`；
* `src/lib/display-prefs.ts` —— `TILE_STEP_SCALE` 标记 + 读取时迁移；
* 测试：`src/lib/tile-flow.test.ts`（17 档 / 上限 400 / 级差区间 / 迁移）、
  `src/lib/display-prefs.test.ts`（旧记录迁移 / 新记录原样）、
  `src/features/photo-grid/store.test.ts`（越界与 NaN 不再写死下标）；
* 文档：`BROWSE.md` §5.8、`DESIGN.md` §8.3、`design/main.md` §3.2.1/§4.4（9 档 → 17 档、
  默认 208 → 260）、`plans/M2-W3.md` §1.1；
* `scripts/check-browse-boot.mjs` —— 顺带修了一条**脆弱断言**：点「一行里空着的槽位」时
  纵坐标落在右上角 **toast 区**（320 宽 fixed 一列，卡片 `pointer-events-auto`）被吃掉，
  删除测试留下的 toast 正好盖住那一格。改成从上往下试几个点、取第一个不被 toast 盖住的。
  （这是工装问题，不是产品问题：手点那一下确实会点到 toast 上。）

## 验证

* `pnpm typecheck` 0 ｜ `pnpm test` **715** ｜ `lint:colors` / `lint:arch` / `lint:i18n` ✓ ｜
  `pnpm build` ✓ ｜ `pnpm smoke:ui` → `problems: []` ｜ `timeout 400 pnpm check:browse` ✓。
* 未做：真机目视（档位手感归人类）。

## 遗留

* Windows 产物需要重编（前端变了）—— 本波开工前会一起做（`pnpm build` → cargo → `pnpm check:win`）。
* 画布上 tile 尺寸只是示意（`design/main.md` 已写明「尺寸以实现为准」），无需改稿。
