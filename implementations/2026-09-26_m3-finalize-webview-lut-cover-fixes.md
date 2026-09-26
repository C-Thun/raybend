# M3 修复：定稿资格、桌面右键、LUT 封面颜色
完成时间：2026-09-26 13:25:59 CST

## 范围

修复当前 M3 的三项反馈，不新增布局或后续功能：
1. latest 自动保存不应阻止定稿；优先匹配命名 issue，最后才归为 latest。
2. 关闭 Tauri / WebView2 默认网页菜单、浏览器快捷键与状态栏。
3. LUT 封面严重丢色：定位编码阶段、修复生成结果，并兼容已导入的旧封面。

## 定稿：两层问题与修复

### 实际复现的前端原因

`App.tsx` 的工具栏在 `EditorWorkspace.onMount()` 注册动作之前读取 `editorActions()`。
旧动作槽是普通模块变量，首次返回 null 后，按钮表达式没有订阅任何编辑信号；后续注册不能触发重算。
这会让定稿按钮一直灰掉，即使后端已经算出 `canFinalize=true`。

用真实 Solid 客户端响应式运行时（Node 子进程 `--conditions=browser`）复现：先创建工具栏的 memo，
再注册动作，再把资格从 false 改为 true，旧实现仍得到 false；修复后同一测试通过。
该测试还验证编辑、读取已保存配置、撤销/重做、卸载与再次进入。

四处同类动作槽（editor / browse / import / viewer）收敛为 `src/lib/action-slot.ts`：
注册身份也成为信号，控件收到注册后再订阅工作区自身的状态；重复注册同一对象不重复通知。
没有再造第二套命令或菜单动作。

### 后端规则

旧源码 `can_finalize()` 通过 `selection()` 判断；它原本允许 `Selection::Latest`，
因此不能把“后端明确比较 latest 并拒绝”当作已复现的根因。本次将资格判定独立于选中态，直接规定：

- 非原始配置，且不等于任何不可变命名定稿，就可以定稿。
- latest 工作副本不进入去重列表。
- 选中态按命名定稿 → SOOC/RAW 原始源 → latest 的顺序匹配。
- 哈希初筛后比较完整 profile，避免哈希碰撞误判。

回归使用真实内存 catalog 与 `develop::save/load`：RAW、SOOC 两侧自动保存 latest 后可定稿；
创建定稿后优先选中该稿件并禁用重复保存；继续修改重新允许；还原到已保存配置后再次匹配原稿。

## LUT：定位证据

截图：`/home/andares/tmp/QQ_1790398397055.png`。左侧封面明显偏灰，中间同样片应用 LUT 保持颜色。
外部提供的样片与内置样片文件字节不同，但解码后 768×576 RGB 像素完全一致。
封面与编辑器本来就共用 `Lut::apply_rgb8`；错误独立于 LUT 求值器。

旧 `webp-rust 0.3.1` 的 Q80 有损编码单独就能复现颜色损失：
384×288 纯橙 RGB `[230,110,20]`，通过真实封面路径编码再解码，变为 `[157,129,107]`。
新增颜色测试在旧实现失败、替换后通过（各通道误差 ≤8）。

替换为 `webp 0.3.1` / `libwebp-sys 0.9.6` 包装的官方 libwebp 编码，保持有损 WebP Q80。
Windows 静态源码构建已通过，复用现有 MSVC，不增加运行时 DLL。解码仍由 image-webp 负责。
依赖许可和库随附的 BSD 版权声明已登记 `THIRD-PARTY-NOTICES.md`。

生成顺序也与编辑器一致：完整 sRGB 样片先套 LUT，再 Lanczos3 缩为 384×288。
作者提供的伴生图只转规格、不再套 LUT，尺寸仍为 384×288 / 768×576 两档。

### 7 个实际 CUBE 的像素检验

测试来源 `C:\src\resource\Free`；输出 `/tmp/raybend-lut-cover-probe/`。
MAE 是编码前后 RGB 各通道平均绝对差（0–255）。色度代理为每像素 `max(R,G,B)-min(R,G,B)` 的均值，非 CIE 色度。
旧封面只读取自当前 app data，没有手动更改实际应用数据。

| LUT | RGB MAE | 未压缩色度代理 | 旧封面 | 修复后 |
| --- | ---: | ---: | ---: | ---: |
| Agfa Optima | 2.716 | 29.736 | 7.203 | 29.293 |
| Fashion Film | 2.737 | 36.820 | 8.861 | 36.272 |
| Fast Film | 2.766 | 36.084 | 8.615 | 35.428 |
| Film Fade | 3.025 | 51.093 | 12.672 | 50.632 |
| Soft Fade | 2.756 | 36.346 | 8.946 | 35.715 |
| Street Crush | 2.730 | 35.260 | 8.764 | 34.720 |
| Velvia 100 | 2.938 | 41.533 | 10.266 | 40.850 |

日常单测另用非恒等 LUT，把封面与真正的 `render_develop` 输出比较。
外部 7 文件的 probe 为 ignore 测试，只有排障手动运行，不拖慢日常测试。

### 已导入封面的自动修复

