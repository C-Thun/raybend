# raybend — 架构与工程基线（memory/ARCHITECTURE.md）

> 本文件是**技术架构与工程基线的事实来源**：前端分层与依赖方向（§0–§4）、版本基线（§5）、
> 已确定的架构决定（§6）、调研结论存档（§7）、构建与排障（§8）、Solid 壳组件纪律（§9）。
> 2026-09-27 由 AGENTS.md 瘦身迁出（§3/§5.3/§6/§7/§2.17）与原「前端架构」文档合并而成；
> 章节号 6.x/7.x 与 AGENTS.md 原编号保持一致，便于旧引用对照。
>
> 相关：`AGENTS.md`（每次会话的纪律核心与索引）、`memory/DESIGN.md`（视觉与配色）、
> `memory/PLAN.md`（排期与问题清单）、`memory/FINISHED.md`（已完成部分）、
> `design/main.md`（界面设计稿说明）。
>
> 前端分层部分建立时间：2026-09-15 ｜ 本文件合并时间：2026-09-27

---
## 0. 一条总原则

**层数比网页应用少，但每一层的边界必须是真的。**

桌面客户端没有 SSR、没有多租户、没有路由级懒加载的分层压力，所以**不引入**那些只在
Web 应用里成立的层（BFF、加载器编排、页面级数据预取…）。但下面三件事必须有明确归属，
否则它们一定会糊成一团：

1. **基础元素**（按钮、分段控件…）——它们决定界面的一致的下限
2. **一级模块**（`recent`、`source-tree`…）——它们决定代码能不能改
3. **横切范式**（`easy copy`、`easy destroy`…）——它们跨模块复用，但又不能变成垃圾抽屉

---

## 1. 分层

```text
┌─ 组装层 ─────────────────────────────────────────────┐
│  src/App.tsx              路由与全局 Provider         │
│  src/workspaces/import/   把 features 拼成导入三列    │
│  src/shell/               titlebar / flowbar / toolsbar │
├─ 模块层 ─────────────────────────────────────────────┤
│  src/features/<domain>/   一级模块（自带 state + 视图）│
├─ 基础元素层 ─────────────────────────────────────────┤
│  src/components/ui/       原语 + 范式组件              │
├─ 纯逻辑层 ───────────────────────────────────────────┤
│  src/lib/                 无框架、无 DOM 的算法与模型  │
├─ 令牌层 ─────────────────────────────────────────────┤
│  src/styles/              tokens / scrollbar / motion  │
├─ 素材层 ─────────────────────────────────────────────┤
│  src/assets/              图片、字体等**纯数据**        │
└──────────────────────────────────────────────────────┘
```

**依赖方向只能向下**（箭头从下往上被 import）。反例：
`src/lib/shortpath.ts` 里 import 任何组件、`components/ui` 里 import `features` —— 都是错的。

### 1.1 各层的职责与禁令

| 层 | 放什么 | 禁令 |
| --- | --- | --- |
| `src/styles/` | CSS 变量、`@theme` 映射、全局滚动条与动效 | 禁止写组件样式；色值只在这里（`tokens.css`）与 `memory/DESIGN.md` |
| `src/assets/` | **纯数据**：logo 等图片素材（经 Vite import，走上哈希与「文件不存在就构建失败」） | 禁止放代码（`.ts`/`.tsx`）；它自己不 import 任何东西 |
| `src/lib/` | 纯函数与领域模型：`shortpath` / `tile-flow` / `format` / `tree` / `clipboard` / `easy-destroy` / `appearance` | **禁止 import solid-js 的渲染 API、禁止碰 DOM**（`createSignal` 这类响应式原语可以，它不依赖 DOM）；每个文件**必须**有同名 `*.test.ts` |
| `src/components/ui/` | 无业务含义的基础元素（16 个原语）与横切范式组件（`EasyCopy` / `EasyDestroy`） | 禁止 import `features/`；禁止硬编码色值；文案必须走 i18n |
| `src/features/<domain>/` | 一级模块：自己的类型、state、视图、单元测试 | **禁止 import 其它 `features/`**；禁止直接 `invoke`；禁止自带令牌或色值 |
| `src/shell/` | 外壳三行（`titlebar` / `flowbar` / `toolsbar`） | 只做布局与「当前工作流」的装配，不写业务逻辑 |
| `src/workspaces/<flow>/` | 把若干 feature 拼成一个工作区（当前只有 `import`） | 只做组合与布局，不自己持有模块内部状态 |
| `src/api/` | Tauri 命令的前端封装（一层薄薄的类型化壳） | 只有这里 `import { invoke }`；未来的 `raybend-ipc` 契约与之逐条对应 |

### 1.2 为什么不给每个 feature 都配一个「service / repository」层

后端的偏好在这里会过度设计：桌面应用的「服务」就是 Tauri 命令，
再包一层 service 只会把类型与错误码搅浑。所以：

- **`src/api/` 就是唯一的数据出入口**，模块直接调用它（`listRecentDirs()`、`scanDirs()` …）
- 需要缓存/派生时，把**纯计算**放进 `src/lib/`，需要状态就放进该 feature 的 store
- 只有真的出现「多个后端 + 可替换实现」时才引入接口抽象（例如将来的 RAW 后端可插拔，
  但那个抽象在 **Rust 侧的 `raybend-raw`**，不在前端）

---

## 2. 一级模块清单（与 `memory/DESIGN.md` §10.2 的对应）

`memory/DESIGN.md` §10.2 叫它们「第二批：大组件」，在本文件里它们的落点是**模块**：

| `memory/DESIGN.md` §10.2 | 代码位置 | 归属工作单元 |
| --- | --- | --- |
| A `TitleBar` | `src/shell/TitleBar.tsx` | M1-4 |
| B `FlowBar` | `src/shell/FlowBar.tsx` | M1-4 |
| C `ExifStrip` | `src/features/exif-strip/` | M1-4 |
| D `ToolsBar` | `src/shell/ToolsBar.tsx` | M1-4 |
| E `SourcePanel` | `src/workspaces/import/SourcePanel.tsx`（组合层） | M1-5 |
| F `RecentList` | `src/features/recent/` | M1-5 |
| G `SourceTree` | `src/features/source-tree/` | M1-5 |
| H `SelectedTiles` | `src/features/selected-dirs/` | M1-5 |
| I `PhotoGrid` | `src/features/photo-grid/` | M1-5 |
| J `GridControlBar` | `src/features/photo-grid/`（同模块的子视图） | M1-5 |
| K `RepositoryList` | `src/features/repositories/` | M1-5 |
| —— | `src/features/import-queue/`（导入执行与进度） | M1-6 |
| —— | `src/features/browse/`（浏览：查询 store、行模型、网格、标记工具栏、左右两列） | M2-W1 阶段 6 |
| —— | `src/workspaces/browse/`（浏览工作区三列组合层） | M2-W1 阶段 6 |
| —— | **`src/features/commands/`（命令注册表 + 命令面板 + 快捷键设置 + 分发器）** | M2-W3 |
| —— | **`src/lib/{key-chords,commands,command-match,shortcuts}.ts`（命令与快捷键的纯逻辑，均带单测）** | M2-W3 |
| —— | **`src/features/<域>/actions.ts`（工作区 / 看图件的**动作槽**：命令通过它调真实动作）** | M2-W3 |
| —— | `src/dev/SpikeViewport.tsx` + `src/api/spike.ts`（**渲染 spike 诊断页**，`?spike=1` 才加载） | M2-W1 阶段 7 |

### 2.0 命令与快捷键的归属（M2-W3 定）

| 层 | 放什么 | 规矩 |
| --- | --- | --- |
| `lib/{key-chords,commands,command-match,shortcuts}.ts` | 键位解析/格式化/匹配、命令**类型**与冲突判定、模糊匹配、偏好存储与导入导出 | 纯逻辑；**不认识** DOM / i18n / store（分层规矩），所以判定在 lib、**措辞在 features** |
| `features/commands/catalog.ts` | **全部命令**（id / 文案 key / 分组 / 作用域 / 默认键 / when / enabled / run） | 不 import 任何 feature：动作由组装层通过 `CommandDeps` 注入（组装层是唯一能碰所有 store 的地方） |
| `features/commands/dispatcher.ts` | **全应用唯一的键盘监听** | 输入框 / 模态 / Ark 组件内部的键不归它管（`shouldHandleKey`）；元素自己的焦点行为（`Tile` 的 Enter、列表 roving focus）也不接 |
| `features/<域>/actions.ts` | 工作区 / 看图件的**动作槽**（挂载时注册、卸载时置空） | 槽里只有动作没有状态：读状态一律回 store（唯一事实来源） |

