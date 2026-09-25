# raybend 前端架构（ARCHITECTURE.md）

> 本文件规定**代码分层与依赖方向**，是前端部分的事实来源。
> 相关：`AGENTS.md`（项目纪律）、`DESIGN.md`（视觉与令牌）、`design/main.md`（界面设计）。
>
> 建立时间：2026-09-15 ｜ 适用于 M1 起的所有前端工作

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
| `src/styles/` | CSS 变量、`@theme` 映射、全局滚动条与动效 | 禁止写组件样式；色值只在这里（`tokens.css`）与 `DESIGN.md` |
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

## 2. 一级模块清单（与 `DESIGN.md` §10.2 的对应）

`DESIGN.md` §10.2 叫它们「第二批：大组件」，在本文件里它们的落点是**模块**：

| `DESIGN.md` §10.2 | 代码位置 | 归属工作单元 |
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

## 3. 状态归属

| 状态 | 归属 | 理由 |
| --- | --- | --- |
| 主题、密度、语言 | `src/lib/appearance.ts` + 组装层 Provider | 全局、设备级偏好 |
| 当前工作流（导入/浏览/编辑/导出） | `src/shell/` 的 store | 外壳自己的状态 |
| 当前仓、当前选中目录 | **app store**（`src/App.tsx` 提供） | 跨模块共享，且 `DESIGN.md` §12.4.1 明确要求跨面板同步 |
| 模块内部（Recent 列表、树的展开集合、已选目录集合…） | 该 feature 自己的 store | 换模块时整块丢弃，不需要全局清理 |

**展开状态与选中状态必须分开存**（`DESIGN.md` §12.4.1）：
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

## 5. 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-09-15 | 初版：分层与依赖方向、一级模块清单（对应 `DESIGN.md` §10.2）、状态归属、两条可执行检查 |
| 2026-09-17 | 新增**素材层 `src/assets/`**（纯数据，任何层可 import；不带方向约束）—— 品牌资产入场时 `shell → app` 的误报暴露了它原本无处可归 |
| 2026-09-17 | 补 M2-W1 的模块落点：浏览（`features/browse` + `workspaces/browse`）、渲染（`crates/raybend/src/render`）、库目录操作（`repo/dirs` + `src-tauri/dirs`）、spike 窗口与诊断页 |
