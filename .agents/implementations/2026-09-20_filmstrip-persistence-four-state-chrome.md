# 胶片带分域持久化与看图四态
完成时间：2026-09-20 23:40:53 CST

## 改动范围

- 胶片带 17 档默认值从 141px 下调到 132px（158×132），上下各 8px，总高默认 148px。
- 胶片带改为受控组件；import / browse 各自持有一份尺寸档位，通过现有
  `setting_get / setting_set` 写入 `app.db.settings`，键分别为
  `filmstrip.import_tile_step` 与 `filmstrip.browse_tile_step`。
- 每个工作流有独立的 2 秒防抖计时器：滚轮期间只更新内存，最后一次变更静止 2 秒后写最终值；
  相同值不安排写入，迟到的启动读取也不会覆盖用户刚改的新值。
- `Tab` 看图布局由三态改为四态：两侧 + 胶片带 → 右侧 + 胶片带 → 仅胶片带 → 仅 view。
  import / browse 继续共用 `viewer-chrome.ts` 与 `PhotoViewingController`。
- browse left 可拖下限从 264px 调整为 220px，与 import left 的像素兜底一致；默认宽度仍是 300px。
- 同步更新 `design/browse.pen`、`design/browse.md`、`BROWSE.md`、`DESIGN.md` 与 `AGENTS.md`。

## 关键决策

1. `FilmStrip` 不直接依赖数据库，只接收档位与变更回调；持久化 store 由 `App` 根层创建，
   从而既保持组件唯一，也让两个工作流的值在切换或关闭看图后继续存在。
2. `settings` 是既有 KV 表，本次只新增两个键，不改变 app.db schema，因此不需要新增 schema migration；
   数据库仍走统一命令与单写者队列，没有另建存储通道。
3. 第二档不是“关两侧”的别名：左、右可见性拆为 `chromeShowsLeft / chromeShowsRight`，
   避免工作区各自判断字符串而再次产生两套循环。
4. 命令体系评估：没有新增独立用户命令；既有 `view.chrome.cycle` / `Tab` 动作直接采用新四态，
   胶片带尺寸仍是区域内隐藏手势，不适合额外加入命令面板或快捷键设置。

## 单元测试

- 默认尺寸、17 档上下限与派生总高度。
- import / browse 分开加载与分开写入。
- 同一工作流连续变化只写最终值，两个工作流的防抖互不取消。
- 迟到的数据库读取不覆盖用户刚调整的档位。
- 四态顺序、左右栏与胶片带可见性、四次循环回原态。
- browse left 下限固定为 220px，非法宽度继续夹取。

## 验证

- `pnpm typecheck`：通过。
- `pnpm test`：通过，73 个测试文件全部通过。
- `pnpm lint:colors`：通过。
- `pnpm lint:arch`：通过。
- `pnpm lint:i18n`：通过。
- `pnpm build`：通过；仅保留既有的大 chunk 提示。
- `pnpm smoke:ui`：通过，`problems: []`。
- `pnpm check:browse`：通过；覆盖默认胶片带尺寸、browse 域 2 秒防抖写入、import 域不串写，
  以及四次 `Tab` 按顺序回到完整布局。
- `node --check scripts/check-browse-boot.mjs`：通过。
- Pencil 中检查了普通 film、compare focus film 与四态说明画板；胶片带容器和缩略图尺寸已同步，
  未发现新增的溢出或布局错误。

## 未由 Agent 声称验证的 E2E

- Windows 真机关闭并重新打开软件后，import / browse 分别恢复自己的胶片带尺寸。
- 连续快速滚动后，数据库只在停止约 2 秒后写最终档位；关闭软件前不足 2 秒的最后变化不保证落盘。
- 四档布局在真实窗口尺寸下逐档切换时，右栏与左右栏的显示顺序符合预期。