**Rust 侧（M2-W1 新增）**：

| 模块 | 位置 | 职责 |
| --- | --- | --- |
| 渲染 | `crates/raybend/src/render/`（`viewport` / `gpu` / `presentation` / `scene` / `stats`） | 视口变换与坐标口径（**Rust 独有**）、wgpu 上下文与离屏渲染、合成测试图、帧统计与报告 |
| 库目录操作 | `crates/raybend/src/repo/dirs.rs` | 新建子目录 / 删除空目录（名字校验按 Windows 口径、深度空判定） |
| 目录树菜单 IPC | `src-tauri/src/dirs.rs` | 上面三个动作的命令层（`root + rel`，越界在 Rust 挡） |
| spike 调试窗口 | `src-tauri/src/spike_viewport.rs` | `label = "spike-viewport"` 的窗口 + 独立渲染线程 + 命令层（**不动主窗口**） |

一个 feature 目录的固定形状：

```text
src/features/recent/
├── types.ts        # 该模块的类型（尽量复用 src/api 的返回类型）
├── store.ts        # createRecentStore()：signals + 命令，导出 Provider
├── RecentList.tsx  # 视图（只做呈现，逻辑在 store）
├── RecentList.test.ts
└── index.ts        # **唯一对外出口**：别的层只能从 index.ts 拿东西
```

`index.ts` 是硬要求：它让「模块的公开面」变成一件可以 review 的事，
也避免上层按文件路径钻进去拿内部实现。

---

### 2.1 原生编辑视口：最小平台呈现适配（2026-09-25）

编辑器继续共用 Rust/wgpu/WGSL 管线。平台变化只从
`crates/raybend/src/render/presentation.rs::PresentationAdapter` 接入：一个枚举、一个产品选择入口、
一个 wgpu instance 配置出口；目前不需要 trait、动态分发、注册表或第二套渲染循环。

| 入口 / 责任 | 当前行为 |
| --- | --- |
| `PresentationAdapter::for_editor()` | Windows 选择 `WindowsComposition`；其它平台选择 `PlatformDefault` |
| `WindowsComposition` | DX12 + `DxgiFromVisual`，GPU 内容位于父窗直接绘制层之上、WebView 子窗之下 |
| `PlatformDefault` | 保留 wgpu 原生默认；spike 显式用它，macOS/Linux 暂作接入占位，不代表已完成跨平台验证 |
| `instance_descriptor()` | 先应用策略，再读取环境覆盖；Windows 产品初始化失败会报错，不自动退回已知冲突的 HWND 路径 |
| `SurfaceComposition` | 独立控制 alpha 与初始底色；编辑器为 `Opaque`，透明 spike 为 `Transparent` |
| `GpuContext` | 唯一的设备、surface、纹理、WGSL、绘制、resize 和恢复实现；按传入配置建 instance |
| `src-tauri/src/render_window.rs` | 唯一窗口句柄与尺寸胶水；适配器不依赖 Tauri、不持有窗口 |

以后做 macOS/Linux 时，先验证目标系统的原生表面与 WebView 合成关系，再在这里增加对应策略，
修改 `for_editor()` 的平台选择。若平台需要原生 layer/子窗，再在现有窗口胶水增加必要接入；
现在不伪造未实现的 Metal/Wayland 接口，也不承诺只改一个枚举就能完成移植。
坐标、照片处理、着色器和渲染循环继续共用。离屏渲染不参与窗口合成，保持现有独立初始化。

Windows 当前是整个编辑视口的 wgpu 上下文使用 DX12，不存在 Vulkan → DX12 的跨 API 拷贝。
策略原因、排障顺序与证据边界见 [原生视口经验 §8](docs/native-viewport-coordinate-guide.md)。

---

### 2.2 系统互操作统一采用 adapter（2026-09-30 崔总明确要求）

**新增或重构系统互操作时，先设计 adapter 边界，再接系统 API；不能等实现散落后再抽象。**
包括系统状态/资源读取、设备识别、变更通知、系统设置入口和原生呈现协作。
业务层依赖自有类型与能力契约，平台 adapter 封装原生句柄、API、路径、版本探测与兼容回退；
前端仍只经既有 API 层发意图，Tauri 薄壳负责生命周期与装配，不承担色彩业务。

沿用已有 `PresentationAdapter`、窗口句柄胶水与 `SystemPreferencesAdapter`，按职责扩展；
不因采用 adapter 就新建顶层框架、重复 GPU 上下文或渲染循环。接口须表达不支持/未知/失败，
不能把回退伪装成检测成功；监听应可释放，异步快照/结果应防止旧状态覆盖新状态。
提供 fake adapter 支持确定性的无头测试，真实平台行为仍由真机验证。

色彩管理的具体 adapter 分工、快照/事件与状态范围见
[`specs/color-management.md` §5.3](../specs/color-management.md)。本轮是架构约束，尚未实施系统色彩适配。

---

## 3. 状态归属

| 状态 | 归属 | 理由 |
| --- | --- | --- |
| 主题、密度、语言 | `src/lib/appearance.ts` / `src/i18n/index.ts` + 组装层 | 全局、设备级偏好（现有 localStorage 键） |
| 当前工作流（导入/浏览/编辑/导出） | `src/shell/` 的 store | 外壳自己的状态 |
| 当前仓、当前选中目录 | **app store**（`src/App.tsx` 提供） | 跨模块共享，且 `memory/DESIGN.md` §12.4.1 明确要求跨面板同步 |
| 模块内部（Recent 列表、树的展开集合、已选目录集合…） | 该 feature 自己的 store | 换模块时整块丢弃，不需要全局清理 |

**首次设备偏好（2026-09-29）**：`src/api/system-preferences.ts` 提供可替换的
`SystemPreferencesAdapter`；桌面原生快照来自 `src-tauri/src/system_preferences.rs`，
Windows/macOS 外观复用 Tauri，Windows 首选显示语言读取 Win32，WebView 的媒体查询/首选语言补齐未知项。
Linux 外观暂走 WebView 媒体查询，避免把当前 Tao 未配置 portal 时固定返回的 Light 当成探测结果。
`src/lib/system-preferences.ts` 只做纯默认值转换。`src/index.tsx` 在首屏前初始化并持久化：
已有设置分别优先；没有主题时按系统选择，检测失败 dark；中文显示语言用 zh-CN，其它/未知用 en-US。
原生探测限 800ms，之后不订阅系统变化。macOS/Linux 复用 adapter 接口，原生语言接入与真机验证留到相应平台阶段。
app.db 继续经既有迁移框架创建，设备主题/语言不复制进数据库。规格与证据见
`specs/first-start-system-preferences.md`、`implementations/2026-09-29_first-start-system-preferences.md`。

**splash 启动语言**：`src-tauri/src/splash.rs` 在文档初始化脚本中嵌入
`public/splash-locale.js`，先于主界面系统语言初始化捕获软件保存的语言；缺失/非法/读取失败时英文。
本轮启动的各 WebView 复用 `raybend.splash-launch.v1` 派生快照，启动标识每次重建，
只保留一条；软件语言的唯一来源仍是 `raybend.locale`。主界面首次保存中文或手动切换不改本次 splash，
下次启动才重新读取。splash 不等待 IPC/app.db、不探测系统语言、没有新权限或网络请求。
`pnpm check:startup [url]` 覆盖首次英文及后续中英文图片加载；Windows 原生窗口时序仍需真机确认。
规格见 `specs/splash-language.md`，实施证据见 `implementations/2026-09-29_splash-language.md`。

**展开状态与选中状态必须分开存**（`memory/DESIGN.md` §12.4.1）：
树的展开集合属于 `source-tree` 模块内部；「当前选中目录」属于 app store。
这样「为显示选中而强制展开树」在结构上就**做不到**，不需要靠自觉。

---

## 4. 可执行的约束

口号不管用，所以两条规则有脚本：

| 规则 | 检查方式 |
| --- | --- |
| 色值只在 `tokens.css` | `pnpm lint:colors` |
| 层之间只能向下依赖、`features/*` 互不 import | `pnpm lint:arch`（`scripts/check-architecture.mjs`） |

`lint:arch` 的实现刻意保持简单：读文件里的 import 语句，比对**层序号**与 **feature 名**，
不解析语法树 —— 规则本身简单到不需要 AST。它也会拒绝 `components/ui` 里出现 `features/` 字样。

---


---

## 5. 版本基线（2026-09-15 核实）

> ⚠️ **下表只描述 raybend 软件本体。** `website/`（官方站点）走的是**另一套完全不同的选型**
> （Solid 2.0 线、Tailwind v4、Lucide、纯静态站），对照见 §4 的「`website/` 是官网」。

