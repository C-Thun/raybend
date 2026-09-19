# M2-W2 换手尾项收口
完成时间：2026-09-20 00:19:01 CST

## 改动范围

收口 `plans/HANDOFF-2026-09-19.md` 留下的三项工作：

1. 修复浏览冒烟中照片 tile 多选后看似不重绘的问题；
2. 接通库数据重建的前端实时进度与完成提示；
3. 为共用胶片带补齐横向像素级锚定。

质量门同时发现上一轮共用库卡片仍从 `browse` 跨 feature 引用 `repositories`，因此将唯一的
`RepositoryCard` 下沉到 `components/ui/`；导入与浏览仍使用同一个组件，没有复制实现。

## 涉及文件

- 冒烟夹具与选择器：`scripts/check-browse-boot.mjs`
- 统一事件入口：`src/api/events.ts`、`src/api/import.ts`、`src/api/db.ts`
- 重建 DTO 契约：`src/api/types.ts`、`src/api/dto-contract.json`、
  `src/api/dto-contract.test.ts`、`src-tauri/src/contract.rs`
- 重建 UI：`src/features/repositories/LibrarySettingsDialog.tsx`、
  `src/features/repositories/rebuild-progress.ts`、中英文语言包，以及从 `App` 到导入侧库列表的
  共用 toast 注入链
- 胶片带：`src/components/ui/viewer/FilmStrip.tsx`、
  `src/components/ui/viewer/film-strip-anchor.ts`
- 库卡片分层：`src/components/ui/RepositoryCard.tsx`、`RepositoryList.tsx`、
  `BrowsePanels.tsx`、`features/repositories/state.ts`
- 单元测试：`rebuild-progress.test.ts`、`film-strip-anchor.test.ts`

## 关键决策与理由

### tile 选中态

逐层探针证明 store 与适配器都已拿到两张选中项；真正中断 Solid 更新的是右侧 `AssetInfo`：
浏览冒烟的 `DEMO_ITEMS` 没跟上新增的 GPS/作者/描述/地点/创建时间字段，选择第一张后
`undefined.toFixed()` 抛出未处理异常。修复方式是补齐夹具，而不是改生产选择状态。

同时把照片断言统一限定在 `main [data-virtual-scroller] [role="option"]` 下，避免共用库卡片
也使用 `role="option"` 后被误当照片；页面未处理异常现在会让冒烟直接失败，防止再次假绿。

### 重建进度

订阅必须先于 `repository_rebuild` 命令建立，小库可能在反过来的空隙里把事件全部发完。
事件按 `repositoryId` 与当前弹窗库双重过滤；弹窗运行期间被关闭或切到另一个库时，旧库结果
只发全局 toast，不会污染新库的计数与摘要。事件订阅失败不拦真正重建，按钮 loading 与最终
命令结果仍可工作。

Tauri 事件的运行时判断与懒加载从导入 API 抽成 `api/events.ts`，导入进度和重建进度共用，
避免出现第二套监听实现。进度事件也进入 Rust/TS 共用 DTO 契约。

### 胶片带锚定

持续记录第一张仍有像素可见的照片及其 `offsetLeft - scrollLeft`；内容指纹变化后优先把同一张
钉回同一横向像素，参考照片消失则退到当前照片，两者都不在新列表时保持原位。
当前照片下标变化仍走原有 `scrollIntoView(nearest)`，内容集合变化则只走像素锚定，避免两套
滚动互相覆盖。导入与浏览两侧继续复用同一个 `FilmStrip`。

## 验证

- `pnpm typecheck`：通过
- `pnpm test`：58 个测试文件通过（含新增阶段映射与胶片带锚定边界测试）
- `pnpm lint:arch`：通过
- `pnpm lint:i18n`：通过
- `pnpm lint:colors`：通过
- `cargo test -p raybend -p raybend-desktop`：通过
  （raybend 805 passed / 1 ignored；desktop 44；worker 6；main 2；doc 2）
- `cargo clippy -p raybend -p raybend-desktop --all-targets`：无告警
- `pnpm build`：通过
- `pnpm check:browse`：通过，浏览启动、目录读取、多选/对比链路与页面异常门禁全绿
- `pnpm smoke:ui`：通过，`problems: []`
- Windows `cargo build -p raybend-desktop --features custom-protocol`：通过
- `pnpm check:win`：通过；exe 晚于 dist，4 个当前资源名全部命中

## 遗留与人类验证

Agent 只完成冒烟，以下仍需人类真机确认：

- 在真实大库执行一次「重建数据」，目视阶段文案与完成 toast，并确认老库缺失元数据被补齐；
- 在胶片带横向滚到中段，进入/退出「只看对比图」并逐张反选，确认参考照片的屏幕横坐标不跳；
- Windows 页面实际画出、重建交互和视觉正确性。程序化检查只能证明资源已嵌入，不能替代目视 E2E。
