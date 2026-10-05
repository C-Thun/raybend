完成时间：2026-10-04 01:43:21 +0800

# Editor Presets 实现检查

## 范围与结论

对当前工作区进行代码检查，依据 `specs/editor-presets.md`、需求原文、`design/editor.md` §3.10，以及 `2026-09-30_editor-presets.md` 与 `2026-09-30_recent-update-audit.md` 两份实施记录。核对了面板、store、IPC、app.db 迁移、LUT 解析、编辑提交和 XMP 写出。

本轮为检查，没有修改业务代码、设计或已有工作区变更。以下发现仍未修复。临时复现只使用内存 SQLite 和 /tmp 文件，没有连接实际照片库。

## 主要发现

### 1. [P1] LUT 在进入编辑器后丢失时，预设仍覆盖当前 LUT，保底失效

位置：`src/features/editor/store.ts:804`、`src/workspaces/editor/EditorWorkspace.tsx:227`、`src-tauri/src/editor.rs:1248`、`src-tauri/src/lut.rs:44`。

预设的 LUT 存在性只根据 store 中的 `available` 判断；工作区进入时读取一次 LUT 库，应用预设前不重查文件。如果那时文件还在、之后从磁盘删除，旧的 `available:true` 会让预设覆盖当前 LUT。该 LUT 未命中渲染缓存时，Rust `resolve` 报错并向上返回，整份显影参数被拒，画面停留在最后一帧。编辑提交只校验 LUT ID 的形状，仍可以把失效引用存进 latest；预览刷新也会因解析失败而失败。

这是“LUT 不存在时忽略该项、不动当前 LUT”的遗漏。不能通过全局吞掉所有 LUT 错误修复，因为既有 issue/latest 另有缺失资源报错契约；应在预设应用路径用实际资源状态作判定，并考虑检查后文件又消失的竞态。

证据：实际 store 复现中旧 LUT 为 `old`，库缓存把 `lost` 标为可用，应用后 LUT 变成 `lost`；Rust 的严格失败路径已逐行核对。已标记 `available:false` 的 LUT 可以正常忽略。未在真机删除文件做 GUI 验收。

### 2. [P2] 曲线与预设没有同高容器，预设树也未获得有界的内部滚动区

位置：`src/features/editor/panels.tsx:324`、`src/features/editor/preset-panel.tsx:322`、`src/features/editor/CurveEditor.tsx:183`。

第 3 组用 Show 分别挂载曲线与预设，外层只有自然高度。预设树只设置 `min-height:224px` 与 `overflow-y:auto`，没有 height/max-height 或一个提供有限高度的父容器，内容增加时会向外长大，滚动主要落到整个右栏。自动滚动代码修改的是树自身的 scrollTop，因此大量目录时无法保证其工作。

曲线本体当前是占满内容宽的 `aspect-square`，另有通道行与 RAW 基础曲线块；原报告用于估算的“CurveEditor 130”已不能解释当前代码。最小高度也不能满足明确要求的“切换不伸缩”。应由同一个内容容器约束两页签高度，树在其中 flex 填充并内部滚动。

此项是代码与布局约束检查结论，没有测量 Windows GUI 的实际尺寸和 DPI。

### 3. [P2] 刷新整库时不清理失效选中 ID

位置：`src/features/editor/store.ts:769`、`src/features/editor/preset-panel.tsx:119`、`src/features/editor/preset-panel.tsx:253`。

先 Shift 多选 p1、p2，再删除 p2，`setPresetLibrary` 只替换列表，selection 仍是 `[p1,p2]`。界面只剩 p1 高亮，但应用按钮按 ids.length 判断而保持禁用。随后拖动 p1 还会把已经删除的 p2 当作移动成员；不存在项的 directoryId 是 undefined，与目标目录不等，因此没有被过滤，可能在前一项已移动后报错。

实际 store 复现已确认：库只剩 p1，而 selection 保留 p1、p2。库刷新应按有效 ID 收敛 selection，并清理失效目录的展开记录；拖动落点也要排除不存在条目。

### 4. [P2] 移动时自动加后缀突破名称上限，合法预设可能从界面消失

位置：`crates/raybend/src/store/presets.rs:257`、`src/lib/presets.ts:196`。

创建限制为最多 80 个 Unicode 字符。移动重名时直接加 ` 2` 等后缀，没有按字符给后缀预留空间。80 个 emoji 的名称创建合法，前端 UTF-16 length 为 160，也能显示；一次重名移动变为 82 个 Unicode 字符、162 个 UTF-16 单元，超过前端清洗层的 160，整条记录被丢弃。

实际当前 Rust 源码在内存 SQLite 中已复现移动后名称长度为 82；实际 TS 清洗函数已复现显示记录从 1 变 0。记录没有从数据库删除，非空目录仍受删除保护，界面却无法对该记录操作。ASCII/中文最长名同样违反后端原定上限，只是通常不会立即触及前端 160 的阈值。

应统一 Unicode 字符计数，并在生成重名后缀时截短基名，保证最终名称仍在上限内。

## 其它问题与风险

### [P2] 整库响应没有乱序保护

位置：`src/workspaces/editor/EditorWorkspace.tsx:233`、`src/workspaces/editor/EditorWorkspace.tsx:267`。

初次读取、新建、删除、移动都无条件用响应替换整库。连续 Shift 删除两项、或初次读取与修改交错时，只要旧快照响应较晚，就会把已删除条目重新显示。仅移动批次内部串行，不能保护其它操作。需要复用现有串行任务机制或带版本的库响应，保证刷新不会回退。

代码路径已确认；提取实际回调并注入乱序响应的复现脚本已写到 /tmp，但执行前命令工具启动失败，本轮没有取得该新增脚本的运行结果。因此未把它计入上面的动态复现项。