- LUT 私有目录新增 `cover-meta.json`，记录派生封面版本与来源；当前缓存版本 2。
- 导入时保存伴生原图为 `cover-source.<ext>`，外部目录移走仍可重建。
- `lut_cover` 读取时检查版本：旧封面只重建一次；有效缓存直接返回。
- 缓存丢失但 LUT 可用时仍尝试重建；LUT 文件丢失时可继续显示已有封面。
- 版本和缓存写入共用核心 `fs_atomic`，从现有 FullCache 的写入下沉并改为独立临时文件；
  cover 先发布、版本最后发布，中途失败可在下次重试。并发写、覆盖写和失败清理都有单测。
- 这是派生缓存升级，不改变 app.db schema、不另建 DB 迁移通道；LUT ID、分类和照片引用保留。
- 当前库中 7 个 CUBE 的原始导入文件均经只读检查确认存在，新版首次读取可以自动修复。

**遗留边界**：旧版本没有记录封面来源。若外部源文件已经丢失，无法确定原封面是否为作者自带图，
保留旧图，避免错误地换成统一样片。新导入通过来源记录与原图留存解决此问题。

## Tauri 原生接管

新增本地 `desktop-behavior` 插件，以 `tauri.conf.json` 的插件配置控制：
`browserContextMenus=false`、`browserAcceleratorKeys=false`。
在全局 `on_webview_ready` 中设置 WebView2 原生 `AreDefaultContextMenusEnabled`、
`AreBrowserAcceleratorKeysEnabled`，并关闭浏览器状态栏和内置网页缩放。
主窗口、闪屏及后创建的全屏窗口共用此处配置。

DOM 兜底仅 preventDefault，不 stopPropagation，应用自身右键处理器仍可收到事件；
重复页面钩子只安装一次。Windows 原生部分通过实际 Windows 编译与配置单测；
非 Windows 平台只使用右键 DOM 兜底，不宣称其浏览器快捷键已统一接管。

## 主要文件

- `crates/raybend/src/store/issues.rs`：资格 / 匹配规则及真实 latest 保存回归。
- `src/lib/action-slot.ts`、`action-slot.test.ts`：统一响应动作槽与客户端回归；四个 actions.ts 复用。
- `crates/raybend/src/develop/lut_import.rs`：正确编码 / 顺序 / 版本化封面与颜色回归。
- `crates/raybend/src/fs_atomic.rs`、`display/full_cache.rs`、核心 lib.rs：共用原子派生文件写入。
- `src-tauri/src/lut.rs`：导入来源留存、读取自动重建、缺图可再生。
- `src-tauri/src/desktop_behavior.rs`、`desktop_context.js`、lib.rs、tauri.conf.json：原生设置及 DOM 兜底。
- `src/lib/desktop-context.test.ts`：取消默认菜单但保留应用右键事件。
- Cargo.toml / Cargo.lock、THIRD-PARTY-NOTICES.md、FUTURE.md、IMAGING.md、design/editor.md：依赖与口径登记。

## 验证（Agent 冒烟 / 单元）

| 命令 | 结果 |
| --- | --- |
| `pnpm typecheck` | 通过 |
| `pnpm test` | 957 通过（包含真实 Solid 客户端动作注册回归） |
| `pnpm lint:colors / lint:arch / lint:i18n` | 通过 |
| `cargo test -p raybend --lib` | 1093 通过 / 4 ignore，测试执行 9.34s |
| `cargo test -p raybend-desktop --lib` | 87 通过 / 1 ignore |
| 7 个 CUBE probe（ignore 显式运行） | 7 文件全部通过，执行 1.43s |
| Windows `develop::lut_import` | 7 通过 / 1 ignore，0.14s |
| Windows `fs_atomic` | 2 通过，0.02s |
| Windows `store::issues` | 6 通过，0.03s |
| Windows `desktop_behavior` | 2 通过 |
| `pnpm debug:win`（含 pnpm build、主程序与 worker） | 通过；最终 exe 为 2026-09-26 13:21:45 CST |
| `pnpm check:win` | 时间 / 4 资源引用 / worker 协议均匹配 |
| `pnpm smoke:ui http://127.0.0.1:1420/dev/kitchen-sink` | 通过，problems=[]，没有控制台错误 |
| `git diff --check` | 通过 |

构建仍有现有的大 JS chunk 提示，不影响成功结果。
冒烟第一次误传了应用首页 URL，陈列室专用断言因缺少样例失败；改用脚本要求的 kitchen-sink 后通过。
未把首页上不存在的陈列室样例当作产品缺陷。

## 命令 / 热键与真机边界

定稿仍复用 `editor.issue.finalize`（命令面板 / Edit 菜单）；默认键留空以避免低频永久操作误触，
可由快捷键设置绑定。其余改动是自动缓存修复或全局默认 WebView 行为，没有新增可触发动作。

**未做真机 E2E / 目视验证**。请用 `C:\rb-target\raybend\debug\raybend-desktop.exe` 复查：
打开 LUT 库让旧封面重建、同样片应用同 LUT 比较；编辑后定稿按钮可用、读取已存稿件不能重复定稿、
撤销/重做同步；主窗 / 全屏右键不再出现网页菜单，应用快捷键仍可使用。

前一轮 M3 和其他会话的导出设计改动保留；没有发布、推送或生成安装发布物。