| 层 | 选型 | 当前版本 | 备注 |
| --- | --- | --- | --- |
| 外壳 | Tauri | `@tauri-apps/cli` 2.11.4（3.0 处于 alpha） | 分层为 3.0 迁移做准备，见 §6.2 |
| 构建 | Vite | 8.x | RapidRAW 亦用 Vite 8 |
| UI | Solid | **1.9.x（选定）**；`@solidjs/router` 1.0.0 | Solid 2.0 仍是 RC（2.0.0-rc.8），迁移登记 `memory/FUTURE.md` |
| 样式 | Tailwind CSS | 4.3.3 | CSS-first 配置 + CSS 变量令牌层 |
| 组件原语 | **Ark UI**（`@ark-ui/solid`） | 5.39.x | 已定；实测对比见 §7.6 |
| 图标 | **Tabler**（`@tabler/icons-solidjs`） | 3.46.0 | 已定；实测对比见 §7.7 |
| RAW 解码 | rawler 0.8.0（LGPL-2.1-only） | 上游 dnglab | 可插拔后端，候选见 `memory/FUTURE.md` |
| 渲染 | wgpu + WGSL | 需锁定版本 | **版本锁定有前例教训**：RapidRAW 将 wgpu 降到 29.0 以规避 Apple 设备 P3 色偏 |
| DB | SQLite（rusqlite + 迁移工具） | — | 见 §6.4 存储架构 |
| 色彩 | lcms2（预留，后期接入） | — | 第一阶段不做色彩管理 |
| 类型桥 | specta / tauri-specta | — | Rust 类型 → TS 类型，避免手写漂移 |
| Rust 工具链 | **1.98.1**（`rust-toolchain.toml` 锁定，不改全局默认） | rawler 要 1.89，Tauri 3 要 1.95 | 全局默认仍是用户自己的版本，仓库内自动切换 |

---

---

## 6. 已确定的架构决定

### 6.1 渲染架构 = 原生 wgpu + 透明挖洞（方案 B）

- WebView2 是覆盖整个窗口的一层；照片视口在 webview 中间**挖洞（透明）**，Rust/wgpu 绘制其下的原生 GPU 底板；Windows 底板通过 DirectComposition visual 呈现。
- **Windows 产品呈现默认 = DX12 + DirectComposition visual + Opaque**（2026-09-25 真机确认）：
  Tauri 透明窗口在重绘时会用 softbuffer/GDI 清底；GPU 若直接呈现到同一 HWND，会在窗口边缘越出屏幕时闪烁。
  必须让产品 GPU 表面通过 `DxgiFromVisual` 位于独立合成层，不能退回「同 HWND + 移动时补帧」。
  `GpuContext` 仍是一份实现，后端/呈现配置在 `render/presentation.rs::PresentationAdapter` 收口；
  `SurfaceComposition` 只管 alpha，不能用透明度隐式决定平台后端。macOS/Linux 暂留默认策略入口，
  不提前做跨平台框架。保留 `WGPU_BACKEND` / `WGPU_DX12_PRESENTATION_SYSTEM` 显式诊断覆盖。
  适配边界见 `ARCHITECTURE.md` §2.1，源码证据与经验见 `docs/native-viewport-coordinate-guide.md` §8。
- **接口纪律（防返工的三条红线）**：
  1. 视口状态由 Rust 独有：`Viewport { zoom, pan_px, rotation, fit_mode, clip_rect, dpr, surface_format }`；前端只发送交互意图，不做坐标数学。
  2. 覆盖层（蒙版、裁剪柄、直方图采样框等）使用 Rust 提供的**同一变换矩阵**，禁止前端自行推导像素对齐。
  3. WGSL 源码与 uniform 结构体属于 Rust crate，前端不接触像素格式与色彩空间。
- **分类使用**：Library/网格视图用 DOM 虚拟化（webview 内，便于选中/键盘/拖拽）；Develop 视口用原生 wgpu 直绘。
- **透明链是硬约束（2026-09-24 血泪）**：wgpu 直绘在 webview **底下**，所以洞口之上（一直到 `html` / `body`）**任何一层都不能有底色**。`html` / `body` 必须**常驻透明**（`index.css` 里显式写 `transparent` —— `color-scheme: dark` 下「没有背景」的画布会被刷成不透明的默认深色），底色由各页面自己的根容器画。W2 只做了「根 div → main → 洞口」、漏了 `html/body` → 编辑器视口**从来没出过图**（spike 页在 JS 里自己置了透明所以看着正常）。冒烟有断言盯着这两层。
- **风险与退路**：Tauri 官方未一级支持「webview 上叠加原生 GPU 内容」（相关 issue #8246、#13740），但社区已有多例可用实现（RapidRAW 的 WGPU 直绘、`clearlysid/tauri-wgpu-cam`）。若 Windows 上出现不可解的合成/DWM 问题，退路是切换 Tauri 的 CEF 运行时（自带宽高一致的 Chromium），已登记 `memory/FUTURE.md`。
- 色彩管理**预留不做**：第一阶段只保证 SDR 下不变形，ICC/HDR 在后期里程碑接入。
- **坐标契约（原生视口开工前必读）**：见 §7.9 与 `docs/native-viewport-coordinate-guide.md`。
  三条真机踩出来的铁律：**WebView DPR ≠ 系统缩放**（文字缩放 110% 会叠加）、
  **wgpu 的 layout 与设备绑定**（设备重建必须连 layout 一起重建）、
  **渲染线程的 panic 会静默冻住画面**（界面照旧响应 —— 必须捕获并重启）。

### 6.2 为 Tauri 3.0 迁移做准备（现在就要遵守）

Tauri 3.0 已进入 alpha（`3.0.0-alpha.0`），已知关键变更：

- **运行时 crate 化**：移除了 `wry` / `cef` feature flag，`tauri::Wry` 被移除，改为显式依赖 `tauri-runtime-wry` / `tauri-runtime-cef`。
  → **纪律**：不要在任何地方引用 `tauri::Wry` 具体类型，也不要在 `Cargo.toml` 里假定 feature flag；把窗口句柄抽象成我们自己的 `NativeWindowHandle` 类型，包在 `src-tauri` 内。
- `tauri-build` 不再把 resources 复制到 cargo target 目录，dev 运行改为从源路径解析资源。
  → **纪律**：资源路径一律走 Tauri 的 path API，不硬编码相对路径，不假设 target 目录里有资源。
- 顶层导出整理（issue #14011）：避免深层 `use tauri::...` 的边角导出，只用稳定的一级 API。
- MSRV 提升到 1.95；Linux 端迁 GTK4（对 Windows 优先的本项目暂不相关）。
- **迁移预案**：等 3.0 进入 beta/rc 时，执行顺序为「先升 `tauri` 与 `tauri-build` → 再改运行时 crate 依赖 → 最后处理插件 API 变动」。因为业务逻辑不在 `src-tauri` 内，预期工作量可控。

### 6.3 RAW 解码层（可插拔）