### 后端 payload 缺少约定的 version === 1 校验

位置：`crates/raybend/src/store/presets.rs:47`、`src/lib/presets.ts:131`。

规格要求基本校验包含 version===1，后端当前只要求 JSON 对象。`{"version":2,"tone":{}}` 可以写入并返回成功，前端却整行过滤；`{}` 同样可以成为不可见记录，并占住同名/目录非空约束。当前 Rust 源码的内存 SQLite 复现已确认 version 2 被接受。

坏 JSON 读取跳行也没有规格要求的 warning，不利于发现“目录显示空但后端拒绝删除”的原因。应明确支持版本、拒绝无法使用的新载荷，并对已有坏行提供诊断；不要静默删记录。

### 新建失败一律显示“同名已存在”

位置：`src/features/editor/preset-panel.tsx:142`、`src/features/editor/preset-panel.tsx:160`、`src/workspaces/editor/EditorWorkspace.tsx:252`、`src/workspaces/editor/EditorWorkspace.tsx:265`。

名称输入没有前端长度/控制字符校验。任意后端失败都返回 false，面板随即把它当重名。普通粘贴 41 字目录名或 81 字预设名，会看到错误的重名提示；真正错误写到面板、位于模态框之后。应复用既有名称校验口径，并区分重名和其它错误，把实际失败原因显示在当前弹窗。

### [P3] 包含镜头组时 rev 抬两次

位置：`src/features/editor/store.ts:816`、`src/features/editor/store.ts:388`、`src/features/editor/store.ts:822`。

`updateLensSide` 自己 bump，应用预设末尾又 bump。实际复现 delta=2，与实施报告“只 bump 一次”不符。batch 和现有提交入口使本轮没有证据证明它会产生两次 DB 提交，故只列为低优先级实现一致性问题。

## 已核对正常的部分

- 默认目录保护、空目录删除保护、Unicode 重名检查、六个 IPC 注册及 app v8 迁移均存在。
- 快照按 PARAMS 派生各组参数，包含默认值；普通完整快照应用和未保存大类保留通过实际 store 复现。
- 基础曲线、几何、编辑来源与 as-shot 元数据没有被预设覆盖；色温保存的是当前生效值。
- 已知缺失 LUT 可忽略，lut.id=null 可清除，用户曲线四通道按现有契约处理。
- Escape 取消、卸载清理与拖拽后 click 屏蔽已在后续检查中补入，不再是当前遗漏。
- 命令体系：规格与实施报告已说明面板上下文动作不登记全局命令、无默认热键，属于明确决定。
- XMP：预设库本身是设备级资产；应用后仍走 commitDevelop → develop_commit → sidecar::queue_sync，未发现预设绕过已有 XMP 镜像提交链。未进行真实 sidecar 写出/读回验收。
- 色彩管理可选组是 CM-W4 中明确尚未接线的后续范围，本轮不把它当作六组预设的遗漏。FINISHED.md 中 app.db.settings 的“编辑器预设”是在外部应用选择语境下，不能用于判断本功能的两表存储错误。

## 验证

已完成：

- `cargo test -p raybend store::presets --lib --offline`：8 条通过；有一条既有 unused_mut warning。
- 当前运行时显式 import `src/lib/presets.test.ts`、`src/lib/pointer-drag.test.ts`、`src/features/editor/store.test.ts`：44 条测试通过。
- `./node_modules/.bin/tsc --noEmit`：退出码 0。
- 三项现有检查脚本：architecture、i18n、hardcoded-colors 均通过。
- TS 临时探针：失效选中、最长 emoji 移动后过滤、镜头 rev=2、缓存 LUT 替换、普通快照应用均取得运行结果。
- Rust 临时探针用 #[path] 编译当前 store/presets.rs，直接调用其 CRUD 和实际 app_0008 schema：最长名称后缀与错误版本接纳取得运行结果。只适配错误类型，没有复制 CRUD 逻辑。第一次独立 rustc 编译因选到不同 serde_core 编译单元失败；按现有 fingerprint 选择匹配依赖后通过，这是探针构建问题。

工具限制：

- `pnpm exec` 启动器因受限网络不能验证 pnpm@12.3.4 的 registry signature，报 ERR_PNPM_PNPM_ENGINE_IDENTITY_UNVERIFIABLE。按既有实施报告的方式直接使用仓库已安装工具，未安装依赖或修改包管理配置/锁文件。
- 当前 Node 26.5.0 直接 `node --test <ts files>` 只给出了文件级成功，没有显示各内部测试；使用显式 import 实际执行并确认 44 条测试。未把文件级成功当成 44 条测试证据。
- 收尾时 exec_command 对默认 shell、/bin/bash、/usr/bin/bash 及仓库与 /tmp 两个 cwd 均返回 CreateProcess / No such file or directory。备用 node_repl 也因 sandboxCwd 本地 URI 校验失败不可用。此前文件读取、Cargo 测试、TS 探针、tsc 和 lint 均成功，因此这里只能定位为本会话工具启动异常，不能据此判断 WSL 本身损坏。
- 最后追加的乱序响应与额外断言尚未执行；没有把它们写成通过。完整 GUI、真实照片库、DPI、拖拽手感与磁盘变化后的画面需崔总在真实环境确认。

## 测试遗漏

当前 presets.test.ts 覆盖纯 payload/计划，store.test.ts 未覆盖预设专用的刷新选中、应用 rev 和多选删除路径；面板没有固定高度/内部滚动的可重复验收代码。存储测试也缺名称上限后的移动碰撞及版本契约。后续修复应同时补这些回归测试，不应只重复现有绿色测试。

