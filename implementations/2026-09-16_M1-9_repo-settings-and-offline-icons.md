# M1-9：库设置接线 + 在线/离线图标化

完成时间：2026-09-16 15:49:05 CST

## 一、真相比预想的简单：后端与弹窗都写好了，**只是没人挂载**

* Rust 侧早有 `repository_settings` / `repository_set_template`（先校验再写，`catalog.db` 与
  `app.db` 两处都写）/ `repository_remount` / `repository_template_preview`；
* 前端 `LibrarySettingsDialog`（含模版输入、变量 chip、实时预览、校验、保存）也写好了 ——
  **但全仓零引用**（`grep LibrarySettingsDialog` 只命中它自己）。这就是「配置功能没实装」的真相。

## 二、这一轮做的

1. `repositories/index.ts` 导出 `LibrarySettingsDialog`；`RepositoryList` 自己持有一个对话框实例，
   卡片右侧那一格 **在线 = 齿轮** → 打开库设置；
2. **离线 = 离线图标**（`cloud-off`）→ 点它走 `store.remount`（= 对所有登记路径重新查找一次）；
3. **两者都不带可见文字**（人类明确要求）：「离线」只进 `aria-label` 与悬停 `title`；
4. 模版保存后回调 `onTemplateSaved` → 外面重新读库列表（卡片上的模版是缓存值）；
5. 画廊加「库卡片」样例（在线 + 离线各一张），冒烟实测：
   * 齿轮在、离线图标在、**离线按钮 `textContent` 为空**、卡片整块文字里没有「离线」；
   * 点离线图标 → 重新查找被触发（`data-remounts` 0 → 1）；
   * **点齿轮 → 库设置弹窗真的开**，且读到了当前模版（浏览器里走 `db.ts` 的降级值）；
6. Rust 测试 `renaming_the_library_folder_simulates_unplugging_the_drive`：
   **改名 = 拔盘** → 离线；改回 → 在线；挪到别处后**登记新路径** → 重新挂上（人类指定的测法）。

## 三、明确不做（记在 `FUTURE.md` §H）

* 导入模版**纯手输**，不做变量下拉/可视化拼装 —— 已把「该怎么做」登记进 `FUTURE.md`（H3 之前）；
* 库设置里除模版以外的项（名称、路径增删、序号计数）本单元不做。

## 四、验证

* `cargo test -p raybend --lib store::repository` → 26 通过（含新测试）
* `pnpm typecheck` / `pnpm test`（402 通过）
* `pnpm smoke:ui` → `problems: []`，`repoCards` 断言全绿（见上）

## 五、遗留

* **真机验证归人类**：插拔 U 盘/移动硬盘时离线图标的行为、点它是否重新挂上。
* 卡片上的**多路径**形态（设计稿要求「不能假设只有一行路径」，见 `design/main.md` §3.3）
  仍然是「多路径时显示 `N 条路径` 的小字」，没有做收起/展开形态 —— 登记在案，未做。