- 首选 **rawler 0.8.0**（上游 [dnglab](https://github.com/dnglab/dnglab)），但必须在 `raybend-raw` 内做成**后端可插拔**接口，禁止其它模块直接依赖 rawler 类型。
- 已知事实：rawler 是 **LGPL-2.1-only**（已核实与 GPLv3 兼容〔LGPL v2.1 → GPLv3 的转换路径〕，因而可静态链接进本项目的 AGPL-3.0）、**API 不稳定且不遵循 SemVer**、**没有 GPU 依赖（纯 CPU，rayon）**、dnglab README 明说 **Windows 未官方支持**，且明确声明「**不要把 dnglab/rawler 用于处理不可信文件**」。
  → **纪律**：RAW 解码必须在**独立 worker 进程**中执行，崩溃不得带走主进程；对文件做大小/格式预检。
- rawler 提供：CFA 像素、black/white level、白平衡系数、色彩矩阵（`xyz_to_cam` / `color_matrix`）、active/crop area、orientation、以及**嵌入式预览与缩略图**。
- rawler 不提供：高质量去马赛克、降噪、镜头校正、色调映射、色彩管理。这些属于未来的 `raybend-develop`，第一阶段只有最基础处理。
- 第一阶段策略：**优先使用 RAW 内嵌 JPEG 预览**做网格缩略图与快速查看（毫秒级，比解码快 1–2 个数量级），真正解码走后台任务。

### 6.4 存储架构 = 全局库 + 每仓 catalog（分层）

目录布局：

```text
%LOCALAPPDATA%\com.cthun.raybend\
  app.db              # 全局：应用设置 / 库注册表 / 标签词典 / LUT 分类与文件元数据
  backups\            # 迁移前快照（VACUUM INTO，保留 7 份）
  cache\_sources\thumbs.db # 当前缩略图缓存：源文件与命名 issue 的两档 AVIF
  luts\<LUT id>\    # 用户导入的 .cube / Hald 本体与一张 384 或 768 宽 4:3 WebP 封面
  logs\

<库根目录>\            # 用户指定，可多个
  catalog.db          # 库真相源：库 ID、资产、元数据、编辑栈/latest、不可变 issue、修订
  photos\             # 导入的落地目录（模版决定 photos/ 内的相对路径）
    2026-08-15\
      MYP0001.png
      _RAW\
        MYP0001.ORF   # 同名 RAW 放同级的 _RAW/（见 REPOSITORY.md §4.1）
  cache\              # **库内缓存**（M3-W3 起）：大图（每 issue 一份 AVIF）
    full\<资产 id>\
      latest-raw-v6.avif  # latest 编辑结果（管线版本号随实现升级）
      issue-42-<hash>-raw-v6.avif # 命名定稿独立的 1920 AVIF
  index.db            # 派生索引（缩略图索引、人脸、相似度）—— 可删可重建；真需要时才建
```

> **布局于 2026-09-15 按用户口述变更**：`catalog.db` **直接在库根**（不再藏于 `.raybend/`），
> 导入物落在库根的 `photos/`，`repo.json` 取消（库身份记在 `catalog.db` 内 + 中央 `app.db` 登记）。
> **完整业务规格见 `memory/FUNCTION-REPOSITORY.md`**（库身份 / 同路径多库 / 同库多路径 / 在线离线 / 导入模版 / 序号 / 重名 / RAW 分流 / 目录透传）。

- **缓存分两处**（M3-W3 定）：**小图**（网格 / 胶片带 / 看图的缩略图）在
  `%LOCALAPPDATA%\com.cthun.raybend\cache\_sources\thumbs.db`（当前实现：源文件与定稿按不同缓存键共享一库；几万行小 BLOB）；
  **大图**（每个命名 issue 独立一张 1920 AVIF）在**库根**的 `cache/full/`（跟着库走，换机器/搬盘不用重渲染）。
  两处的编码格式统一 **AVIF 质量 90 / 4:4:4**（人类 2026-09-24 定；快照恒为 AVIF，
  不考虑换 JXL —— 格式范围与 JXL 定位见 `memory/FUTURE.md` C8）。
- **真相源规则**：本地编辑/评分/关键词以 **DB 为准**，XMP 只是互操作通道；RAW 永不写回原文件（只写 `.xmp` sidecar）。issue 的 XMP 表达契约见 `specs/issue-xmp-contract.md`，实现规格见 `specs/xmp-sidecar.md`（2026-09-30 草案待审）；**XMP 不是第一公民**，整库备份 = 直接拷 repos 目录；当前尚未写出 sidecar。
- **写并发**：SQLite 单写者 → 采用**单一写者 actor**（专属线程 + 专属连接，所有写操作串行化），读走连接池；批量事务；`PRAGMA journal_mode=WAL, synchronous=NORMAL, busy_timeout=5000, foreign_keys=ON`。
- **库身份与多路径**（`memory/FUNCTION-REPOSITORY.md` §2）：库身份 = `catalog.db` 内的唯一 ID；`app.db` 记录「库 ID → 多个路径」。
  路径挂载了哪个库靠**读该路径下 catalog.db 的 ID** 比对，而不是靠路径字符串 —— 这是「同路径不同库 / 同库多路径」的机制。
  生产入口通过核心 `CatalogSessions` 获取固定位置的租约，实际连接再次核对 ID；
  `availability` 区分未知/探测中/在线/离线/不可用及具体原因，不把权限、损坏和版本故障压成拔盘。
  连接代次与快照 revision 分开，IPC 使用十进制字符串。排队写执行时检查代次，旧写者退场后才可重开；
  app.db/全局状态锁不承载探测、扫描和连接退出等待。细则见 `memory/FUNCTION-REPOSITORY.md` §2。
  手动定位复用身份读取、PathForms 和有界探测；超时后台工作仅验证，不迟到登记。
  位置移除的本机事务与同库 acquire/退场互斥，后端再次保护当前根；无需 schema 变化。
  前端连接事实由 App 唯一 store 持有；卡片只接所需呈现字段，错误文案归 i18n，避免 UI 反向依赖 IPC DTO。
- **跨库搜索**：`ATTACH` 多库 + `UNION ALL`；必要时在 `app.db` 维护轻量定位表做快速筛选。
- **备份**：升级前自动 `VACUUM INTO` 快照（保留 7 份）；程序版本低于库版本时**拒绝打开**并提示。
- **升级等待（2026-09-29）**：app.db 后台打开/迁移，产品 splash 等它返回；设置与最近目录命令复用 blocking，等待打开锁不占窗口事件线程。catalog 经既有 CatalogSessions 开库迁移，外壳按执行 ID 保存活动快照；前端先订阅再查询、按精确十进制 revision 拒绝旧状态，订阅失败只降级查询内存快照。MigrationGate 复用现有视觉，专层拦截全窗输入；updater 原生端也检查正在执行的升级。新增必需预置图时须延长整体升级阶段并记录完成状态，当前只处理 SQL，详见 FUNCTION-REPOSITORY.md §2.2.1。
- **禁止**把 catalog 放在云同步盘 / 网络盘上：创建/登记/打开经既有位置分类器执行策略；
  已登记库不删除，报告不支持的位置。解析最近存在祖先可识别部分符号链接/目录联接；
  云客户端与挂载识别仍有启发式限制。导入来源读取不受 catalog 策略牵连。

### 6.5 缩略图 / 预览缓存

> **三种图（thumb / preview / 大图）的规格、生成节点与显示规则见 `memory/FUNCTION-IMAGING.md`**
> （唯一事实源，含格式支持范围）；本节只讲架构与红线。

- 当前尺度：网格 384、胶片带 192、过渡/定稿预览 1920（最长边）；1:1 大图走实时解码和显影，不进持久缓存。
- **中性缩略图与编辑结果分开**：未编辑源文件按来源生成；latest 参数变化后刷新当前小图与预览；每个命名 issue 的 384/192/1920 AVIF 快照独立保留，删除定稿才清理该定稿快照。
- 缓存键包含资产/来源、尺寸档、渲染签名（含管线版本）；命名 issue 还含 issue ID 与 profile 哈希。latest 作废不能误删命名快照。
- 当前小图存 `%LOCALAPPDATA%\com.cthun.raybend\cache\_sources\thumbs.db` 的 SQLite BLOB；1920 AVIF 存库根 `cache/full/<asset_id>/`。更细的按库迁移、容量上限、frecency 淘汰与缓存设置面板属于后续优化，不能写成 M3 已有功能。
- **红线**：删掉整个缓存目录后，功能降级但可从原图和配置重建；`app.db`、`catalog.db` 与导入的 LUT 本体不是可删缓存。

---

---

## 7. 调研挖出的关键真相（必须记住）

### 7.1 SQLite 的边界（结论：够用，风险在写并发）

- 单库上限 281TB；WAL 下「多读者 + 单写者」，现代 NVMe 上写者可达数千 TPS。
- 量级估算：10 万张照片的元数据约 100–300MB，100 万张约 1–3GB（含索引）。行业先例：Lightroom `.lrcat`、darktable `data.db`+`library.db`、digiKam 四个 SQLite 库，没有一个是「SQLite 撑不住」。
- **真正的风险是并发写**（导入 + 缩略图生成 + AI 分析同时写）：用 §6.4 的单写者 actor 解决。

### 7.2 中文搜索

SQLite FTS5 默认 `unicode61` 分词器**对中文基本无效**；必须使用 `tokenize='trigram'`（SQLite 3.34+），英文列可另建 `porter` 索引。

### 7.3 文件身份与跨平台路径

- **不要用路径做主键**：Windows 用 `GetFileInformationByHandleEx(FileIdInfo)` 得到 `(VolumeSerial, FileId128)`，可稳定追踪重命名/移动（NTFS 上可靠）；路径只作展示与 fallback。
- 跨平台注意：**macOS 使用 NFD 规范化且大小写不敏感，Linux 是字节串且大小写敏感，Windows 大小写不敏感**。
  → 现在就定死：路径存储用 **NFC 规范化值 + 保留原始名 + 额外的大小写折叠列**用于唯一索引。

### 7.4 嵌入式预览优先

几乎所有相机 RAW 都内嵌全尺寸 JPEG 预览与缩略图。读取它比解码快 1–2 个数量级。导入与浏览先取内嵌预览，真正做到「瞬间出图」；只有进入 1:1 或显影时才做完整解码。

### 7.5 参考实现：RapidRAW（`https://github.com/CyberTimon/RapidRAW`）

在相片编辑与「Tauri 做图像软件」这件事上做得很好，**找不到方案时优先参考它的实现思路**（注意许可，见下）。
真实技术栈（来自其 `src-tauri/Cargo.toml` 与 `package.json`）：

- **Rust 侧**：`rawler`（**用自己的 fork**：`CyberTimon/RapidRAW-DngLab`）、`wgpu 29.0`（注释：降级以规避 Apple 设备 P3 色偏）、`ort`（ONNX Runtime，`load-dynamic`）+ `tokenizers`（AI）、`image_hasher`（感知哈希/相似度）、`mozjpeg-rs` / `webp` / `jxl-encoder` / `jxl-oxide`（编解码）、`nalgebra` + `glam` + `half`（色彩数学 / f16）、`mimalloc`、`trash`、`kamadak-exif` + `little_exif`、`quick-xml`（XMP）、`fuzzy-matcher`（命令面板模糊搜索）、`gphoto2`（联机拍摄，仅 Unix）。
- **前端侧**：React 19 + **`lucide-react`** + **`react-window`**（网格是虚拟列表，**不是 canvas tile**）+ `konva` / `react-konva`（蒙版与裁剪的 canvas 覆盖层）+ `dnd-kit` + `zustand` + `i18next`（从一开始就国际化）。
- **没有 SQLite 依赖**：它是**sidecar 流派**（编辑状态写文件），没有 catalog 数据库。这正是 raybend 要走与之不同的路线的地方。
- **渲染**：编辑器视口用「透明挖洞 + wgpu 直绘」；官方博文记录改造前后**拖动滑块 20fps → 120fps**，瓶颈原本是「JPEG 编码 → IPC → 浏览器解码」。
- **许可：AGPL-3.0**（已核实 `LICENSE` 为 GNU Affero GPL v3）。
  → **本项目同样采用 AGPL-3.0-only，两边许可一致**：在保留版权与许可声明、并注明来源的前提下，其代码可被借鉴/移植进 raybend（需逐文件确认文件头的许可标注）。
  → 其 fork 的 rawler（`CyberTimon/RapidRAW-DngLab`）仍属 **LGPL-2.1 派生**，可直接作为我们的 rawler 来源（用于吸收其解码修复），使用前确认 fork 新增代码的许可标注。
  → 反向约束：一旦 raybend 对外提供网络服务，就必须按 AGPL 第 13 条向使用者提供对应源码。

### 7.6 组件原语：Kobalte vs Ark UI

两者都是 headless（无样式 + 无障碍）组件库，都要自己写样式（与 Tailwind 搭配无冲突）。区别：

| 维度 | Kobalte | Ark UI |
| --- | --- | --- |
| 出品 | kobaltedev 社区（Solid 原生） | Chakra 团队（`chakra-ui/ark`） |
| 底层 | Solid 原生实现 | 基于 **Zag.js 状态机**，多框架同构（React/Solid/Vue/Svelte） |
| 定位 | SolidJS 专属 UI 工具包 | 跨框架设计系统底座，45+ 组件 |
| 文档/生态 | 面向 Solid 用户 | 文档更完整，跨框架资料多 |
| 风险 | 跟随 Solid 版本演进 | 多框架抽象层厚，Solid 端偶有滞后；但 Zag 的状态机在复杂控件（日期选择、滑块、菜单）上逻辑更严谨 |
| 关键结论 | 若选 Solid 1.9，二者都可用；**Kobalte 更「薄」、Ark 更「全」** | 复杂的组合控件（日期区间、级联菜单）Ark 的状态机更省心 |

决策留到设计阶段（用 Pencil 画出第一批界面控件后再定），结论写入 `memory/PLAN.md` 对应里程碑。

**✅ 已决（2026-09-15）**：本项目选 **Ark UI**（`@ark-ui/solid`）—— 原因是 `splitter`（可拖拽分栏）
与 `tree-view`（标签层级树）是我们最需要且最难自研的两个组件，只有它提供；其活跃度也明显领先。
实测数据（版本号、组件数、活跃度）见 `memory/FINISHED.md` 附录 A.10。

### 7.7 图标体系（核实结果，2026-09-15）

| 图标集 | 数量 | 风格 | 许可 | 填充态 | 影像领域覆盖 |
| --- | --- | --- | --- | --- | --- |
| **Lucide** | ~1600（sitemap 实测） | 仅描边（24px 网格） | ISC（Feather 派生部分 MIT） | ❌ **官方明确不支持 fill**；可用 `fill=currentColor; strokeWidth=0` 变通，官方 issue 自述「对约 60–70% 图标可用」 | **比预期好**：实测存在 `pipette`(吸管)、`contrast`、`crop`、`frame`、`proportions`、`layers`、`tags`、`folder-tree`、`gallery-thumbnails`、`git-compare`、`badge-check`、`star-half`、`grip-vertical`、`sliders-horizontal`、`combine`、`lasso`、`swatch-book`、`table-2`、`map-pin`(及多种变体)；实测**缺失** `history`、`flip-horizontal` |
| **Tabler** | 6,184（5,130 描边 + **1,054 填充**） | 24px / 2px 描边，描边与填充**成对文件** | MIT | ✅ 独立 filled 文件 + `icon-filled` class | 数量最大，领域图标较全 |
| **Phosphor** | 1,248 图标 × 6 字重 | thin/light/regular/bold/**fill**/**duotone** | MIT | ✅ 官方 fill + duotone，**按 16px 起设计** | 中等 |
| **Heroicons** | ~300（含变体约 888 文件） | outline / solid / mini(20) / micro(16) | MIT | ✅ solid 变体 | 弱，纯通用 UI |
| **Material Symbols** | 2,500+ | outlined / rounded / sharp × **FILL 可变轴**（0↔1 连续插值） | Apache-2.0 | ✅✅ 单一可变字体即可切换 | **最强**：实测含 `raw_on`、`tonality`、`vignette`、`dehaze`、`healing`、`gradient`、`auto_fix_high`、`burst_mode`、`tune`、`straighten`、`exposure`、`add_photo_alternate` |

**结论与纪律**：

- 第一阶段**不需要自绘几十个图标**；原则是「**选定一套 → 只用套内图标做组合 → 降低数量**」。
- 首选 **Tabler**（唯一同时满足「数量大 + 描边/填充成对 + MIT + 小尺寸清晰」）；次选 Lucide（生态最成熟、风格最现代，但激活态需用别的手段表达而非填充）。
- 编辑模块的领域专用图标（histogram / vignette / tonality / dehaze 等）从 **Material Symbols** 借用（Apache-2.0 可与 MIT 混用，但混用需注意描边粗细与网格差异）。
- 图标尺寸规范：16 / 20 / 24 三档，激活态优先用**强调色 + 背景块**表达（Web 设计里有大量成熟做法），不依赖填充变体。
- **✅ 已决（2026-09-15）**：本项目选 **Tabler**（`@tabler/icons-solidjs`）；
  实测数据见 `memory/FINISHED.md` 附录 A.10。选型原则仍是「只用套内图标做组合、降低数量」，不自绘几十个图标。

### 7.8 Tauri 分发与 npm

- **Tauri 桌面应用不需要发布到 npm**，也不需要占位：分发物是安装包（NSIS / MSI）与自动更新元数据，npm 只服务于 JS 库消费者。`@tauri-apps/cli` 是构建期依赖，不是产品发布物。
- 只有当 raybend 将来对外提供**可被 JS 项目引用的库/插件**（例如 CLI 封装、SDK、编辑器 Web 组件）时才需要占位 npm 包名；届时再评估（当前只能占用 `@cthun/*` 作用域）。
- 待办：代码签名方案（避免 SmartScreen 拦截）与自动更新通道，属于发布里程碑。

---

### 7.9 原生视口的坐标契约（2026-09-19 真机血泪）

> 完整推导、证据与验证方法见 **`docs/native-viewport-coordinate-guide.md`**（Astro 的报告）。
> 这一节只留**必须记住的结论**，防止重犯。

**事故**：spike 把 Windows 的**显示缩放**当成 WebView 的 CSS 像素比例。本机显示缩放 125%、
辅助功能文字大小 110% → WebView2 的有效比例是 **1.25 × 1.10 = 1.375**，而旧代码只取了
`scale_factor() = 1.25` → 洞口、命中测试、拖动、缩放锚点**共用错比例** → 灰块、裁剪边界、
鼠标集体错位。**这不是「差一个标题栏」也不是「减固定偏移」能修的**：比例错造成的偏差随位置增大，
固定平移只能碰巧修好一个点。

**四条铁律**：

1. **四种量不能混用**：

   | 量 | 来源 | 用途 |
   | --- | --- | --- |
   | native scale | Tauri `scale_factor()` / 显示器 DPI | 原生窗口逻辑尺寸、环境诊断 |
   | **WebView DPR** | `window.devicePixelRatio` | **DOM client/CSS → surface 物理像素** |
   | DOM viewport | `window.innerWidth/innerHeight` | WebView 实际 CSS 视口（别拿 native 尺寸÷native scale 冒充） |
   | image zoom | Rust `Viewport.zoom` | 图像像素 → 物理像素（1:1 永远是 1.0） |

2. **前端只上报原始事实**（`getBoundingClientRect()`、`clientX/Y`、CSS 位移、DPR）——
   **物理换算全部在 Rust**。上报 DPR ≠ 把变换数学搬到前端。
3. **DPR 必须读运行时值**：`devicePixelRatio` 已经含显示器 DPI + 系统文字缩放 + 页面缩放，
   不要写死乘 1.1，也**不许**用「强制文字缩放 100%」来掩盖问题（那正是本次 bug 的成因）。
4. **别用 `screenX/Y × dpr` 猜容器偏移**：屏幕坐标、窗口外框、客户区、混合 DPI 桌面不是同一坐标域。

**诊断红旗（这些「通过」全是假的）**：「物理洞口 = CSS × 左栏 DPR」（程序用了自己的 DPR，
证明不了 DPR 对）；「CSS = 客户区 ÷ native scale」（算出来的，不是量的）；
「数学往返误差 = 0」（只证明互逆，**共同用错单位也是 0**）；「命中中心 ✅」（只证明自洽）；
节流上报必须**保留尾样本**（停手那一次也要发）。

**变化要收口**：CSS 洞口是布局真相，DPR 变了就**按新 DPR 重建**物理洞口（别反复缩放上次整数化过的矩形）；
跨屏/过渡用**带递增 revision 的整包布局事务**，别让旧输入套用新布局。

**验证顺序（便宜→贵）**：纯状态单测（把 OS 1.25 与 WebView 1.375 当**不同输入**，别只测两者相等的档）→
IPC 单测（用**真实字段名**反序列化；缺 DPR 必须报错，不许静默回退）→
离屏 GPU 像素回读（**期望值必须外部给定**，别让被测函数自己生成）→ 构建核对 →
**最短真机门槛**（复位 → 1:1 → 十字缩放，这三步不过就停，别做完 25 分钟全套才发现第一步就错）。

**工程侧的血（同样记牢）**：

- **wgpu 的 `BindGroupLayout` 与设备绑定**：设备重建后**必须重建 layout**，跨设备复用会在
  `create_bind_group` 抛校验错 —— 在 spike 里表现为 **panic 打死渲染线程**：界面照旧响应、
  图永远冻住（逐字日志见 `implementations/2026-09-19_windows-spike-verified-and-device-loss.md` §3.4）。
  → **编辑模块的渲染线程必须「捕获 panic + 重启 + 上报」**。
- **`Surface::configure()` 返回 `()`**：失败只能靠 `push_error_scope` 或 panic 看见。
- **`device.destroy()` 之后** surface 的 presentation 仍指向已销毁设备 → 下次取帧必报 `Validation`
  （`wgpu-core` 的 `present.rs:168` `check_is_valid()`）—— 这是「演练丢失」应有的样子，**不是 bug**。
- **日志**：本仓曾经**没装任何 logger**（wgpu 的 `log::*` 全被丢弃），所以「去看 wgpu 报错日志」
  是张空头支票。要原文就得装 logger（`RUST_LOG`）或走 error scope。
- **`WSLENV`**：WSL→Windows **只转发 `WSLENV` 里列出的变量**（§8 第 2 条）。
  `WGPU_BACKEND=dx12 pnpm spike:win` 曾经是假的（变量被丢掉，窗口照旧跑 Vulkan）；
  脚本已把 `WGPU_BACKEND` 并进 `WSLENV`。

---

## 8. 构建与运行命令（已实测，2026-09-15）

**2026-09-29 发行脚本更新**：`pnpm release … --win-msi` / `--win-nsis`（可组合）由 WSL 构建前端，再同步到探测到的 Windows 本地镜像执行 Cargo/Tauri；Windows 无需 Node。路径统一经 `scripts/lib/windows-paths.mjs` + `wslpath` 转换并回译核对，不猜盘符/挂载前缀；镜像/target 可用 `--win-dir` 或 `RAYBEND_WIN_BUILD_DIR` 覆盖。仍复用 WSLENV 与 dav1d 模块。debug/spike 命令不改。使用及验证边界见 `docs/release.md`、`specs/release-windows-msi.md`。

```bash
# —— WSL 侧（日常开发）——
pnpm install
pnpm tauri dev            # Linux/webkit2gtk 版窗口（经 WSLg 显示）
cargo check --workspace   # Rust 侧快速检查（WSL 侧 target/）

# —— 前端质量门（改完就跑这几条；CI 尚未建立，先靠习惯）——
pnpm typecheck            # 类型检查
pnpm test                 # 单元测试（node --test，零依赖，应当保持在秒级）
pnpm lint:colors          # 色值只允许出现在 tokens.css
pnpm lint:arch            # 分层依赖方向（ARCHITECTURE.md §1/§2）
pnpm lint:i18n            # 界面文案只允许出现在语言包（DESIGN.md §11.1）
pnpm build                # 生产构建

# —— 运行时冒烟（需另一个终端先 `pnpm dev`）——
pnpm smoke:ui                  # 默认打应用外壳；也可传 URL
pnpm smoke:ui http://localhost:1420/dev/kitchen-sink

# —— 发版（只准备产物；**不** commit / tag / push，那些由人做）——
pnpm release test              # 自测包（版本号不变）
pnpm release patch --dry-run   # 先看计划

# —— Windows 侧（真实产品环境：WebView2）——
# 前端在 WSL 构建，Windows 只跑 Rust，不需要在 Windows 装 Node。
#
# ⚠️ 日常就用 `pnpm debug:win`（脚本已封装下面这几条，含 dav1d 环境变量）——
#    下面这份是**排障时对照用的原命令**。
pnpm build                              # ① WSL 里产出 dist/
export CARGO_TARGET_DIR='C:\rb-target\raybend'
# ② dav1d（AVIF 解码）静态库位置 —— 没这几条 Windows 侧 cargo 会在 dav1d-sys 报「找不到 dav1d」。
#    库由 scripts/build-dav1d-win.cmd 一次性构建（换机器/换盘才需要再跑），默认 C:\rb-deps\dav1d-1.5.0。
export SYSTEM_DEPS_DAV1D_NO_PKG_CONFIG=1
export SYSTEM_DEPS_DAV1D_LIB=dav1d
export SYSTEM_DEPS_DAV1D_LINK=static
export SYSTEM_DEPS_DAV1D_SEARCH_NATIVE='C:\rb-deps\dav1d-1.5.0\lib'
export SYSTEM_DEPS_DAV1D_INCLUDE='C:\rb-deps\dav1d-1.5.0\include'
export WSLENV='CARGO_TARGET_DIR:SYSTEM_DEPS_DAV1D_NO_PKG_CONFIG:SYSTEM_DEPS_DAV1D_LIB:SYSTEM_DEPS_DAV1D_LINK:SYSTEM_DEPS_DAV1D_SEARCH_NATIVE:SYSTEM_DEPS_DAV1D_INCLUDE'
# ③ 跨 WSL→Windows 透传环境变量（cmd 的 set 经互操作不可靠）
cmd.exe /c 'pushd \\wsl.localhost\Ubuntu-24.04\home\andares\repos\c-thun\raybend & cargo build -p raybend-desktop -p raybend --features custom-protocol'
/mnt/c/rb-target/raybend/debug/raybend-desktop.exe   # ④ 运行（产物在 C: 本地）
```

**十 条硬规矩（实测踩坑）：**

1. **Windows 构建的产物必须落在 Windows 本地盘**（debug 默认 `C:\rb-target\...`；release 自动探测 `%LOCALAPPDATA%\raybend\build`，可用 `--win-dir` 指定）。9p 共享（`\\wsl.localhost`）不支持 rustc 增量编译的锁文件语义，会报 `os error -2147024895`，且会把 Windows 产物污染进 WSL 的 `target/`。
2. **跨 WSL→Windows 传环境变量用 `WSLENV`**，不要用 cmd 的 `set VAR=x & ...`（`&` 前的空格会进值，且引号经互操作会丢）。
3. 前端改动后必须重新 `pnpm build`（dist 是编译期嵌入的）；Rust 改动则只需重跑 cargo。
4. **`--features custom-protocol` 必须加，否则 Windows 版会白屏/页面打不开**。
5. **启动 Windows 的 exe 直接用 `/mnt/c/...` 路径跑，不要经 `cmd.exe /c "start \"标题\" 路径"`**。
   实测：那种写法的引号会被 WSL→cmd 的互操作吃掉，结果弹出 **「Windows 找不到文件 '\RayBend\'」**——
   参数被切碎，一个标题字串被当成了文件路径。直接跑最稳：

   ```bash
   (cd /mnt/c/rb-target/raybend/debug && ./raybend-desktop.exe >/dev/null 2>&1 &)
   ```

   （若确实需要用 `start`，就得接受这层引号嵌套很难写对；直接执行免去全部转义问题。）

6. **`src-tauri/tauri.linux.conf.json` 是开发期便利，不是最终形态**（M1-4 加的）：
   它保留系统标题栏，只为 WSLg 下还能拖边缩放（产品在 Windows 上是沉浸式 `decorations: false`）。
   **发布 Linux 版之前必须处理**（`memory/FUTURE.md` G11）。
   另：Tauri 的平台配置合并是 **RFC 7396 merge patch** —— **数组会被整体替换**，
   所以那个文件必须重复整份窗口配置，改主配置的窗口尺寸时别忘了同步它。

7. **Windows 构建必须带上 `-p raybend`**（2026-09-24 教训）：`raybend-raw-worker` 是
   `raybend` 包里的 **bin 目标**，只选 `raybend-desktop` 时**根本不会被构建** ——
   于是「主程序是新的、worker 是旧的」，而旧 worker 不认新协议：
   表现是**编辑器永远卡在「正在载入照片」**（那次卡了整整一轮人工测试）。
   现在 `pnpm check:win` 会核对 worker 的**内容**（二进制里必须有当前 `PROTOCOL_TAG`）
   与时间（不比 Rust 源码旧），过期直接报错。
   另外主程序与 worker 之间还有**协议版本握手**：对不上会当面报错并给出重建命令，
   不会再静默失败。
8. **先 `pnpm build` 再构建 Windows，构建完必须 `pnpm check:win`**（2026-09-16 血的教训）：
   `dist/` 是**编译期嵌进 exe** 的，改了前端不重建 dist，产物里就是旧界面 ——
   而当时「exe 里有资源名」的核对是**假绿**（dist 自己旧，旧名字当然对得上），
   结果让人类拿着「没有修复的版本」白测一轮。
   `pnpm check:win` 既比**时间**（exe 必须比 dist 新）也比**内容**（资源名逐个命中），
   不合格会直接打印补救命令。
9. **dav1d 静态库是 Windows 构建的前置**（2026-09-24，AVIF 解码）：   `image` 的 `avif-native` 拉进 `dav1d-sys`，它用 `system-deps` 找 dav1d 库；
   Windows 上没有 pkg-config，所以**位置靠环境变量告诉它**（见上面的原命令）。
   * 库不在仓库里，用 **`scripts/build-dav1d-win.cmd`** 一次性构建
     （meson + ninja + nasm + VS Build Tools，约 3 分钟，产物 `C:\rb-deps\dav1d-1.5.0\`）；
   * 环境变量已封进 **`scripts/lib/dav1d-win.mjs`**，`pnpm debug:win` / `pnpm spike:win`
     会自动带上 —— **别在别处再写一份**（`AGENTS.md` §2.12）；
   * 链的是**静态库**（`SYSTEM_DEPS_DAV1D_LINK=static`）⇒ 安装包里不需要带 `dav1d.dll`；
   * WSL 侧不同：用系统库（`sudo apt install libdav1d-dev`，1.4.1）走 pkg-config，**不需要**这些变量；
   * 许可与版本登记在 `THIRD-PARTY-NOTICES.md`。
10. **target 目录会只进不出，靠 `pnpm clean:win` / `clean:wsl` 自然清**（2026-09-25）：
    cargo **从不回收**「不再是当前构建图一部分」的产物 —— 每换一次依赖版本 / feature 组合 /
    构建参数就多留一份（实测：一个 target 目录攒到 11.6 GiB，`deps/` 里 `libtoml` 11 份、
    `incremental/` 里 `raybend_desktop_lib-*` 10 个不同 unit-hash 目录；WSL 侧一次就清出 15 GB）。
    * `pnpm debug:win` 的第 ④ 步会自动带上（复用该次构建的 `--message-format=json` 单元清单，
      **不额外编译**）；想单独打就 `pnpm clean:win` / `pnpm clean:wsl`；
    * 判据是**权威的**：cargo 的 JSON 输出会把构建图里**每个单元**都报出来（已最新的也报，带
      `fresh:true`），「不在图里 **且** 已凉 ≥ `--keep-days`（默认 3 天）」才删 ——
      宽限期是为了不碰**另一个会话正在用的另一套构建参数**产出的东西；
    * `.fingerprint/` 不碰（只几十 MB，删错会让 cargo 白重编）；
    * 先看后删：`pnpm clean:win --dry-run`（会列出最占空间的几项）。
    * 什么时候仍需 `cargo clean`：规则/目录结构大改、或想彻底重来 —— 代价是一次冷构建。
    * **2026-10-05 扩充**：① 清理范围新盖 `examples/`（与 deps 同一套主干认领，含 rustc
      中断残留的 `rustcXXXX` 临时目录；实测这里能积 43 GB）；② 清单只在没显式给 `--json`
      时**每次重建**（旧 `last-build.json` 会被当作权威，拿 10 天前的图判活会误判）；
      ③ WSL 侧清单 = **两段拼接**（`build --lib --bins` + `test --lib --bins --tests
      --no-run`，cfg(test) 是另一套单元）；**不含 examples** —— `cargo test` / `--all-targets`
      会重建它们，每个 ~200MB 调试信息，27 个一套 22 GB，只靠 3 天宽限期幸存；一次性
      深清用 `--keep-days 0`（WSL 侧一次回收 41 GB：99 GB → 14 GB）。
      剩余结构性选项（未采纳，待拍板）：`[profile.dev] debug = "line-tables-only"`、
      `[profile.dev.package."*"] debug = false`（体积砍 3–5 倍，代价是调试器变量信息）。
      另：`-p raybend` 与 `--workspace` 两种调用会各养一套依赖变体（实测差 107 个单元），
      尽量用一致的构建调用。

### 8.1 为什么必须加 `--features custom-protocol`（重要，别拆掉）

已定位到源码级。`tauri` 的 `build.rs`：

```rust
let custom_protocol = has_feature("custom-protocol");
let dev = !custom_protocol;        // ← dev 由 feature 决定，不是 debug/release！
```

而 `tauri-codegen` 在 `dev == true` 且配置了 `devUrl` 时，**产出的嵌入资源是空的**：

```rust
} else if dev && config.build.dev_url.is_some() {
    let assets = EmbeddedAssets::default();   // ← 空资源
```

于是 webview 只能去连 `devUrl`（`http://localhost:1420`）—— 本机没服务时就报「无法访问此页面」。

**表现对照**：

| 路径 | 命令 | 结果 |
| --- | --- | --- |
| WSL 开发 | `pnpm tauri dev` | ✅ 正常（Vite 在 1420 上服务，且 CLI 不加该 feature → `dev=true` 是对的） |
| WSL 开发 | `cargo run` | 同上（**也会去连 1420**，所以不走这条） |
| Windows 裸 `cargo build` | 无 feature | ❌ 窗口出来了但页面打不开 |
| Windows 生产 | `--features custom-protocol` | ✅ 嵌入真实资源 |

> Tauri CLI 的 `tauri build` 会自动加这个 feature；**但我们的 Windows 构建路径不走 CLI**，必须手写。
> `src-tauri/Cargo.toml` 的 `[features]` 段里已注明这一点 —— **不要当模板残留删掉**
> （M0-1 “删净模板演示”时曾把它误删，直接导致了这个 bug）。

### 8.2 Windows 侧验证纪律

**“窗口出现了”不等于“功能对了”。** Windows 侧跑完必须至少确认：

1. 页面**真的画出来了**（不是白屏/错误页）—— **由人类目视**（`AGENTS.md` §2.8）
2. 产物**比 `dist/` 新**（否则跑的可能是旧前端）
3. Agent 侧可做的程序化冒烟：检查 exe 里含的是 **`dist/assets/` 当前的资源文件名**
   （含则说明资源确实被嵌入；内容字节是 brotli 压缩的，搜原始字符串搜不到是正常的）
4. **「进程起来了」不等于「功能对了」，而「主程序对了」也不等于「它的子进程对了」**
   （2026-09-24）：RAW 解码跑在**独立进程**里，主程序重建不会顺带重建它。
   凡是「主程序 + 伙伴二进制」的形态，验证清单里都要有**伙伴的那一份**。
5. **换了图标后，「看起来没换」多半是 Windows 的图标缓存，不是产物没换**（2026-09-17 实测）：
   同一个路径的 exe 被覆盖时，资源管理器/任务栏会继续显示旧图标。
   判定要用**证据**，别用眼睛：
   - 文件图标 = exe 的 `RT_ICON` 资源（6 张，与 `src-tauri/icons/icon.ico` 逐字节比对）；
   - 窗口/任务栏图标 = `icons/icon.ico` 的**第 1 个条目**（tauri-codegen 取 `entries()[0]`）解码成 RGBA 后的字节；
   - 想「眼见为实」就把 exe 复制成**新文件名**（新路径不撞缓存），或清缓存：`ie4uinit.exe -show`、
     删 `%LOCALAPPDATA%\Microsoft\Windows\Explorer\iconcache_*.db` 后重启资源管理器。
   已固定的任务栏图钉把图标缓存在 `.lnk` 里，要**取消固定再固定**。

### 8.3 本次「吃一堑」汇总（2026-09-15）

| # | 坑 | 后果 | 教训 |
| --- | --- | --- | --- |
| 1 | M0-1 清理模板时把 `[features] custom-protocol` 当成模板残留删了 | **Windows 版一直是白屏**，而 WSL 侧因为走 `pnpm tauri dev` 完全正常，所以问题被掩盖了很久 | **「模板自带」不等于「模板残留」**。删配置性代码前先搞清它的用途；`tauri build` 才注入的东西，我们走裸 cargo 就必须自己写明 |
| 2 | 验证只看 `MainWindowHandle` / `MainWindowTitle` | 声称「Windows 运行验证通过」，而实际页面根本打不开 | **“窗口出现了”不等于“功能对了”**（§2.8）。验证清单必须包含「内容看得见」 |
| 3 | 用 `cmd.exe /c "start \"标题\" 路径"` 启动 exe | 弹「找不到文件 `\RayBend\`」，干扰用户 | WSL→Windows 的**引号嵌套不可靠**；直接用 `/mnt/c/...` 跑 |
| 4 | exe（02:50）比 `dist/`（10:28）旧 | 即使逻辑正确，跑的也是旧前端 | **产物时间戳必须晚于 `dist/`**，构建前先 `pnpm build` |

实测参考：首次 Windows 全量构建约 3–4 分钟；WSL 侧 `cargo check` 首次约 2–3 分钟。

---

### 8.4 可选 AI 的构建输入（2026-10-04）

模型制作归独立 `model-registry`，软件构建只读取 Git 忽略的 `ai-model-source.local.json`。编码器通用 artifact 和 RayBend 标签 profile 分开，三级定位+完整摘要，按现有可信 manifest 准入。

`ai:export` 负责显式 CPU 制作、Git 库的受限自动提交和成功后的原子源登记；`ai:use` 只校验并登记。普通 debug/release/Tauri 构建复用 `scripts/lib/ai-build.mjs`；auto 输入不可用时提示基础版，required 构建前失败，off 跳过源检查。CPU ORT 固定官方 ZIP/摘要有缓存；普通构建不下载权重、不导出 ONNX、不改模型 Git。

同一个冻结计划控制 Vite capability、Rust `photo-ai-runtime`、Tauri 资源和 `raybend-ai-build.json`。裸 cargo 默认无推理依赖。`check:win` 按产物记录与原生编译标记核对，禁止按当前模型源猜测旧 exe 能力；关闭 AI 要清理构建输出的 ai-model/ai-runtime。独立 bundle 需与先前 build 记录一致。

详细协议与环境检查见 `specs/ai-model-library-build.md`，构建/制作使用见 `docs/ai/tinyclip-v1/README.md`。升级基础版保留既有标签/XMP/人工纠错；安装器切换与干净机 CPU 依赖仍由崔总验收。

## 9. Solid：包装组件透传 children —— `untrack` 必须写在「插入点」

> 人类 2026-09-23 定：「那个提示要并行说明，这两者的教训」。同一个错误手法一天里各踩一次。
> 规矩的**一句话版**在 本文件 §9：壳组件一律在插入点 `{untrack(() => props.children)}`，禁止提到组件 body 里。


同一个手法有**两面**，2026-09-23 一天里各踩一次 —— 先踩 A 面，修 A 面时又踩 B 面：

- **A 面：不 `untrack`（原样写 `{props.children}`）**
  → `{props.children}` 会被编译成 `insert(el, () => props.children)` —— 那是一个 **render effect**，
  **不是一次性插入**；而 children 的 getter 每求值一次，里面的 `createComponent(...)` 就重跑一遍。
  于是**「创建 children 期间被读到的任何信号」都成了「重建整棵子树」的开关**。
  真凶实例：Ark 的 `SliderRoot` 用 `createSplitProps()` **同步**读走 `props.value`
  （= tiles 的 `tileStep`）→ 拖动第一格值一变 → 整条状态栏重建 → 正在拖的 Ark Slider 实例
  被换掉 → 「拖一格就断、焦点也丢」。
- **B 面：把 `untrack` 提到组件 body 里**（`const children = untrack(() => props.children)`）
  → children 在本组件返回的 `<XxxContext.Provider>` **之前**就被造出来了，而 Solid 的
  `useContext` 走的是**「创建时的 owner 链」** —— 子组件于是落在 Provider **外面**，
  拿到 `undefined`。真凶实例：`TilesShell` 的 children（网格）被提前造 →
  `PhotoGrid` 的 `useTilesFitRequest()` 拿到 `undefined` → **「横向适合窗口」按钮按了毫无反应**
  （当时正治着拖动，于是变成「修好一个、按死另一个」）。
- **✅ 正解：在插入点就地 untrack** —— `{untrack(() => props.children)}`（就写在 JSX 里那一行）。
  两个毛病同时消失：重建开关没了（A 面），children 仍在 Provider 的 owner 链里（B 面）。
- **规矩**：凡是把 children 透传出去的壳组件（`BarFrame` / `TilesShell` 这类），
  **一律在插入点 untrack 一次**；**禁止**提到组件 body 里提前求值。
- **两面必须成对验**：只验一边就会出现「按下葫芦浮起瓢」。现成回归在
  `scripts/check-browse-boot.mjs`：
  * 「缩放滑块拖动」——断言**一路跟手** + 滑块/状态栏/网格**节点身份不变**（A 面）；
  * 「横向适合窗口」——断言点击后**当前行真的铺满**（= 网格确实收到了 fit 请求，B 面）。
  两条各自去掉对应的一半就立刻报红（已反面验证）。
- **判据不能只看「值变了没有」**：重建 + 拿新值重画一遍也会让第一步的值变（假绿）。
  要断言**节点身份**（`el === 上次那个 el`），并把网格/列表一起钉住 ——
  它们被重建就意味着滚动位置与选择丢了。
- **诊断手法**（下次别再从零查）：给 `Node.prototype.insertBefore/appendChild/replaceChild`
  挂钩子，只记录「插入目标在关注的容器内」的那些调用并打印 `new Error().stack`；
  再配合 `element === 上次那个` 的身份比对，就能定位到是哪一层被重建。
  B 面更快：在子组件里读一次那个 context，把结果（`yes` / `no`）写到 DOM 属性上看一眼。

---

## 10. 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-09-15 | 初版：分层与依赖方向、一级模块清单（对应 `memory/DESIGN.md` §10.2）、状态归属、两条可执行检查 |
| 2026-09-17 | 新增**素材层 `src/assets/`**（纯数据，任何层可 import；不带方向约束）—— 品牌资产入场时 `shell → app` 的误报暴露了它原本无处可归 |
| 2026-09-17 | 补 M2-W1 的模块落点：浏览（`features/browse` + `workspaces/browse`）、渲染（`crates/raybend/src/render`）、库目录操作（`repo/dirs` + `src-tauri/dirs`）、spike 窗口与诊断页 |
| 2026-09-27 | 由 AGENTS.md 瘦身迁入并合并：版本基线（§5）、架构决定（§6）、调研存档（§7）、构建与排障（§8）、untrack 纪律（§9）；章节号 6.x/7.x 沿用 AGENTS 原编号 |
