# 控制点本轮代码评审与修复

> 完成时间：2026-11-01 20:58:42
> 范围：本轮「容差重做（亮度×色相 / 白线=容差 / 调试面板）+ 5 条界面修正」；不动历史遗留和其它工作区。

## 1. 评审确认并已修复的问题

| 严重度 | 位置 | 问题 | 修复 |
| --- | --- | --- | --- |
| 高 | `src/features/editor/area-markers.tsx:340` | 容差已改为 100..1000，但读数仍 `×100`，会显示 30000 | 改为直接 `Math.round(tolerance)` |
| 高 | `src/shell/TitleBar.tsx:76` | 只守卫 JSX，生产构建仍生成 `MaskDebugDialog-*.js` | `lazy(...)` 工厂本身也放进 `import.meta.env.DEV` 分支；生产包实测无该 chunk |
| 高 | `src/lib/mask-debug.ts:45` | DEV 写入 localStorage 的调试值会被生产包读取并影响渲染 / 线长 | 初始化、写入、载荷、线长读取均加 DEV 闸门；`node --test` 没有 `import.meta.env`，用可选链兼容 |
| 高 | `src-tauri/src/editor.rs::into_parts` | 即使直接构造 IPC，生产构建仍可能接受 `mask_debug` | `cfg!(debug_assertions)` 下才允许覆盖，release 永远用固化常量 |
| 高 | `crates/raybend/src/display/output.rs` | SOOC 只在前端编辑器载荷里禁用控制点，缩略图 / 导出 / 定稿仍会计算 | 新增 `DevelopStack::area_specs()`，SOOC 下控制点统一旁路；输出与取样共用 |
| 高 | `crates/raybend/src/store/develop.rs::to_spec` | 旧种子会被当新感知域种子解释，旧定稿 / 侧车的 0..1 容差会被新校验拒绝 | 旧种子版本先旁路、旧容差读时折算；`valid` 接受旧域以便读回；补两条回归测试 |
| 中 | `src/workspaces/editor/EditorWorkspace.tsx:1251` | 多个旧控制点迁移共用一份初始栈，后面的点会用前面点的旧种子取样 | 每个点取样前重建当前栈，前面的新种子会进入后一点的「该层输入」 |
| 中 | `src/features/editor/store.ts:738`、工作区落点入口 | 命令面板或切换基准的竞态可绕过 SOOC 按钮禁用 | `armPointArea` / `placePointArea` / `placeAreaPoint` 三处都加 RAW 闸门；补 SOOC 前端回归测试 |
| 中 | `src/features/editor/store.ts` | `setAreaPoint` 收到 NaN 会产生 NaN 容差 | 新增 `clampTolerance`，非法值回默认；补 NaN 断言 |
| 中 | `src/features/editor/store.ts:968` | 定稿 / 侧车不经过 DB `load`，旧容差不会折算，手柄与线长错误 | `loadDevelopState` 对旧 `0..1` 容差统一折算；补 0.15 → 235 断言 |
| 低 | `src/features/editor/area-markers.tsx:261` | 名字翻边只看图像 y×scale，漏掉视口平移 | 用 Rust 完整仿射矩阵求洞口 CSS y |
| 低 | `src/features/editor/area-markers.tsx` | 眼睛关闭 / 不可编辑时组件不卸载，flowbar 提示可能残留 | 增加显式清理 effect |
| 低 | `src/features/editor/panels.tsx:514` | 镜头查询错误仍在面板内联显示 | 错误正文改走 `onError` → toast，面板只保留「重试」动作 |
| 文案 | `src/i18n/zh-CN.ts` | 武装提示与需求口径不一致 | 改为「在图片中点击选取采样点」 |

## 2. 验证

```bash
cargo test --workspace
# raybend lib 1628 passed；desktop lib 194 passed；其余 crate 16 passed
# 合计 1838 passed / 0 failed

pnpm typecheck && pnpm test
# tsc 通过；1381 passed / 0 failed

pnpm lint:colors && pnpm lint:arch && pnpm lint:i18n
# 全过

pnpm build
# 通过；dist/assets 中无 MaskDebugDialog、raybend.mask-debug.v1、editor_mask_defaults

pnpm debug:win
# 通过；exe / worker / dist 均为最新
```

新增或加强的测试：`area_specs_disable_points_for_sooc_and_legacy_seed_versions`、
`legacy_tolerance_is_accepted_and_mapped_to_the_new_domain`、
`SOOC：不能武装/落点；已有控制点只在载荷里旁路，数据不改`、NaN 容差与旧容差折算断言。

## 3. pi-lens 诊断核对

* 之前的 21 个红错全部是**修改过程中的过期缓存**：当前 `tsc --noEmit` 为 0，实时 LSP 检查 10 个相关文件也未再报告错误。
* `store.test.ts` 的 3 个 TS7053 同样是旧版本缓存；现行测试与 typecheck 均通过。
* `no-useless-fallback-in-spread` 命中的是为兼容旧 Rust 缺字段而保留的运行时防线，**不是可安全删除的冗余**。
* `React anti-pattern`、`hyphenated-svg-attribute` 是 React 规则误用于 Solid；本项目 Solid 的 `on:` 原生事件与 SVG 属性写法是刻意的。
* Rust `unwrap` / `expect` 报告均位于 `#[cfg(test)]` 测试代码，不是生产路径 panic 风险。
* `@theme` 是 Tailwind 4 合法语法；复杂度、fan-out、Markdown 风格提示不属于功能缺陷。

## 4. 未修复 / 剩余风险（不扩大本轮范围）

* 旧**定稿**中的控制点现在会安全旁路，而不是按新定义误算；latest 会自动重取样，但旧定稿要保留新口径仍需重新定稿。
* 24MP 多层 CPU 链尚未做真机性能基准；本轮只做了查表与短路，不以推测宣称性能达标。
* 色彩管理 v2（Rec.2020 工作域）与取样样域的统一仍属既有交接书 P1-5，未在本轮扩大处理。
* `src/features/browse/BrowsePanels.tsx` 的 `issueError` 仍有历史内联错误展示，属本轮范围外；后续应纳入同一 toast 通道。`reposError`、LUT 文件缺失等是常显状态，不应机械搬成 toast。
* GUI 视觉、SOOC 真机切换、拖动手感仍需崔总验收；本记录只证明单元测试、生产构建与 Windows 产物构建通过。
